// src/context/snapshot.rs
//
// Diff-scoped + repo-level context construction.

use std::fs;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use crate::state::DiffAnalysis;
use crate::context::types::{
    Import, SymbolDef, SymbolResolution,
    FileContext, ContextSnapshot, LanguageKind,
    FullContextSnapshot,
};
use crate::context::test_snapshot::build_test_context_snapshot;

/// Build FULL context snapshot:
/// - diff-scoped code context
/// - repo-level test context
pub fn build_full_context_snapshot(
    repo_root: &Path,
    diffs: &[DiffAnalysis],
) -> FullContextSnapshot {
    let code = build_context_snapshot(repo_root, diffs);
    let tests = build_test_context_snapshot(repo_root);

    FullContextSnapshot { code, tests }
}

/// Existing diff-scoped context builder (unchanged)
pub fn build_context_snapshot(
    repo_root: &Path,
    diffs: &[DiffAnalysis],
) -> ContextSnapshot {
    let mut files = Vec::new();

    for diff in diffs {
        let rel_path = PathBuf::from(&diff.file);
        let abs_path = repo_root.join(&rel_path);

        let source = fs::read_to_string(&abs_path).unwrap_or_default();
        let (symbols, imports) = parse_file(&rel_path, &source);
        let resolution = resolve_symbol(diff.symbol.as_deref(), &symbols);

        files.push(FileContext {
            path: rel_path,
            symbols,
            imports,
            symbol_resolution: resolution,
        });
    }

    ContextSnapshot { files }
}

/// Reused by both code + test snapshots
pub fn parse_file(
    file: &Path,
    source: &str,
) -> (Vec<SymbolDef>, Vec<Import>) {
    let mut parser = Parser::new();

    let (language, kind) = match file.extension().and_then(|s| s.to_str()) {
        Some("py") => (python::language(), LanguageKind::Python),
        Some("rs") => (rust::language(), LanguageKind::Rust),
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
        kind,
        tree.root_node(),
        file,
        source,
        &mut symbols,
        &mut imports,
        None,
    );

    (symbols, imports)
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
        // ---------- Python ----------
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

        // ---------- Rust ----------
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

        // ---------- Imports ----------
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
