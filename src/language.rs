use std::fmt;
use std::path::Path;

use crate::custom;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Go,
    Python,
    C,
    Cpp,
    Java,
    Ruby,
    Php,
    Bash,
    Css,
    Lua,
    /// Custom language loaded from ~/.config/syms/languages/
    Custom(u16),
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Self> {
        // Built-in languages first
        let builtin = match ext {
            "rs" => Some(Self::Rust),
            "ts" | "cts" | "mts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "cjs" | "mjs" | "jsx" => Some(Self::JavaScript),
            "go" => Some(Self::Go),
            "py" | "pyi" => Some(Self::Python),
            "c" | "h" => Some(Self::C),
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "rb" | "gemspec" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "sh" | "bash" | "zsh" => Some(Self::Bash),
            "css" => Some(Self::Css),
            "lua" => Some(Self::Lua),
            _ => None,
        };

        // Fall back to custom languages
        builtin.or_else(|| custom::from_extension(ext).map(Self::Custom))
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?;
        Self::from_extension(ext)
    }

    /// Resolve the target language of an injection marker (e.g. `#set! injection.language "kak"`).
    pub fn from_injection_name(name: &str) -> Option<Self> {
        let builtin = match name {
            "rust" => Some(Self::Rust),
            "typescript" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "javascript" => Some(Self::JavaScript),
            "go" => Some(Self::Go),
            "python" => Some(Self::Python),
            "c" => Some(Self::C),
            "cpp" | "c++" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "ruby" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "bash" | "sh" => Some(Self::Bash),
            "css" => Some(Self::Css),
            "lua" => Some(Self::Lua),
            _ => None,
        };
        builtin.or_else(|| custom::from_injection_name(name).map(Self::Custom))
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::Rust => "rs",
            Self::TypeScript => "ts",
            Self::Tsx => "tsx",
            Self::JavaScript => "js",
            Self::Go => "go",
            Self::Python => "py",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Ruby => "rb",
            Self::Php => "php",
            Self::Bash => "sh",
            Self::Css => "css",
            Self::Lua => "lua",
            Self::Custom(idx) => custom::short_name(idx),
        }
    }

    pub const fn color_code(self) -> &'static str {
        match self {
            Self::Rust | Self::JavaScript => "33",                            // yellow
            Self::TypeScript | Self::Tsx | Self::Css | Self::Go | Self::Lua => "36", // cyan
            Self::Python | Self::Ruby | Self::Php | Self::Custom(_) => "35", // magenta
            Self::C | Self::Cpp | Self::Bash => "32",                        // green
            Self::Java => "31",                                              // red
        }
    }

    pub fn ts_language(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP_ONLY.into(),
            Self::Bash => tree_sitter_bash::LANGUAGE.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Custom(idx) => custom::get(idx)
                .expect("custom language index out of bounds")
                .ts_language
                .clone(),
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short_name())
    }
}
