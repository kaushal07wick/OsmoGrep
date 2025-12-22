// src/context/index.rs
//
// Repository indexing logic.
//
// PURPOSE:
// - Cache shallow structural facts
// - NEVER resolve symbols for diffs
// - NEVER infer semantics
// - BEST-EFFORT ONLY
//

use std::{
    collections::HashMap,
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
   Public entry
   ============================================================ */

pub fn spawn_repo_indexer(repo_root: PathBuf) -> IndexHandle {
    let handle = IndexHandle::new_indexing(repo_root.clone());
    let handle_clone = handle.clone();

    std::thread::spawn(move || {
        let result = (|| {
            let code_roots = detect_code_roots(&repo_root);
            let test_roots = detect_test_roots(&repo_root);

            *handle_clone.code_roots.write().unwrap() = code_roots.clone();
            *handle_clone.test_roots.write().unwrap() = test_roots.clone();

            let facts = extract_repo_facts(&repo_root);
            handle_clone.set_facts(facts);

            // BEST-EFFORT auxiliary cache only
            let symbols =
                build_symbol_index(&repo_root, &code_roots, &test_roots);
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
   Repo facts (Cheap + Conservative)
   ============================================================ */

fn extract_repo_facts(repo_root: &Path) -> RepoFacts {
    let mut languages = Vec::new();
    let mut test_frameworks = Vec::new();

    if repo_root.join("Cargo.toml").exists() {
        languages.push(Language::Rust);
        test_frameworks.push(TestFramework::CargoTest);
    }

    if repo_root.join("pytest.ini").exists()
        || repo_root.join("conftest.py").exists()
    {
        languages.push(Language::Python);
        test_frameworks.push(TestFramework::Pytest);
    } else if repo_root.join("pyproject.toml").exists()
        || repo_root.join("setup.py").exists()
        || repo_root.join("requirements.txt").exists()
    {
        languages.push(Language::Python);
        test_frameworks.push(TestFramework::Unittest);
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
   Roots detection
   ============================================================ */

fn detect_code_roots(repo_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let src = repo_root.join("src");
    if src.is_dir() {
        roots.push(src);
    }

    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let named = repo_root.join(repo_name);
    if named.is_dir() {
        roots.push(named);
    }

    if roots.is_empty() {
        roots.push(repo_root.to_path_buf());
    }

    roots
}

fn detect_test_roots(repo_root: &Path) -> Vec<PathBuf> {
    ["test", "tests"]
        .iter()
        .map(|n| repo_root.join(n))
        .filter(|p| p.is_dir())
        .collect()
}

/* ============================================================
   Symbol index (AUXILIARY ONLY)
   ============================================================ */

fn build_symbol_index(
    repo_root: &Path,
    code_roots: &[PathBuf],
    test_roots: &[PathBuf],
) -> SymbolIndex {
    let mut by_file = HashMap::new();

    for root in code_roots {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored(e))
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let abs_path = entry.path();

            // NEVER index tests
            if is_under_any(abs_path, test_roots) {
                continue;
            }

            let rel_path = abs_path
                .strip_prefix(repo_root)
                .unwrap_or(abs_path)
                .to_path_buf();

            let kind = match rel_path.extension().and_then(|s| s.to_str()) {
                Some("py") => LanguageKind::Python,
                Some("rs") => LanguageKind::Rust,
                _ => continue,
            };

            if let Some(fs) = index_file(abs_path, &rel_path, kind) {
                by_file.insert(rel_path, fs);
            }
        }
    }

    SymbolIndex { by_file }
}

/* ============================================================
   File indexing (FULL FILE, NO LIMITS)
   ============================================================ */

#[derive(Clone, Copy)]
enum LanguageKind {
    Python,
    Rust,
}

fn index_file(
    abs_path: &Path,
    rel_path: &Path,
    kind: LanguageKind,
) -> Option<FileSymbols> {
    let src = fs::read_to_string(abs_path).ok()?;

    let mut parser = Parser::new();
    match kind {
        LanguageKind::Python => parser.set_language(&python::language()).ok()?,
        LanguageKind::Rust => parser.set_language(&rust::language()).ok()?,
    }

    let tree = parser.parse(&src, None)?;
    let root = tree.root_node();

    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    walk_node(
        kind,
        root,
        rel_path,
        &src,
        &mut symbols,
        &mut imports,
        None,
    );

    Some(FileSymbols { symbols, imports })
}

/* ============================================================
   Tree walk (FULL, CLASS-AWARE, FACTUAL)
   ============================================================ */

fn walk_node(
    kind: LanguageKind,
    node: Node,
    file: &Path,
    src: &str,
    symbols: &mut Vec<SymbolDef>,
    imports: &mut Vec<Import>,
    current_class: Option<String>,
) {
    match (kind, node.kind()) {
        /* -------- Python class -------- */
        (LanguageKind::Python, "class_definition") => {
            if let Some(n) = node.child_by_field_name("name") {
                if let Ok(cls) = n.utf8_text(src.as_bytes()) {
                    let cls = cls.to_string();

                    symbols.push(SymbolDef {
                        name: cls.clone(),
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                    });

                    for i in 0..node.child_count() {
                        if let Some(c) = node.child(i) {
                            walk_node(
                                kind,
                                c,
                                file,
                                src,
                                symbols,
                                imports,
                                Some(cls.clone()),
                            );
                        }
                    }
                    return;
                }
            }
        }

        /* -------- Python function -------- */
        (LanguageKind::Python, "function_definition") => {
            if let Some(n) = node.child_by_field_name("name") {
                if let Ok(fn_name) = n.utf8_text(src.as_bytes()) {
                    let name = match &current_class {
                        Some(c) => format!("{c}.{fn_name}"),
                        None => fn_name.to_string(),
                    };

                    symbols.push(SymbolDef {
                        name,
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }

        /* -------- Rust -------- */
        (LanguageKind::Rust, "function_item")
        | (LanguageKind::Rust, "struct_item")
        | (LanguageKind::Rust, "enum_item") => {
            if let Some(n) = node.child_by_field_name("name") {
                if let Ok(text) = n.utf8_text(src.as_bytes()) {
                    symbols.push(SymbolDef {
                        name: text.to_string(),
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }

        /* -------- Imports -------- */
        (LanguageKind::Python, "import_statement")
        | (LanguageKind::Python, "import_from_statement")
        | (LanguageKind::Rust, "use_declaration") => {
            if let Ok(text) = node.utf8_text(src.as_bytes()) {
                imports.push(Import {
                    module: text.trim().to_string(),
                    names: Vec::new(),
                });
            }
        }

        _ => {}
    }

    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            walk_node(
                kind,
                c,
                file,
                src,
                symbols,
                imports,
                current_class.clone(),
            );
        }
    }
}

/* ============================================================
   Helpers
   ============================================================ */

fn is_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| path.starts_with(r))
}

fn is_ignored(e: &DirEntry) -> bool {
    matches!(
        e.file_name().to_string_lossy().as_ref(),
        ".git"
            | ".venv"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "__pycache__"
    )
}
