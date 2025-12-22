// src/context/index.rs
//
// Repository indexing logic.
//
// Guarantees:
// - Bounded output
// - No junk directories
// - No semantic inference
// - Deterministic symbol sets
//

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use walkdir::{DirEntry, WalkDir};
use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use super::types::{
    FileSymbols,
    Import,
    IndexHandle,
    RepoFacts,
    SymbolDef,
    SymbolIndex,
};

use crate::detectors::{
    framework::TestFramework,
    language::Language,
};

/* ============================================================
   Constants (hard limits)
   ============================================================ */

const MAX_SYMBOLS_PER_FILE: usize = 50;
const MAX_IMPORTS_PER_FILE: usize = 20;

/* ============================================================
   Public entry
   ============================================================ */

pub fn spawn_repo_indexer(repo_root: PathBuf) -> IndexHandle {
    let handle = IndexHandle::new_indexing();
    let handle_clone = handle.clone();

    std::thread::spawn(move || {
        let result = (|| {
            let facts = extract_repo_facts(&repo_root);
            handle_clone.set_facts(facts);

            let symbols = build_symbol_index(&repo_root);
            handle_clone.set_symbols(symbols);

            Ok::<(), String>(())
        })();

        match result {
            Ok(_) => handle_clone.mark_ready(),
            Err(e) => handle_clone.mark_failed(e),
        }
    });

    handle
}

/* ============================================================
   Repo facts
   ============================================================ */

fn extract_repo_facts(repo_root: &Path) -> RepoFacts {
    let mut languages = Vec::new();
    let mut test_frameworks = Vec::new();

    if repo_root.join("Cargo.toml").exists() {
        languages.push(Language::Rust);
    }

    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("setup.py").exists()
        || repo_root.join("requirements.txt").exists()
    {
        languages.push(Language::Python);
    }

    if repo_root.join("tests").exists() {
        if languages.contains(&Language::Python) {
            test_frameworks.push(TestFramework::Pytest);
        }
        if languages.contains(&Language::Rust) {
            test_frameworks.push(TestFramework::CargoTest);
        }
    }

    RepoFacts {
        languages,
        test_frameworks,
        forbidden_deps: vec![
            "torch".into(),
            "tensorflow".into(),
            "jax".into(),
        ],
    }
}

/* ============================================================
   Symbol index
   ============================================================ */

fn build_symbol_index(repo_root: &Path) -> SymbolIndex {
    let mut by_file = HashMap::new();
    let mut by_symbol = HashMap::new();

    for entry in WalkDir::new(repo_root)
        .into_iter()
        .filter_entry(|e| !is_ignored(e))
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        let kind = match path.extension().and_then(|s| s.to_str()) {
            Some("py") => LanguageKind::Python,
            Some("rs") => LanguageKind::Rust,
            _ => continue,
        };

        let file_symbols = match index_file(path, kind) {
            Some(s) => s,
            None => continue,
        };

        for sym in &file_symbols.symbols {
            let key = format!("{}::{}", sym.file.display(), sym.name);
            by_symbol.entry(key).or_insert_with(|| sym.clone());
        }

        by_file.insert(path.to_path_buf(), file_symbols);
    }

    SymbolIndex { by_file, by_symbol }
}

/* ============================================================
   Unified file indexing
   ============================================================ */

#[derive(Clone, Copy)]
enum LanguageKind {
    Python,
    Rust,
}

fn index_file(path: &Path, kind: LanguageKind) -> Option<FileSymbols> {
    let src = fs::read_to_string(path).ok()?;

    let mut parser = Parser::new();
    match kind {
        LanguageKind::Python => parser.set_language(&python::language()).ok()?,
        LanguageKind::Rust => parser.set_language(&rust::language()).ok()?,
    }

    let tree = parser.parse(&src, None)?;
    let root = tree.root_node();

    let mut symbols = Vec::new();
    let mut imports = Vec::new();
    let mut import_set = HashSet::new();

    walk_node(
        kind,
        root,
        path,
        &src,
        &mut symbols,
        &mut imports,
        &mut import_set,
    );

    symbols.truncate(MAX_SYMBOLS_PER_FILE);
    imports.truncate(MAX_IMPORTS_PER_FILE);

    Some(FileSymbols { symbols, imports })
}

/* ============================================================
   Tree walking (bounded)
   ============================================================ */

fn walk_node(
    kind: LanguageKind,
    node: Node,
    path: &Path,
    src: &str,
    symbols: &mut Vec<SymbolDef>,
    imports: &mut Vec<Import>,
    import_set: &mut HashSet<String>,
) {
    if symbols.len() >= MAX_SYMBOLS_PER_FILE
        && imports.len() >= MAX_IMPORTS_PER_FILE
    {
        return;
    }

    match (kind, node.kind()) {
        (LanguageKind::Python, "function_definition")
        | (LanguageKind::Python, "class_definition")
        | (LanguageKind::Rust, "function_item")
        | (LanguageKind::Rust, "struct_item")
        | (LanguageKind::Rust, "enum_item")
        | (LanguageKind::Rust, "trait_item") => {
            extract_named_symbol(node, path, src, symbols);
        }

        (LanguageKind::Python, "import_statement")
        | (LanguageKind::Python, "import_from_statement")
        | (LanguageKind::Rust, "use_declaration") => {
            if let Ok(text) = node.utf8_text(src.as_bytes()) {
                let key = text.trim().to_string();
                if import_set.insert(key.clone()) {
                    imports.push(Import {
                        module: key,
                        names: Vec::new(),
                    });
                }
            }
        }

        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_node(
                kind,
                child,
                path,
                src,
                symbols,
                imports,
                import_set,
            );
        }
    }
}

/* ============================================================
   Helpers
   ============================================================ */

fn extract_named_symbol(
    node: Node,
    path: &Path,
    src: &str,
    symbols: &mut Vec<SymbolDef>,
) {
    if let Some(name) = node.child_by_field_name("name") {
        if let Ok(text) = name.utf8_text(src.as_bytes()) {
            symbols.push(SymbolDef {
                name: text.to_string(),
                file: path.to_path_buf(),
                line: node.start_position().row + 1,
            });
        }
    }
}

fn is_ignored(e: &DirEntry) -> bool {
    let name = e.file_name().to_string_lossy();

    matches!(
        name.as_ref(),
        ".git"
            | ".venv"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "__pycache__"
    )
}
