use std::io::Write;

use crate::symbol::{Symbol, Visibility};

/// Write a symbol to the output in a fzf-friendly format.
///
/// Tab-delimited fields:
///   `field1: lang kind [vis] name` | `field2: file:line:end_line:col` | `field3: signature`
pub fn write_symbol<W: Write>(w: &mut W, sym: &Symbol, color: bool) -> std::io::Result<()> {
    let lang = sym.lang.short_name();
    let kind = sym.kind.short_name();
    let path = sym.file.display();
    let name = &sym.name;
    let vis = match sym.visibility {
        Some(Visibility::Public) => "[pub] ",
        Some(Visibility::Private) => "[prv] ",
        None => "",
    };

    if color {
        let lang_color = sym.lang.color_code();
        let kind_color = sym.kind.color_code();
        let vis_colored = match sym.visibility {
            Some(Visibility::Public) => "\x1b[32m[pub]\x1b[0m ",  // green
            Some(Visibility::Private) => "\x1b[90m[prv]\x1b[0m ", // dim
            None => "",
        };
        writeln!(
            w,
            "\x1b[{lang_color}m{lang:<4}\x1b[0m \x1b[{kind_color}m{kind:<10}\x1b[0m {vis_colored}\x1b[1m{name}\x1b[0m\t{path}:{line}:{end_line}:{col}\t{sig}",
            line = sym.line,
            end_line = sym.end_line,
            col = sym.col,
            sig = sym.signature,
        )
    } else {
        writeln!(
            w,
            "{lang:<4} {kind:<10} {vis}{name}\t{path}:{line}:{end_line}:{col}\t{sig}",
            line = sym.line,
            end_line = sym.end_line,
            col = sym.col,
            sig = sym.signature,
        )
    }
}
