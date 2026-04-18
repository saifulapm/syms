use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use tree_sitter::Query;

use crate::custom;
use crate::language::Language;
use crate::symbol::SymbolKind;

/// A raw query definition: a tree-sitter query string and the symbol kind it extracts.
struct RawQueryDef {
    query_str: &'static str,
    kind: SymbolKind,
}

/// A compiled query with metadata, ready to execute.
pub struct CompiledQuery {
    pub query: Query,
    pub kind: SymbolKind,
    pub name_idx: u32,
    pub def_idx: Option<u32>,
}

macro_rules! lang_cache {
    ($lang:expr, $queries:expr) => {{
        static CACHE: OnceLock<Vec<CompiledQuery>> = OnceLock::new();
        CACHE.get_or_init(|| compile($lang, $queries))
    }};
}

/// Returns the compiled queries for a given language. Compiled once, cached forever.
pub fn compiled_queries(lang: Language) -> &'static [CompiledQuery] {
    match lang {
        Language::Rust => lang_cache!(lang, RUST_QUERIES),
        Language::TypeScript => lang_cache!(lang, TYPESCRIPT_QUERIES),
        Language::Tsx => lang_cache!(lang, TYPESCRIPT_QUERIES),
        Language::JavaScript => lang_cache!(lang, JAVASCRIPT_QUERIES),
        Language::Go => lang_cache!(lang, GO_QUERIES),
        Language::Python => lang_cache!(lang, PYTHON_QUERIES),
        Language::C => lang_cache!(lang, C_QUERIES),
        Language::Cpp => lang_cache!(lang, CPP_QUERIES),
        Language::Java => lang_cache!(lang, JAVA_QUERIES),
        Language::Ruby => lang_cache!(lang, RUBY_QUERIES),
        Language::Php => lang_cache!(lang, PHP_QUERIES),
        Language::Bash => lang_cache!(lang, BASH_QUERIES),
        Language::Css => lang_cache!(lang, CSS_QUERIES),
        Language::Lua => lang_cache!(lang, LUA_QUERIES),
        // Custom languages: queries are pre-compiled during loading
        Language::Custom(idx) => custom::get(idx).map_or(&[], |l| &l.queries),
    }
}

/// Raw injection query text for a language.
/// Built-ins currently return `None`; custom languages get theirs from TOML.
pub fn injection_query_text(lang: Language) -> Option<&'static str> {
    match lang {
        Language::Custom(idx) => custom::get(idx)
            .and_then(|l| l.injection_query_text.as_deref()),
        _ => None,
    }
}

/// A compiled tree-sitter injection query.
pub struct CompiledInjectionQuery {
    pub query: Query,
    pub content_idx: u32,
    /// Capture index for `@injection.language` (dynamic per-match language selection).
    pub language_capture_idx: Option<u32>,
    /// Static per-pattern language from `#set! injection.language "..."`.
    pub pattern_languages: Vec<Option<String>>,
}

/// Returns the compiled injection query for a language, or None if the language
/// has no injection query. Compiled once per language and leaked to 'static.
pub fn compiled_injection_query(lang: Language) -> Option<&'static CompiledInjectionQuery> {
    static CACHE: OnceLock<Mutex<HashMap<Language, Option<&'static CompiledInjectionQuery>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap();
    if let Some(entry) = guard.get(&lang) {
        return *entry;
    }
    let compiled = compile_injection_query(lang).map(|c| &*Box::leak(Box::new(c)));
    guard.insert(lang, compiled);
    compiled
}

fn compile_injection_query(lang: Language) -> Option<CompiledInjectionQuery> {
    let src = injection_query_text(lang)?;
    let ts_lang = lang.ts_language();
    let query = match Query::new(&ts_lang, src) {
        Ok(q) => q,
        Err(e) => {
            eprintln!("syms: injection query compile error for {lang:?}: {e}");
            return None;
        }
    };
    let content_idx = query.capture_index_for_name("injection.content")?;
    let language_capture_idx = query.capture_index_for_name("injection.language");
    let mut pattern_languages = Vec::with_capacity(query.pattern_count());
    for pat_idx in 0..query.pattern_count() {
        let mut lang_str = None;
        for setting in query.property_settings(pat_idx) {
            if &*setting.key == "injection.language" {
                lang_str = setting.value.as_deref().map(str::to_string);
            }
        }
        pattern_languages.push(lang_str);
    }
    Some(CompiledInjectionQuery {
        query,
        content_idx,
        language_capture_idx,
        pattern_languages,
    })
}

fn compile(lang: Language, raw: &[RawQueryDef]) -> Vec<CompiledQuery> {
    let ts_lang = lang.ts_language();
    raw.iter()
        .filter_map(|def| {
            let query = match Query::new(&ts_lang, def.query_str) {
                Ok(q) => q,
                Err(e) => {
                    eprintln!("syms: query compile error for {:?}: {e}", def.kind);
                    return None;
                }
            };
            let name_idx = query.capture_index_for_name("name")?;
            let def_idx = query.capture_index_for_name("def");
            Some(CompiledQuery {
                query,
                kind: def.kind,
                name_idx,
                def_idx,
            })
        })
        .collect()
}

// ── Rust ──────────────────────────────────────────────────────────────

static RUST_QUERIES: &[RawQueryDef] = &[
    // Method query MUST come before function query so methods win in dedup
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(impl_item body: (declaration_list (function_item name: (identifier) @name) @def))",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_item name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Struct,
        query_str: "(struct_item name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_item name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Trait,
        query_str: "(trait_item name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(type_item name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Constant,
        query_str: "(const_item name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Module,
        query_str: "(mod_item name: (identifier) @name) @def",
    },
];

// ── TypeScript / TSX ──────────────────────────────────────────────────

static TYPESCRIPT_QUERIES: &[RawQueryDef] = &[
    // Export queries MUST come before bare queries so "export function" wins in dedup
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(export_statement (function_declaration name: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(export_statement (class_declaration name: (type_identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Interface,
        query_str: "(export_statement (interface_declaration name: (type_identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(export_statement (type_alias_declaration name: (type_identifier) @name)) @def",
    },
    // Bare declarations
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_declaration name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Interface,
        query_str: "(interface_declaration name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(type_alias_declaration name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method_definition name: (property_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function) @_val)) @def",
    },
];

// ── JavaScript ────────────────────────────────────────────────────────

static JAVASCRIPT_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(export_statement (function_declaration name: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(export_statement (class_declaration name: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method_definition name: (property_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(lexical_declaration (variable_declarator name: (identifier) @name value: (arrow_function) @_val)) @def",
    },
];

// ── Go ────────────────────────────────────────────────────────────────

static GO_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method_declaration name: (field_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Struct,
        query_str: "(type_declaration (type_spec name: (type_identifier) @name type: (struct_type))) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Interface,
        query_str: "(type_declaration (type_spec name: (type_identifier) @name type: (interface_type))) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(type_declaration (type_spec name: (type_identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Constant,
        query_str: "(const_declaration (const_spec name: (identifier) @name)) @def",
    },
];

// ── Python ────────────────────────────────────────────────────────────

static PYTHON_QUERIES: &[RawQueryDef] = &[
    // Methods (inside class) must come before bare functions so methods win in dedup
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(class_definition body: (block (function_definition name: (identifier) @name) @def))",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(class_definition body: (block (decorated_definition definition: (function_definition name: (identifier) @name)) @def))",
    },
    // Decorated first so @decorator shows in signature
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(decorated_definition definition: (function_definition name: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(decorated_definition definition: (class_definition name: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_definition name: (identifier) @name) @def",
    },
];

// ── C ─────────────────────────────────────────────────────────────────

static C_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition declarator: (pointer_declarator declarator: (function_declarator declarator: (identifier) @name))) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Struct,
        query_str: "(struct_specifier name: (type_identifier) @name body: (_)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_specifier name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(type_definition declarator: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Constant,
        query_str: "(preproc_def name: (identifier) @name) @def",
    },
];

// ── C++ ───────────────────────────────────────────────────────────────

static CPP_QUERIES: &[RawQueryDef] = &[
    // Qualified methods first (Class::method)
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(function_definition declarator: (function_declarator declarator: (qualified_identifier name: (identifier) @name))) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition declarator: (function_declarator declarator: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_specifier name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Struct,
        query_str: "(struct_specifier name: (type_identifier) @name body: (_)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Module,
        query_str: "(namespace_definition name: (namespace_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_specifier name: (type_identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Type,
        query_str: "(type_definition declarator: (type_identifier) @name) @def",
    },
];

// ── Java ──────────────────────────────────────────────────────────────

static JAVA_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(constructor_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Interface,
        query_str: "(interface_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_declaration name: (identifier) @name) @def",
    },
];

// ── Ruby ──────────────────────────────────────────────────────────────

static RUBY_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method name: (_) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(singleton_method name: (_) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class name: (constant) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Module,
        query_str: "(module name: (constant) @name) @def",
    },
];

// ── PHP ───────────────────────────────────────────────────────────────

static PHP_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(method_declaration name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(class_declaration name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Interface,
        query_str: "(interface_declaration name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Trait,
        query_str: "(trait_declaration name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Enum,
        query_str: "(enum_declaration name: (name) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Module,
        query_str: "(namespace_definition name: (namespace_name) @name) @def",
    },
];

// ── Bash ──────────────────────────────────────────────────────────────

static BASH_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_definition name: (word) @name) @def",
    },
];

// ── CSS ───────────────────────────────────────────────────────────────

static CSS_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Class,
        query_str: "(rule_set (selectors) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(keyframes_statement (keyframes_name) @name) @def",
    },
];

// ── Lua ───────────────────────────────────────────────────────────────

static LUA_QUERIES: &[RawQueryDef] = &[
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_declaration name: (identifier) @name) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Method,
        query_str: "(function_declaration name: (method_index_expression method: (identifier) @name)) @def",
    },
    RawQueryDef {
        kind: SymbolKind::Function,
        query_str: "(function_declaration name: (dot_index_expression field: (identifier) @name)) @def",
    },
];
