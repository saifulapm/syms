use std::fmt;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::language::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Type,
    Constant,
    Module,
    Trait,
    Component,
    Hook,
    Test,
}

impl SymbolKind {
    pub const fn short_name(self) -> &'static str {
        match self {
            Self::Function => "fn",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Type => "type",
            Self::Constant => "const",
            Self::Module => "mod",
            Self::Trait => "trait",
            Self::Component => "component",
            Self::Hook => "hook",
            Self::Test => "test",
        }
    }

    pub const fn color_code(self) -> &'static str {
        match self {
            Self::Function | Self::Method => "35",                          // magenta
            Self::Class | Self::Struct | Self::Enum | Self::Component => "36", // cyan
            Self::Interface | Self::Type | Self::Trait => "34",             // blue
            Self::Constant | Self::Hook => "33",                           // yellow
            Self::Module | Self::Test => "32",                             // green
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.short_name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    #[serde(serialize_with = "serialize_lang")]
    pub lang: Language,
    #[serde(serialize_with = "serialize_path")]
    pub file: Arc<Path>,
    pub line: usize,
    pub end_line: usize,
    pub col: usize,
    pub signature: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
}

// serde serialize_with requires &T signature
#[expect(clippy::trivially_copy_pass_by_ref)]
fn serialize_lang<S: serde::Serializer>(lang: &Language, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(lang.short_name())
}

fn serialize_path<S: serde::Serializer>(path: &Arc<Path>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&path.display().to_string())
}
