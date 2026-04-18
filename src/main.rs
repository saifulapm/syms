use std::io::{self, IsTerminal, Write};
use std::process;

use clap::Parser;

mod custom;
mod language;
mod output;
mod parser;
mod picker;
mod queries;
mod symbol;
mod walker;

const VALID_KINDS: &[&str] = &[
    "fn", "method", "class", "struct", "enum", "type", "interface", "const", "mod", "trait",
    "component", "hook", "test",
];

const BUILTIN_LANGS: &[&str] = &[
    "rs", "ts", "tsx", "js", "go", "py", "c", "cpp", "java", "rb", "php", "sh", "css", "lua",
];

const VALID_FORMATS: &[&str] = &["json", "jsonl"];

#[derive(Parser)]
#[command(
    name = "syms",
    version,
    about = "Fast project-wide symbol search using tree-sitter",
    after_help = "\x1b[1mExamples:\x1b[0m
  syms                              Search current directory
  syms src/                         Search specific directory
  syms --kind fn,method             Only functions and methods
  syms --lang rs,go --exported      Public symbols in Rust and Go
  syms --kind test                  All test functions
  syms --kind component             React components (TSX)
  syms --kind hook                  React hooks (use* prefix)
  syms -o json | jq '.[]'           JSON output piped to jq
  syms -o jsonl                     Streaming JSON (one per line)
  syms --fzf                        Interactive search with fzf
  syms --sk                         Interactive search with skim

\x1b[1mCustom languages:\x1b[0m
  Add tree-sitter grammars via ~/.config/syms/languages/<name>.toml
  See: https://github.com/saifulapm/syms#custom-languages"
)]
struct Args {
    /// Directory or file to search [default: .]
    #[arg(default_value = ".")]
    path: std::path::PathBuf,

    /// Filter by kind: fn,method,class,struct,enum,type,interface,const,mod,trait,component,hook,test
    #[arg(short, long, value_delimiter = ',')]
    kind: Vec<String>,

    /// Filter by language: rs,ts,tsx,js,go,py,c,cpp,java,rb,php,sh,css,lua (+ custom)
    #[arg(short, long, value_delimiter = ',')]
    lang: Vec<String>,

    /// Show only exported/public symbols
    #[arg(short, long)]
    exported: bool,

    /// Output format: json, jsonl
    #[arg(short = 'o', long = "output")]
    format: Option<String>,

    /// Launch fzf with tree-sitter-aware preview
    #[arg(long, conflicts_with = "sk")]
    fzf: bool,

    /// Launch skim (sk) with tree-sitter-aware preview
    #[arg(long, conflicts_with = "fzf")]
    sk: bool,
}

fn main() {
    // Load custom languages before anything else
    custom::init();

    let args = Args::parse();

    if let Err(e) = run(args) {
        if let Some(io_err) = e.downcast_ref::<io::Error>()
            && io_err.kind() == io::ErrorKind::BrokenPipe
        {
            process::exit(0);
        }
        eprintln!("syms: {e:#}");
        process::exit(1);
    }
}

fn run(mut args: Args) -> anyhow::Result<()> {
    validate_filters(&args)?;

    // If the user did not pass --lang, try to infer it from project sentinels
    // (e.g. `artisan` → php, `Cargo.toml` → rs). Explicit --lang always wins.
    if args.lang.is_empty() {
        args.lang = detect_project_langs(&args.path);
    }

    let use_picker = if args.fzf {
        Some(picker::Picker::Fzf)
    } else if args.sk {
        Some(picker::Picker::Sk)
    } else {
        None
    };

    let (tx, rx) = crossbeam_channel::unbounded();

    let path = args.path;
    let walker_handle = std::thread::spawn(move || {
        walker::walk(&path, tx);
    });

    if let Some(p) = use_picker {
        let symbols: Vec<_> = rx
            .into_iter()
            .filter(|sym| matches_filters(sym, &args.kind, &args.lang, args.exported))
            .collect();
        walker_handle.join().ok();
        picker::run(p, &symbols)?;
    } else if args.format.as_deref() == Some("json") {
        let symbols: Vec<_> = rx
            .into_iter()
            .filter(|sym| matches_filters(sym, &args.kind, &args.lang, args.exported))
            .collect();
        walker_handle.join().ok();

        let stdout = io::stdout();
        let out = io::BufWriter::new(stdout.lock());
        serde_json::to_writer(out, &symbols)?;
        println!();
    } else if args.format.as_deref() == Some("jsonl") {
        let stdout = io::stdout();
        let mut out = io::BufWriter::new(stdout.lock());

        for sym in rx {
            if !matches_filters(&sym, &args.kind, &args.lang, args.exported) {
                continue;
            }
            serde_json::to_writer(&mut out, &sym)?;
            writeln!(out)?;
        }

        out.flush()?;
        walker_handle.join().ok();
    } else {
        let use_color = io::stdout().is_terminal();
        let stdout = io::stdout();
        let mut out = io::BufWriter::new(stdout.lock());

        for sym in rx {
            if !matches_filters(&sym, &args.kind, &args.lang, args.exported) {
                continue;
            }
            output::write_symbol(&mut out, &sym, use_color)?;
        }

        out.flush()?;
        walker_handle.join().ok();
    }

    Ok(())
}

fn validate_filters(args: &Args) -> anyhow::Result<()> {
    for k in &args.kind {
        if !VALID_KINDS.contains(&k.as_str()) {
            anyhow::bail!(
                "unknown kind '{k}'. Valid kinds: {}",
                VALID_KINDS.join(", ")
            );
        }
    }
    // Language validation: check built-in + custom
    let custom_shorts: Vec<&str> = custom::registry().iter().map(|l| l.short.as_str()).collect();
    for l in &args.lang {
        if !BUILTIN_LANGS.contains(&l.as_str()) && !custom_shorts.contains(&l.as_str()) {
            let mut all_langs: Vec<&str> = BUILTIN_LANGS.to_vec();
            all_langs.extend(&custom_shorts);
            anyhow::bail!(
                "unknown language '{l}'. Valid languages: {}",
                all_langs.join(", ")
            );
        }
    }
    if let Some(fmt) = &args.format
        && !VALID_FORMATS.contains(&fmt.as_str())
    {
        anyhow::bail!(
            "unknown format '{fmt}'. Valid formats: {}",
            VALID_FORMATS.join(", ")
        );
    }
    Ok(())
}

/// Walk up from `start` looking for a project sentinel file. First match wins.
/// Returns the short-name list to use for `--lang`, or empty if nothing matched.
fn detect_project_langs(start: &std::path::Path) -> Vec<String> {
    // Ordered by specificity: Laravel wins over generic npm; composer wins over
    // a bare `package.json` (so a Laravel app with a Vite frontend auto-detects PHP).
    const SENTINELS: &[(&str, &[&str])] = &[
        ("artisan",          &["php"]),        // Laravel
        ("composer.json",    &["php"]),        // generic PHP
        ("Cargo.toml",       &["rs"]),
        ("go.mod",           &["go"]),
        ("pyproject.toml",   &["py"]),
        ("requirements.txt", &["py"]),
        ("package.json",     &["ts", "tsx"]),
    ];

    let abs = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let mut dir = if abs.is_file() { abs.parent().map(std::path::Path::to_path_buf) } else { Some(abs) };

    while let Some(d) = dir {
        for (sentinel, langs) in SENTINELS {
            if d.join(sentinel).exists() {
                return langs.iter().map(|s| (*s).to_string()).collect();
            }
        }
        dir = d.parent().map(std::path::Path::to_path_buf);
    }
    Vec::new()
}

fn matches_filters(
    sym: &symbol::Symbol,
    kind_filter: &[String],
    lang_filter: &[String],
    exported_only: bool,
) -> bool {
    if !kind_filter.is_empty() && !kind_filter.iter().any(|k| k == sym.kind.short_name()) {
        return false;
    }
    if !lang_filter.is_empty() && !lang_filter.iter().any(|l| l == sym.lang.short_name()) {
        return false;
    }
    if exported_only {
        match sym.visibility {
            Some(symbol::Visibility::Public) | None => {}
            Some(symbol::Visibility::Private) => return false,
        }
    }
    true
}
