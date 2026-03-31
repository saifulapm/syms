use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use crate::output;
use crate::symbol::Symbol;

#[derive(Debug, Clone, Copy)]
pub enum Picker {
    Fzf,
    Sk,
}

impl Picker {
    const fn bin(self) -> &'static str {
        match self {
            Self::Fzf => "fzf",
            Self::Sk => "sk",
        }
    }
}

/// Launch fzf/sk with tree-sitter-aware preview showing the full symbol body.
pub fn run(picker: Picker, symbols: &[Symbol]) -> Result<()> {
    let bin = picker.bin();

    let has_bat = Command::new("bat")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());

    // Field 2 is tab-delimited: file:start_line:end_line:col
    // Preview extracts start/end and shows the exact symbol range from tree-sitter
    // Use printf instead of echo to handle paths with spaces safely
    let preview_cmd = if has_bat {
        r#"IFS=: read -r file start end col <<< {2}; bat --color=always --style=numbers --highlight-line="$start" --line-range="$start:$end" "$file""#
    } else {
        r#"IFS=: read -r file start end col <<< {2}; sed -n "${start},${end}p" "$file""#
    };

    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    // Use IFS=: read for safe field splitting (handles spaces in paths)
    let enter_bind = format!(
        r#"enter:become(IFS=: read -r file start end col <<< {{2}}; {editor} +$start "$file")"#
    );

    // sk doesn't support become(), use execute() instead
    let sk_enter_bind = format!(
        r#"enter:execute(IFS=: read -r file start end col <<< {{2}}; {editor} +$start "$file")"#
    );

    let enter_arg = match picker {
        Picker::Fzf => format!("--bind={enter_bind}"),
        Picker::Sk => format!("--bind={sk_enter_bind}"),
    };

    let preview_arg = format!("--preview={preview_cmd}");

    let mut child = Command::new(bin)
        .arg("--ansi")
        .arg("--delimiter=\t")
        .arg("--with-nth=1,3") // show name + signature, hide file:line:col field
        .arg(&preview_arg)
        .arg("--preview-window=right:60%:wrap")
        .arg(&enter_arg)
        .arg("--bind=ctrl-/:toggle-preview")
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("`{bin}` not found — install it or use stdout mode"))?;

    let mut stdin = child.stdin.take().expect("piped stdin");

    for sym in symbols {
        let _ = output::write_symbol(&mut stdin, sym, true);
    }

    drop(stdin);
    let status = child.wait()?;

    // fzf/sk exit 130 on Esc/Ctrl-C — normal
    if !status.success() && status.code() != Some(130) {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}
