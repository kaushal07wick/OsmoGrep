// src/context/engine.rs
//
// Context slicing engine.
//
// GUARANTEES:
// - Diff is authoritative
// - Only diff file is parsed
// - Full-file scan ONLY to resolve symbol
// - Deterministic
// - No guessing
//

use std::fs;
use std::path::{Path, PathBuf};

use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use crate::state::DiffAnalysis;

use super::types::{
    AssertionStyle,
    ContextSlice,
    DiffTarget,
    ExecutionModel,
    FailureMode,
    Import,
    RepoFacts,
    RepoFactsLite,
    SymbolDef,
    SymbolResolution,
    TestContext,
    TestMatchKind,
};

/* ============================================================
   Engine
   ============================================================ */

pub struct ContextEngine<'a> {
    repo_root: &'a Path,
    facts: &'a RepoFacts,
    test_roots: &'a [PathBuf],
}

impl<'a> ContextEngine<'a> {
    pub fn new(
        repo_root: &'a Path,
        facts: &'a RepoFacts,
        _symbols: &'a super::types::SymbolIndex, // intentionally ignored
        test_roots: &'a [PathBuf],
    ) -> Self {
        Self {
            repo_root,
            facts,
            test_roots,
        }
    }

    /* ========================================================
       Public entry (DIFF-ONLY)
       ======================================================== */

    pub fn slice_from_diff(&self, diff: &DiffAnalysis) -> ContextSlice {
        let file = PathBuf::from(&diff.file);
        let abs_file = self.repo_root.join(&file);

        let source = fs::read_to_string(&abs_file).unwrap_or_default();

        let (symbols, imports) = parse_file(&file, &source);

        let symbol_resolution =
            resolve_symbol(diff.symbol.as_deref(), &symbols);

        let test_context =
            self.build_test_context(&file, diff.symbol.as_deref());

        ContextSlice {
            repo_facts: RepoFactsLite::from(self.facts),

            diff_target: DiffTarget {
                file,
                symbol: diff.symbol.clone(),
            },

            symbol_resolution,

            local_symbols: symbols,
            imports,

            test_context,

            execution_model: None,
            test_intent: None,
            assertion_style: None,
            failure_modes: Vec::new(),
        }
    }

    /* ========================================================
       Test discovery (STRICT, DIFF-DRIVEN)
       ======================================================== */

    fn build_test_context(
        &self,
        src_file: &Path,
        symbol: Option<&str>,
    ) -> TestContext {
        let candidates = find_candidate_tests(self.test_roots, src_file);

        if let Some(sym) = symbol {
            let hits = match_symbol_in_tests(&candidates, sym);
            if !hits.is_empty() {
                return TestContext {
                    existing_tests: hits.clone(),
                    recommended_location: hits[0].clone(),
                    match_kind: TestMatchKind::Content,
                };
            }

            return TestContext {
                existing_tests: Vec::new(),
                recommended_location: default_test_path(
                    self.repo_root,
                    src_file,
                    Some(sym),
                ),
                match_kind: TestMatchKind::None,
            };
        }

        if !candidates.is_empty() {
            return TestContext {
                existing_tests: candidates.clone(),
                recommended_location: candidates[0].clone(),
                match_kind: TestMatchKind::Filename,
            };
        }

        TestContext {
            existing_tests: Vec::new(),
            recommended_location: default_test_path(
                self.repo_root,
                src_file,
                None,
            ),
            match_kind: TestMatchKind::None,
        }
    }
}

/* ============================================================
   File parsing (AUTHORITATIVE)
   ============================================================ */

fn parse_file(
    file: &Path,
    source: &str,
) -> (Vec<SymbolDef>, Vec<Import>) {
    let mut parser = Parser::new();

    let lang = match file.extension().and_then(|s| s.to_str()) {
        Some("py") => python::language(),
        Some("rs") => rust::language(),
        _ => return (Vec::new(), Vec::new()),
    };

    if parser.set_language(&lang).is_err() {
        return (Vec::new(), Vec::new());
    }

    let Some(tree) = parser.parse(source, None) else {
        return (Vec::new(), Vec::new());
    };

    let mut symbols = Vec::new();
    let mut imports = Vec::new();

    walk(
        tree.root_node(),
        file,
        source,
        &mut symbols,
        &mut imports,
        None,
    );

    (symbols, imports)
}

fn walk(
    node: Node,
    file: &Path,
    src: &str,
    symbols: &mut Vec<SymbolDef>,
    imports: &mut Vec<Import>,
    current_class: Option<String>,
) {
    match node.kind() {
        "class_definition" => {
            if let Some(n) = node.child_by_field_name("name") {
                if let Ok(name) = n.utf8_text(src.as_bytes()) {
                    let cls = name.to_string();
                    symbols.push(SymbolDef {
                        name: cls.clone(),
                        file: file.to_path_buf(),
                        line: node.start_position().row + 1,
                    });

                    for i in 0..node.child_count() {
                        if let Some(c) = node.child(i) {
                            walk(
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

        "function_definition" => {
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

        "import_statement" | "import_from_statement" => {
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
            walk(
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
   Symbol resolution (STRICT)
   ============================================================ */

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

/* ============================================================
   Test helpers (PURE FUNCTIONS)
   ============================================================ */

fn find_candidate_tests(
    test_roots: &[PathBuf],
    src_file: &Path,
) -> Vec<PathBuf> {
    let stem = src_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut out = Vec::new();

    for root in test_roots {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("py") {
                continue;
            }

            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.contains(stem) || name.starts_with("test_") {
                out.push(path.to_path_buf());
            }
        }
    }

    out
}

fn match_symbol_in_tests(
    files: &[PathBuf],
    symbol: &str,
) -> Vec<PathBuf> {
    let variants = symbol_variants(symbol);

    files
        .iter()
        .filter(|p| {
            fs::read_to_string(p)
                .map(|src| variants.iter().any(|v| src.contains(v)))
                .unwrap_or(false)
        })
        .cloned()
        .collect()
}

fn symbol_variants(symbol: &str) -> Vec<String> {
    let base = symbol.split('.').last().unwrap_or(symbol);

    vec![
        format!("def {}", base),
        format!("class {}", base),
        format!("test_{}", base),
        format!("test_{}", symbol.replace('.', "_")),
    ]
}

fn default_test_path(
    repo_root: &Path,
    src_file: &Path,
    symbol: Option<&str>,
) -> PathBuf {
    let root = repo_root.join("tests");

    let name = symbol
        .map(|s| format!("test_{s}.py"))
        .unwrap_or_else(|| {
            format!(
                "test_{}.py",
                src_file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
            )
        });

    root.join(name)
}
