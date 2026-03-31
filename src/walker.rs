use std::path::Path;
use std::sync::Arc;

use crossbeam_channel::Sender;
use ignore::WalkBuilder;

use crate::language::Language;
use crate::parser;
use crate::symbol::Symbol;

// Sender is cloned per-thread by WalkParallel, ownership transfer is intentional
#[expect(clippy::needless_pass_by_value)]
pub fn walk(root: &Path, tx: Sender<Symbol>) {
    let walker = WalkBuilder::new(root)
        .hidden(true) // skip hidden files
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build_parallel();

    walker.run(|| {
        let tx = tx.clone();
        Box::new(move |entry| {
            let Ok(entry) = entry else {
                return ignore::WalkState::Continue;
            };

            if entry.file_type().is_some_and(|ft| !ft.is_file()) {
                return ignore::WalkState::Continue;
            }

            let path = entry.path();

            let Some(lang) = Language::from_path(path) else {
                return ignore::WalkState::Continue;
            };

            let Ok(source) = std::fs::read(path) else {
                return ignore::WalkState::Continue;
            };

            // Strip "./" prefix for cleaner output
            let clean_path = path.strip_prefix("./").unwrap_or(path);
            let path: Arc<Path> = Arc::from(clean_path);
            let Ok(symbols) = parser::extract_symbols(&path, &source, lang) else {
                return ignore::WalkState::Continue;
            };

            for sym in symbols {
                if tx.send(sym).is_err() {
                    return ignore::WalkState::Quit;
                }
            }

            ignore::WalkState::Continue
        })
    });
}
