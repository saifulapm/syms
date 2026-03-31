use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use serde::Deserialize;
use tree_sitter::Query;

use crate::queries::CompiledQuery;
use crate::symbol::SymbolKind;

/// Global registry of custom languages, initialized once at startup.
static REGISTRY: OnceLock<Vec<CustomLang>> = OnceLock::new();

/// A loaded custom language with its grammar and compiled queries.
pub struct CustomLang {
    pub name: String,
    pub short: String,
    pub extensions: Vec<String>,
    pub ts_language: tree_sitter::Language,
    pub queries: Vec<CompiledQuery>,
    // Keep the library alive so the language pointer stays valid
    _library: libloading::Library,
}

// SAFETY: tree_sitter::Language is a pointer to static data in the loaded library.
// The library is kept alive in _library field, so the pointer remains valid.
// The Library itself is Send+Sync safe once loaded.
unsafe impl Send for CustomLang {}
unsafe impl Sync for CustomLang {}

/// TOML config file format for a custom language.
#[derive(Deserialize)]
struct LangConfig {
    extensions: Vec<String>,
    parser: String,
    /// Optional: symbol name to call in the .so (default: `tree_sitter_{name}`)
    symbol: Option<String>,
    /// Optional: short name for --lang filter (default: first extension)
    short: Option<String>,
    #[serde(default)]
    queries: Vec<QueryConfig>,
}

#[derive(Deserialize)]
struct QueryConfig {
    kind: String,
    query: String,
}

/// Initialize the custom language registry by scanning config dir.
pub fn init() {
    REGISTRY.get_or_init(|| {
        let Some(config_dir) = config_dir() else {
            return Vec::new();
        };

        let Ok(entries) = std::fs::read_dir(&config_dir) else {
            return Vec::new();
        };

        let mut langs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                match load_custom_lang(&path) {
                    Ok(lang) => {
                        eprintln!("syms: loaded custom language '{}'", lang.name);
                        langs.push(lang);
                    }
                    Err(e) => {
                        eprintln!("syms: failed to load {}: {e:#}", path.display());
                    }
                }
            }
        }
        langs
    });
}

/// Get the custom languages registry.
pub fn registry() -> &'static [CustomLang] {
    REGISTRY.get().map_or(&[], Vec::as_slice)
}

/// Find a custom language by file extension. Returns the registry index.
pub fn from_extension(ext: &str) -> Option<u16> {
    registry()
        .iter()
        .position(|lang| lang.extensions.iter().any(|e| e == ext))
        .map(|i| u16::try_from(i).expect("too many custom languages"))
}

/// Get a custom language by index.
pub fn get(index: u16) -> Option<&'static CustomLang> {
    registry().get(index as usize)
}

/// Get the short name for a custom language.
pub fn short_name(index: u16) -> &'static str {
    get(index).map_or("?", |l| &l.short)
}

fn config_dir() -> Option<PathBuf> {
    // XDG_CONFIG_HOME/syms/languages or ~/.config/syms/languages
    let base = std::env::var("XDG_CONFIG_HOME").map_or_else(
        |_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".config")
        },
        PathBuf::from,
    );
    let dir = base.join("syms").join("languages");
    dir.exists().then_some(dir)
}

fn load_custom_lang(config_path: &Path) -> Result<CustomLang> {
    let toml_str =
        std::fs::read_to_string(config_path).context("failed to read config file")?;
    let config: LangConfig = toml::from_str(&toml_str).context("failed to parse TOML")?;

    let name = config_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Expand ~ in parser path
    let parser_path = shellexpand(&config.parser);

    // Load the shared library
    let library = unsafe {
        libloading::Library::new(&parser_path)
            .with_context(|| format!("failed to load parser library: {parser_path}"))?
    };

    // Resolve the tree-sitter language function
    let symbol_name = config
        .symbol
        .unwrap_or_else(|| format!("tree_sitter_{name}"));

    let ts_language = unsafe {
        let func: libloading::Symbol<unsafe extern "C" fn() -> tree_sitter::Language> = library
            .get(symbol_name.as_bytes())
            .with_context(|| format!("symbol '{symbol_name}' not found in library"))?;
        func()
    };

    // Compile queries
    let queries = config
        .queries
        .iter()
        .filter_map(|qc| {
            let kind = parse_kind(&qc.kind)?;
            let query = match Query::new(&ts_language, &qc.query) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("syms: query error in {name} for {:?}: {e}", qc.kind);
                    return None;
                }
            };
            let name_idx = query.capture_index_for_name("name")?;
            let def_idx = query.capture_index_for_name("def");
            Some(CompiledQuery {
                query,
                kind,
                name_idx,
                def_idx,
            })
        })
        .collect();

    let short = config.short.unwrap_or_else(|| {
        config
            .extensions
            .first()
            .cloned()
            .unwrap_or_else(|| name.clone())
    });

    Ok(CustomLang {
        name,
        short,
        extensions: config.extensions,
        ts_language,
        queries,
        _library: library,
    })
}

fn parse_kind(s: &str) -> Option<SymbolKind> {
    match s {
        "fn" | "function" => Some(SymbolKind::Function),
        "method" => Some(SymbolKind::Method),
        "class" => Some(SymbolKind::Class),
        "struct" => Some(SymbolKind::Struct),
        "enum" => Some(SymbolKind::Enum),
        "interface" => Some(SymbolKind::Interface),
        "type" => Some(SymbolKind::Type),
        "const" | "constant" => Some(SymbolKind::Constant),
        "mod" | "module" => Some(SymbolKind::Module),
        "trait" => Some(SymbolKind::Trait),
        "component" => Some(SymbolKind::Component),
        "hook" => Some(SymbolKind::Hook),
        "test" => Some(SymbolKind::Test),
        _ => {
            eprintln!("syms: unknown kind '{s}' in custom language config");
            None
        }
    }
}

fn shellexpand(path: &str) -> String {
    path.strip_prefix("~/").map_or_else(
        || path.to_string(),
        |rest| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            format!("{home}/{rest}")
        },
    )
}
