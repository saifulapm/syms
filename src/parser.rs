use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tree_sitter::{Parser, QueryCursor, Range, StreamingIterator};

use crate::language::Language;
use crate::queries::{self, CompiledQuery};
use crate::symbol::{Symbol, SymbolKind, Visibility};

const MAX_INJECTION_DEPTH: u32 = 8;

/// Parse a file and extract all symbols, descending into tree-sitter injections
/// (e.g. kak-inside-kak via `provide-module`, `define-command`, etc.) up to
/// `MAX_INJECTION_DEPTH` levels.
pub fn extract_symbols(path: &Arc<Path>, source: &[u8], lang: Language) -> Result<Vec<Symbol>> {
    let mut symbols = Vec::new();
    extract_recursive(path, source, lang, &[], 0, &mut symbols)?;

    // Deduplicate by (line, col) — keep first occurrence
    symbols.sort_by_key(|s| (s.line, s.col));
    symbols.dedup_by_key(|s| (s.line, s.col));

    // Reclassify semantic kinds (component, hook, test)
    for sym in &mut symbols {
        reclassify_semantic_kind(sym);
    }

    Ok(symbols)
}

/// Parse `source` as `lang`, optionally restricted to `ranges` (empty = whole file),
/// run this language's symbol queries, then walk the injection query and recurse
/// into each injected region.
fn extract_recursive(
    path: &Arc<Path>,
    source: &[u8],
    lang: Language,
    ranges: &[Range],
    depth: u32,
    symbols: &mut Vec<Symbol>,
) -> Result<()> {
    let ts_lang = lang.ts_language();
    let mut parser = Parser::new();
    parser.set_language(&ts_lang)?;
    if !ranges.is_empty() {
        parser
            .set_included_ranges(ranges)
            .map_err(|e| anyhow::anyhow!("invalid included ranges at index {}", e.0))?;
    }

    let Some(tree) = parser.parse(source, None) else {
        return Ok(());
    };

    for cq in queries::compiled_queries(lang) {
        extract_with_query(path, source, lang, &tree, cq, symbols);
    }

    if depth >= MAX_INJECTION_DEPTH {
        return Ok(());
    }
    let Some(iq) = queries::compiled_injection_query(lang) else {
        return Ok(());
    };

    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&iq.query, tree.root_node(), source);
    while let Some(m) = matches.next() {
        let mut content_node = None;
        let mut dynamic_lang: Option<String> = None;
        for c in m.captures {
            if Some(c.index) == iq.language_capture_idx {
                dynamic_lang = c.node.utf8_text(source).ok().map(str::to_string);
            } else if c.index == iq.content_idx {
                content_node = Some(c.node);
            }
        }
        let Some(content) = content_node else { continue };
        let lang_name = dynamic_lang
            .or_else(|| iq.pattern_languages[m.pattern_index].clone());
        let Some(name) = lang_name else { continue };
        let Some(injected_lang) = Language::from_injection_name(&name) else { continue };

        let inner_range = Range {
            start_byte: content.start_byte(),
            end_byte: content.end_byte(),
            start_point: content.start_position(),
            end_point: content.end_position(),
        };
        if inner_range.start_byte >= inner_range.end_byte {
            continue;
        }
        // Self-injection guard: don't recurse if the injection spans the same range we're parsing
        if ranges.len() == 1
            && inner_range.start_byte == ranges[0].start_byte
            && inner_range.end_byte == ranges[0].end_byte
        {
            continue;
        }

        extract_recursive(path, source, injected_lang, &[inner_range], depth + 1, symbols)?;
    }

    Ok(())
}

fn extract_with_query(
    path: &Arc<Path>,
    source: &[u8],
    lang: Language,
    tree: &tree_sitter::Tree,
    cq: &CompiledQuery,
    symbols: &mut Vec<Symbol>,
) {
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&cq.query, tree.root_node(), source);

    while let Some(m) = matches.next() {
        let mut name_node = None;
        let mut def_node = None;

        for capture in m.captures {
            if capture.index == cq.name_idx {
                name_node = Some(capture.node);
            } else if cq.def_idx == Some(capture.index) {
                def_node = Some(capture.node);
            }
        }

        let Some(name_n) = name_node else { continue };

        let name = name_n.utf8_text(source).unwrap_or("");
        if name.is_empty() {
            continue;
        }

        let start = name_n.start_position();

        // Use the @def node for signature and end_line (full symbol range)
        let (signature, end_line) = match def_node {
            Some(def_n) => (
                extract_signature(source, def_n),
                def_n.end_position().row + 1,
            ),
            None => (name.to_string(), start.row + 1),
        };

        let display_name = match find_parent_name(name_n, source, cq.kind) {
            Some(parent) => format!("{parent}.{name}"),
            None => name.to_string(),
        };

        let visibility = detect_visibility(name_n, def_node, lang, name);

        // Detect Rust test functions: function_item with #[test] attribute as previous sibling
        let kind = if lang == Language::Rust
            && cq.kind == SymbolKind::Function
            && def_node.is_some_and(|def| has_test_attribute(def, source))
        {
            SymbolKind::Test
        } else {
            cq.kind
        };

        symbols.push(Symbol {
            name: display_name,
            kind,
            lang,
            file: Arc::clone(path),
            line: start.row + 1,
            end_line,
            col: start.column + 1,
            signature,
            visibility,
        });
    }
}

/// Extract the first line of a node's text as the signature.
fn extract_signature(source: &[u8], node: tree_sitter::Node) -> String {
    let start = node.start_byte();
    let end = node.end_byte().min(source.len());
    let text = String::from_utf8_lossy(&source[start..end]);

    let first_line = text.lines().next().unwrap_or("");
    let trimmed = first_line.trim();

    if trimmed.len() > 120 {
        let mut boundary = 117;
        while boundary > 0 && !trimmed.is_char_boundary(boundary) {
            boundary -= 1;
        }
        format!("{}...", &trimmed[..boundary])
    } else {
        trimmed.to_string()
    }
}

/// Walk up the AST to find a parent class/struct/impl/object name.
fn find_parent_name(node: tree_sitter::Node, source: &[u8], kind: SymbolKind) -> Option<String> {
    if kind != SymbolKind::Method {
        return None;
    }

    let mut current = node.parent()?;
    loop {
        match current.kind() {
            // Rust: impl Type { fn method() }
            "impl_item" => {
                let type_node = current.child_by_field_name("type")?;
                return Some(type_node.utf8_text(source).ok()?.to_string());
            }
            // JS/TS/Java/PHP/Ruby: class, trait, interface, module containing methods
            "class_declaration" | "class" | "class_body"
            | "class_definition" | "trait_declaration" | "interface_declaration" => {
                let target = if current.kind() == "class_body" {
                    current.parent()?
                } else {
                    current
                };
                let name_node = target.child_by_field_name("name")?;
                return Some(name_node.utf8_text(source).ok()?.to_string());
            }
            // Go: func (r *Receiver) Method()
            "method_declaration" => {
                let receiver = current.child_by_field_name("receiver")?;
                let text = receiver.utf8_text(source).ok()?;
                // Extract type name from "(r *Type)" or "(r Type)"
                let type_name = text
                    .trim_matches(|c| c == '(' || c == ')')
                    .split_whitespace()
                    .last()?
                    .trim_start_matches('*');
                return Some(type_name.to_string());
            }
            // JS object literal: const obj = { method() {} }
            "object" => {
                let parent = current.parent()?;
                if parent.kind() == "variable_declarator"
                    && let Some(name_node) = parent.child_by_field_name("name")
                {
                    return Some(name_node.utf8_text(source).ok()?.to_string());
                }
                return None;
            }
            _ => {
                current = current.parent()?;
            }
        }
    }
}

/// Detect visibility/export status from the AST.
fn detect_visibility(
    _name_node: tree_sitter::Node,
    def_node: Option<tree_sitter::Node>,
    lang: Language,
    name: &str,
) -> Option<Visibility> {
    match lang {
        // Rust: check for visibility_modifier child on the definition node
        Language::Rust => {
            let def = def_node?;
            Some(if has_child_kind(def, "visibility_modifier") {
                Visibility::Public
            } else {
                Visibility::Private
            })
        }

        // TS/JS: check if parent (or grandparent) is export_statement
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            let def = def_node?;
            let is_exported = def.kind() == "export_statement"
                || def.parent().is_some_and(|p| p.kind() == "export_statement");
            Some(if is_exported {
                Visibility::Public
            } else {
                Visibility::Private
            })
        }

        // Go: uppercase first letter = exported
        Language::Go => {
            let first_char = name.chars().next()?;
            Some(if first_char.is_uppercase() {
                Visibility::Public
            } else {
                Visibility::Private
            })
        }

        // Java/PHP: check for public/private/protected modifiers
        Language::Java | Language::Php => {
            let def = def_node?;
            if has_child_kind(def, "modifiers") || has_child_kind(def, "visibility_modifier") {
                // Look for the actual modifier text
                let is_public = node_children_text_contains(def, "public");
                Some(if is_public {
                    Visibility::Public
                } else {
                    Visibility::Private
                })
            } else {
                // Java: package-private (no modifier) — treat as private
                // PHP: no modifier on functions = global scope = public
                Some(if lang == Language::Php {
                    Visibility::Public
                } else {
                    Visibility::Private
                })
            }
        }

        // Python: leading underscore = private (convention)
        Language::Python => {
            // Strip parent prefix if present (e.g., "Class.method" -> "method")
            let bare_name = name.rsplit('.').next().unwrap_or(name);
            Some(if bare_name.starts_with('_') {
                Visibility::Private
            } else {
                Visibility::Public
            })
        }

        // Languages without visibility concept (including custom)
        Language::C | Language::Cpp | Language::Ruby | Language::Bash | Language::Css
        | Language::Lua | Language::Custom(_) => None,
    }
}

fn has_child_kind(node: tree_sitter::Node, kind: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| c.kind() == kind)
}

fn node_children_text_contains(node: tree_sitter::Node, text: &str) -> bool {
    let mut cursor = node.walk();
    node.children(&mut cursor).any(|c| {
        c.kind().contains(text) || {
            // Check grandchildren for modifier lists
            let mut inner = c.walk();
            c.children(&mut inner).any(|gc| gc.kind() == text)
        }
    })
}

/// Check if a Rust `function_item` has a `#[test]` or `#[tokio::test]` attribute.
/// In tree-sitter-rust, attributes are previous siblings, not children.
fn has_test_attribute(def_node: tree_sitter::Node, source: &[u8]) -> bool {
    // Check previous siblings for attribute_item containing "test"
    let mut sibling = def_node.prev_sibling();
    while let Some(sib) = sibling {
        if sib.kind() == "attribute_item" {
            let text = sib.utf8_text(source).unwrap_or("");
            if text.contains("test") {
                return true;
            }
        } else {
            // Stop at non-attribute siblings (attributes are contiguous before the item)
            break;
        }
        sibling = sib.prev_sibling();
    }
    false
}

/// Reclassify symbols based on semantic patterns (name + language).
fn reclassify_semantic_kind(sym: &mut Symbol) {
    // Get the bare name (after the last dot for qualified names like "Class.method")
    let bare_name = sym.name.rsplit('.').next().unwrap_or(&sym.name);

    match sym.lang {
        // TSX: PascalCase function = React component
        Language::Tsx => {
            if sym.kind == SymbolKind::Function {
                let first = bare_name.chars().next().unwrap_or('a');
                if first.is_uppercase() {
                    sym.kind = SymbolKind::Component;
                } else if bare_name.starts_with("use")
                    && bare_name.len() > 3
                    && bare_name[3..].starts_with(|c: char| c.is_uppercase())
                {
                    sym.kind = SymbolKind::Hook;
                }
            }
        }

        // TS/JS: hooks (useAuth, useState, etc.)
        Language::TypeScript | Language::JavaScript => {
            if sym.kind == SymbolKind::Function
                && bare_name.starts_with("use")
                && bare_name.len() > 3
                && bare_name[3..].starts_with(|c: char| c.is_uppercase())
            {
                sym.kind = SymbolKind::Hook;
            }
        }

        // Python: test_* functions
        Language::Python => {
            if matches!(sym.kind, SymbolKind::Function | SymbolKind::Method)
                && bare_name.starts_with("test_")
            {
                sym.kind = SymbolKind::Test;
            }
        }

        // Go: Test* functions
        Language::Go => {
            if sym.kind == SymbolKind::Function
                && bare_name.starts_with("Test")
                && bare_name.len() > 4
            {
                sym.kind = SymbolKind::Test;
            }
        }

        // Rust: handled at extraction time via has_test_attribute()
        _ => {}
    }
}
