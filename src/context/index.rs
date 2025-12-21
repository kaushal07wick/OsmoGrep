// src/context/index.rs
//
// Repository indexing logic.
//
// Responsibilities:
// - extract RepoFacts
// - build SymbolIndex using tree-sitter (Python + Rust)
// - publish results asynchronously
//
// This module does not define data types and does not talk to the LLM.
//

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;
use tree_sitter::{Parser, TreeCursor};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use super::types::{
    IndexHandle,
    RepoFacts,
    SymbolIndex,
    FileSymbols,
    SymbolDef,
    Import,
};

use crate::detectors::{
    framework::TestFramework,
    language::Language,
};

/* ---------- public entry ---------- */

pub fn spawn_repo_indexer(repo_root: PathBuf) -> IndexHandle {
    let handle = IndexHandle::new_indexing();
    let handle_clone = handle.clone();

    std::thread::spawn(move || {
        let facts = extract_repo_facts(&repo_root);
        handle_clone.set_facts(facts);

        let symbols = build_symbol_index(&repo_root);
        handle_clone.set_symbols(symbols);

        handle_clone.mark_ready();
    });

    handle
}

/* ---------- repo facts ---------- */

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

/* ---------- symbol index ---------- */

fn build_symbol_index(repo_root: &Path) -> SymbolIndex {
    let mut by_file = HashMap::new();
    let mut by_symbol = HashMap::new();

    for entry in WalkDir::new(repo_root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();

        let file_syms = match path.extension().and_then(|s| s.to_str()) {
            Some("py") => index_file(path, LanguageKind::Python),
            Some("rs") => index_file(path, LanguageKind::Rust),
            _ => None,
        };

        if let Some(file_syms) = file_syms {
            for sym in &file_syms.symbols {
                by_symbol.insert(sym.name.clone(), sym.clone());
            }
            by_file.insert(path.to_path_buf(), file_syms);
        }
    }

    SymbolIndex { by_file, by_symbol }
}

/* ---------- unified file indexing ---------- */

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
    let mut cursor = tree.walk();

    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    walk_tree(kind, &mut cursor, path, &src, &mut symbols, &mut imports);

    Some(FileSymbols { symbols, imports })
}

/* ---------- tree walking ---------- */

fn walk_tree(
    kind: LanguageKind,
    cursor: &mut TreeCursor,
    path: &Path,
    src: &str,
    symbols: &mut Vec<SymbolDef>,
    imports: &mut Vec<Import>,
) {
    loop {
        let node = cursor.node();

        match (kind, node.kind()) {
            // ---- Python ----
            (LanguageKind::Python, "function_definition")
            | (LanguageKind::Python, "class_definition") => {
                extract_named_symbol(node, path, src, symbols);
            }

            (LanguageKind::Python, "import_statement") => {
                if let Some(name) = node.child_by_field_name("name") {
                    if let Ok(text) = name.utf8_text(src.as_bytes()) {
                        imports.push(Import {
                            module: text.to_string(),
                            names: Vec::new(),
                        });
                    }
                }
            }

            (LanguageKind::Python, "import_from_statement") => {
                let module = node
                    .child_by_field_name("module")
                    .and_then(|n| n.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("")
                    .to_string();

                let mut names = Vec::new();
                let mut c = node.walk();
                if c.goto_first_child() {
                    loop {
                        let n = c.node();
                        if n.kind() == "identifier" {
                            if let Ok(t) = n.utf8_text(src.as_bytes()) {
                                names.push(t.to_string());
                            }
                        }
                        if !c.goto_next_sibling() {
                            break;
                        }
                    }
                }

                imports.push(Import { module, names });
            }

            // ---- Rust ----
            (LanguageKind::Rust, "function_item")
            | (LanguageKind::Rust, "struct_item")
            | (LanguageKind::Rust, "enum_item")
            | (LanguageKind::Rust, "trait_item") => {
                extract_named_symbol(node, path, src, symbols);
            }

            (LanguageKind::Rust, "use_declaration") => {
                if let Ok(text) = node.utf8_text(src.as_bytes()) {
                    imports.push(Import {
                        module: text.replace("use ", "").replace(';', ""),
                        names: Vec::new(),
                    });
                }
            }

            _ => {}
        }

        if cursor.goto_first_child() {
            walk_tree(kind, cursor, path, src, symbols, imports);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/* ---------- helpers ---------- */

fn extract_named_symbol(
    node: tree_sitter::Node,
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
