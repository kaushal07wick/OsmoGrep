// src/context/index.rs
//
// Repository indexing + context construction.


use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use walkdir::{DirEntry, WalkDir};
use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use crate::detectors::language::Language;
use crate::state::DiffAnalysis;

use super::types::{
    ContextSlice,
    DiffTarget,
    FileSymbols,
    Import,
    IndexHandle,
    RepoFacts,
    SymbolDef,
    SymbolIndex,
    SymbolResolution,
};


pub fn spawn_repo_indexer(repo_root: PathBuf) -> IndexHandle {
    let handle = IndexHandle::new_indexing(repo_root.clone());
    let handle_clone = handle.clone();

    std::thread::spawn(move || {
        let result = (|| {
            let code_roots = detect_code_roots(&repo_root);
            *handle_clone.code_roots.write().unwrap() = code_roots.clone();

            let facts = extract_repo_facts(&repo_root);
            handle_clone.set_facts(facts);

            let symbols = build_symbol_index(&repo_root, &code_roots);
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

fn parse_file(
    file: &Path,
    source: &str,
) -> (Vec<SymbolDef>, Vec<Import>) {
    let mut parser = Parser::new();

    let language = match file.extension().and_then(|s| s.to_str()) {
        Some("py") => python::language(),
        Some("rs") => rust::language(),
        _ => return (Vec::new(), Vec::new()),
    };

    if parser.set_language(&language).is_err() {
        return (Vec::new(), Vec::new());
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return (Vec::new(), Vec::new()),
    };

    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    walk_node(
        match language {
            _ if language == python::language() => LanguageKind::Python,
            _ => LanguageKind::Rust,
        },
        tree.root_node(),
        file,
        source,
        &mut symbols,
        &mut imports,
        None,
    );

    (symbols, imports)
}

pub fn build_context(
    index: &IndexHandle,
    diff: &DiffAnalysis,
) -> Option<ContextSlice> {
    let facts = index.facts.read().ok()?.clone()?;
    let _symbols = index.symbols.read().ok()?.clone()?;

    let file = PathBuf::from(&diff.file);
    let abs = index.repo_root.join(&file);

    let source = fs::read_to_string(&abs).unwrap_or_default();
    let (local_symbols, imports) = parse_file(&file, &source);

    let symbol_resolution = resolve_symbol(diff.symbol.as_deref(), &local_symbols);

    Some(ContextSlice {
        repo_facts: facts,
        diff_target: DiffTarget {
            file,
            symbol: diff.symbol.clone(),
        },
        symbol_resolution,
        local_symbols,
        imports,
    })
}


fn extract_repo_facts(repo_root: &Path) -> RepoFacts {
    let mut languages = Vec::new();

    if repo_root.join("Cargo.toml").exists() {
        languages.push(Language::Rust);
    }

    if repo_root.join("pyproject.toml").exists()
        || repo_root.join("setup.py").exists()
        || repo_root.join("requirements.txt").exists()
    {
        languages.push(Language::Python);
    }

    RepoFacts {
        languages,
        forbidden_deps: vec!["torch".into(), "tensorflow".into(), "jax".into()],
    }
}

fn detect_code_roots(repo_root: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    let src = repo_root.join("src");
    if src.is_dir() {
        roots.push(src);
    }

    let name = repo_root.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let named = repo_root.join(name);
    if named.is_dir() {
        roots.push(named);
    }

    if roots.is_empty() {
        roots.push(repo_root.to_path_buf());
    }

    roots
}

fn build_symbol_index(
    repo_root: &Path,
    code_roots: &[PathBuf],
) -> SymbolIndex {
    let mut by_file = HashMap::new();

    for root in code_roots {
        for entry in WalkDir::new(root)
            .into_iter()
            .filter_entry(|e| !is_ignored(e))
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let abs = entry.path();

            let rel = abs.strip_prefix(repo_root).unwrap_or(abs).to_path_buf();

            let lang = match rel.extension().and_then(|s| s.to_str()) {
                Some("py") => LanguageKind::Python,
                Some("rs") => LanguageKind::Rust,
                _ => continue,
            };

            if let Some(symbols) = index_file(abs, &rel, lang) {
                by_file.insert(rel, symbols);
            }
        }
    }

    SymbolIndex { by_file }
}


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

    walk_node(kind, root, rel_path, &src, &mut symbols, &mut imports, None);

    Some(FileSymbols { symbols, imports })
}

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
                            walk_node(kind, c, file, src, symbols, imports, Some(cls.clone()));
                        }
                    }
                    return;
                }
            }
        }

        (LanguageKind::Python, "function_definition") => {
            if let Some(n) = node.child_by_field_name("name") {
                if let Ok(name) = n.utf8_text(src.as_bytes()) {
                    let full = match &current_class {
                        Some(c) => format!("{c}.{name}"),
                        None => name.to_string(),
                    };

                    symbols.push(SymbolDef {
                        name: full,
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }

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
            walk_node(kind, c, file, src, symbols, imports, current_class.clone());
        }
    }
}

fn resolve_symbol(
    target: Option<&str>,
    symbols: &[SymbolDef],
) -> SymbolResolution {
    let Some(sym) = target else {
        return SymbolResolution::NotFound;
    };

    let matches: Vec<_> = symbols
        .iter()
        .cloned()
        .filter(|s| s.name == sym || s.name.ends_with(&format!(".{sym}")))
        .collect();

    match matches.len() {
        0 => SymbolResolution::NotFound,
        1 => SymbolResolution::Resolved(matches[0].clone()),
        _ => SymbolResolution::Ambiguous(matches),
    }
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
