use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

use blake3::Hasher;

/* ======================= CONFIG ======================= */

const CONTEXT_DIR: &str = ".context";
const CONTEXT_FILE: &str = "context.json";
const META_FILE: &str = "meta.json";

/* ======================= DATA MODEL ======================= */

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone)]
pub struct RepoStats {
    pub file_count: usize,
    pub total_bytes: u64,
    pub total_lines: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub path: String,
    pub language: String,
    pub bytes: u64,
    pub lines: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Symbol {
    pub kind: String,
    pub name: String,
    pub file: String,

    pub inputs: Vec<String>,
    pub input_types: Vec<String>,
    pub output: String,

    pub calls: Vec<String>,
    pub custom_calls: Vec<String>,
    pub lang_calls: Vec<String>,
    pub called_by: Vec<String>,

    pub doc: Option<String>,

    pub line_start: usize,
    pub line_end: usize,
}

#[derive(Serialize, Deserialize)]
pub struct Context {
    pub stats: RepoStats,
    pub files: Vec<FileInfo>,
    pub symbols: Vec<Symbol>,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    stats: RepoStats,
    file_hashes: HashMap<String, String>,
}

/* ======================= PUBLIC ENTRY ======================= */

pub fn load_or_build(root: impl AsRef<Path>) -> Context {
    let root = root.as_ref();
    let ctx_dir = root.join(CONTEXT_DIR);
    let ctx_path = ctx_dir.join(CONTEXT_FILE);
    let meta_path = ctx_dir.join(META_FILE);

    let current_stats = compute_repo_stats(root);
    let current_hashes = compute_file_hashes(root);

    if let (Ok(ctx_raw), Ok(meta_raw)) =
        (fs::read_to_string(&ctx_path), fs::read_to_string(&meta_path))
    {
        if let (Ok(mut ctx), Ok(meta)) = (
            serde_json::from_str::<Context>(&ctx_raw),
            serde_json::from_str::<Meta>(&meta_raw),
        ) {
            if meta.stats == current_stats {
                incremental_update(
                    root,
                    &mut ctx,
                    &meta.file_hashes,
                    &current_hashes,
                );

                fs::write(
                    &ctx_path,
                    serde_json::to_string_pretty(&ctx).unwrap(),
                )
                .unwrap();

                fs::write(
                    &meta_path,
                    serde_json::to_string_pretty(&Meta {
                        stats: current_stats,
                        file_hashes: current_hashes,
                    })
                    .unwrap(),
                )
                .unwrap();

                return ctx;
            }
        }
    }

    let ctx = build_context(root, current_stats.clone(), &current_hashes);

    fs::create_dir_all(&ctx_dir).ok();
    fs::write(&ctx_path, serde_json::to_string_pretty(&ctx).unwrap()).unwrap();
    fs::write(
        &meta_path,
        serde_json::to_string_pretty(&Meta {
            stats: current_stats,
            file_hashes: current_hashes,
        })
        .unwrap(),
    )
    .unwrap();

    ctx
}

/* ======================= IGNORE RULES ======================= */

fn should_ignore(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_string_lossy().as_ref(),
            ".git"
                | ".venv"
                | "venv"
                | "env"
                | ".env"
                | "__pycache__"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".out"
                | ".cache"
                | ".idea"
                | ".vscode"
        )
    })
}

fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()? {
        "py" => Some("python"),
        "rs" => Some("rust"),
        _ => None,
    }
}

/* ======================= HASHING ======================= */

fn hash_file(path: &Path) -> Option<String> {
    let data = fs::read(path).ok()?;
    let mut h = Hasher::new();
    h.update(&data);
    Some(h.finalize().to_hex().to_string())
}

fn compute_file_hashes(root: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();

    for e in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let p = e.path();
        if !e.file_type().is_file() || should_ignore(p) {
            continue;
        }
        if detect_language(p).is_none() {
            continue;
        }
        if let Some(h) = hash_file(p) {
            out.insert(p.display().to_string(), h);
        }
    }

    out
}

/* ======================= STATS ======================= */

fn compute_repo_stats(root: &Path) -> RepoStats {
    let mut stats = RepoStats {
        file_count: 0,
        total_bytes: 0,
        total_lines: 0,
    };

    for e in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !e.file_type().is_file() || should_ignore(e.path()) {
            continue;
        }

        if detect_language(e.path()).is_some() {
            if let Ok(meta) = e.metadata() {
                stats.file_count += 1;
                stats.total_bytes += meta.len();
                if let Ok(src) = fs::read_to_string(e.path()) {
                    stats.total_lines += src.lines().count();
                }
            }
        }
    }

    stats
}

/* ======================= INCREMENTAL ======================= */

fn incremental_update(
    root: &Path,
    ctx: &mut Context,
    old: &HashMap<String, String>,
    new: &HashMap<String, String>,
) {
    let changed: HashSet<_> = new
        .iter()
        .filter(|(p, h)| old.get(*p) != Some(h))
        .map(|(p, _)| p.clone())
        .collect();

    let removed: HashSet<_> = old
        .keys()
        .filter(|p| !new.contains_key(*p))
        .cloned()
        .collect();

    ctx.symbols.retain(|s| !removed.contains(&s.file));
    ctx.files.retain(|f| !removed.contains(&f.path));

    ctx.symbols.retain(|s| !changed.contains(&s.file));
    ctx.files.retain(|f| !changed.contains(&f.path));

    for path in &changed {
        let p = Path::new(path);
        let Some(lang) = detect_language(p) else { continue };
        let Ok(src) = fs::read_to_string(p) else { continue };

        match lang {
            "python" => extract_python(&src, path, &mut ctx.symbols),
            "rust" => extract_rust(&src, path, &mut ctx.symbols),
            _ => {}
        }

        let bytes = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let lines = src.lines().count();

        ctx.files.push(FileInfo {
            path: path.clone(),
            language: lang.into(),
            bytes,
            lines,
        });
    }

    finalize_calls(&mut ctx.symbols);
}

/* ======================= CALL EXTRACTION ======================= */

fn collect_calls(node: Node, src: &str, out: &mut HashSet<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "call" | "call_expression" | "method_call_expression" => {
                if let Some(f) = child.child(0) {
                    if let Ok(txt) = f.utf8_text(src.as_bytes()) {
                        if let Some(name) = txt.split(&['.', ':'][..]).last() {
                            out.insert(name.to_string());
                        }
                    }
                }
            }
            _ => collect_calls(child, src, out),
        }
    }
}

/* ======================= DOCS ======================= */

fn python_doc(node: Node, src: &str) -> Option<String> {
    let body = node.child_by_field_name("body")?;
    let first = body.named_child(0)?;
    if first.kind() == "expression_statement" {
        let s = first.named_child(0)?;
        if s.kind() == "string" {
            return s
                .utf8_text(src.as_bytes())
                .ok()
                .map(|x| x.trim_matches(&['"', '\''][..]).to_string());
        }
    }
    None
}

fn rust_doc(node: Node, src: &str) -> Option<String> {
    let mut docs = Vec::new();
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "line_comment" {
            if let Ok(t) = ch.utf8_text(src.as_bytes()) {
                if t.starts_with("///") {
                    docs.push(t[3..].trim().to_string());
                }
            }
        }
    }
    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

/* ======================= BUILD ======================= */

fn build_context(
    root: &Path,
    stats: RepoStats,
    hashes: &HashMap<String, String>,
) -> Context {
    let mut files = Vec::new();
    let mut symbols = Vec::new();

    for (path, _) in hashes {
        let p = Path::new(path);
        let Some(lang) = detect_language(p) else { continue };
        let Ok(src) = fs::read_to_string(p) else { continue };

        match lang {
            "python" => extract_python(&src, path, &mut symbols),
            "rust" => extract_rust(&src, path, &mut symbols),
            _ => {}
        }

        let bytes = fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        let lines = src.lines().count();

        files.push(FileInfo {
            path: path.clone(),
            language: lang.into(),
            bytes,
            lines,
        });
    }

    finalize_calls(&mut symbols);

    Context {
        stats,
        files,
        symbols,
    }
}

/* ======================= PYTHON EXTRACTION ======================= */

fn extract_python(src: &str, file: &str, out: &mut Vec<Symbol>) {
    let mut p = Parser::new();
    p.set_language(&python::language()).ok();
    let Some(t) = p.parse(src, None) else { return };

    let root = t.root_node();
    let mut cursor = root.walk();

    for node in root.children(&mut cursor) {
        match node.kind() {
            "function_definition" => out.push(extract_python_fn(node, src, file)),
            "class_definition" => out.push(extract_python_class(node, src, file)),
            _ => {}
        }
    }
}

fn extract_python_fn(n: Node, src: &str, file: &str) -> Symbol {
    let name = n
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src.as_bytes()).ok())
        .unwrap_or("<?>")
        .to_string();

    let mut inputs = Vec::new();
    let mut input_types = Vec::new();

    if let Some(params) = n.child_by_field_name("parameters") {
        let mut c = params.walk();
        for p in params.children(&mut c) {
            if p.kind() == "identifier" {
                inputs.push(p.utf8_text(src.as_bytes()).unwrap().to_string());
                input_types.push("unknown".into());
            } else if p.kind() == "typed_parameter" {
                let name = p
                    .child_by_field_name("name")
                    .and_then(|x| x.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("<?>");
                let ty = p
                    .child_by_field_name("type")
                    .and_then(|x| x.utf8_text(src.as_bytes()).ok())
                    .unwrap_or("unknown");
                inputs.push(name.to_string());
                input_types.push(ty.to_string());
            }
        }
    }

    let output = n
        .child_by_field_name("return_type")
        .and_then(|x| x.utf8_text(src.as_bytes()).ok())
        .unwrap_or("unknown")
        .to_string();

    let mut calls = HashSet::new();
    collect_calls(n, src, &mut calls);

    Symbol {
        kind: "function".into(),
        name,
        file: file.into(),
        inputs,
        input_types,
        output,
        calls: calls.into_iter().collect(),
        custom_calls: vec![],
        lang_calls: vec![],
        called_by: vec![],
        doc: python_doc(n, src),
        line_start: n.start_position().row + 1,
        line_end: n.end_position().row + 1,
    }
}

fn extract_python_class(n: Node, src: &str, file: &str) -> Symbol {
    let name = n
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(src.as_bytes()).ok())
        .unwrap_or("<?>")
        .to_string();

    let mut calls = HashSet::new();
    collect_calls(n, src, &mut calls);

    Symbol {
        kind: "class".into(),
        name,
        file: file.into(),
        inputs: vec![],
        input_types: vec![],
        output: "unknown".into(),
        calls: calls.into_iter().collect(),
        custom_calls: vec![],
        lang_calls: vec![],
        called_by: vec![],
        doc: python_doc(n, src),
        line_start: n.start_position().row + 1,
        line_end: n.end_position().row + 1,
    }
}

/* ======================= RUST EXTRACTION ======================= */

fn extract_rust(src: &str, file: &str, out: &mut Vec<Symbol>) {
    let mut p = Parser::new();
    p.set_language(&rust::language()).ok();
    let Some(t) = p.parse(src, None) else { return };

    let root = t.root_node();
    let mut cursor = root.walk();

    for n in root.children(&mut cursor) {
        if n.kind() != "function_item" {
            continue;
        }

        let name = n
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(src.as_bytes()).ok())
            .unwrap_or("<?>")
            .to_string();

        let mut inputs = Vec::new();
        let mut input_types = Vec::new();

        if let Some(params) = n.child_by_field_name("parameters") {
            let mut c = params.walk();
            for p in params.children(&mut c) {
                if let Some(pat) = p.child_by_field_name("pattern") {
                    inputs.push(pat.utf8_text(src.as_bytes()).unwrap().to_string());
                    let ty = p
                        .child_by_field_name("type")
                        .and_then(|t| t.utf8_text(src.as_bytes()).ok())
                        .unwrap_or("unknown");
                    input_types.push(ty.to_string());
                }
            }
        }

        let output = n
            .child_by_field_name("return_type")
            .and_then(|x| x.utf8_text(src.as_bytes()).ok())
            .unwrap_or("()")
            .to_string();

        let mut calls = HashSet::new();
        collect_calls(n, src, &mut calls);

        out.push(Symbol {
            kind: "function".into(),
            name,
            file: file.into(),
            inputs,
            input_types,
            output,
            calls: calls.into_iter().collect(),
            custom_calls: vec![],
            lang_calls: vec![],
            called_by: vec![],
            doc: rust_doc(n, src),
            line_start: n.start_position().row + 1,
            line_end: n.end_position().row + 1,
        });
    }
}

/* ======================= POST PROCESS ======================= */

fn finalize_calls(symbols: &mut Vec<Symbol>) {
    let names: HashSet<String> = symbols.iter().map(|s| s.name.clone()).collect();
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();

    for s in symbols.iter_mut() {
        for c in &s.calls {
            if names.contains(c) {
                s.custom_calls.push(c.clone());
                reverse.entry(c.clone()).or_default().push(s.name.clone());
            } else {
                s.lang_calls.push(c.clone());
            }
        }
        s.custom_calls.sort();
        s.lang_calls.sort();
    }

    for s in symbols.iter_mut() {
        if let Some(v) = reverse.get(&s.name) {
            let mut callers = v.clone();
            callers.sort();
            callers.dedup();
            s.called_by = callers;
        }
    }
}
