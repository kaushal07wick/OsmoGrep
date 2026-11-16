use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use tree_sitter::{Node, Parser};
use tree_sitter_python as python;
use tree_sitter_rust as rust;

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
    #[serde(default)]
    index_version: u32,
    #[serde(default)]
    context_digest: String,
    stats: RepoStats,
    file_hashes: HashMap<String, String>,
}

/* ======================= PUBLIC ENTRY ======================= */

/// Shared extraction for the optional committed-revision graph; no cache writes.
pub fn extract_symbols(source: &str, path: &str) -> Vec<Symbol> {
    let mut symbols = Vec::new();
    match Path::new(path).extension().and_then(|s| s.to_str()) {
        Some("rs") => extract_rust(source, path, &mut symbols),
        Some("py") => extract_python(source, path, &mut symbols),
        _ => {}
    }
    finalize_calls(&mut symbols);
    symbols
}

pub fn load_or_build(root: impl AsRef<Path>) -> Result<Context, String> {
    const INDEX_VERSION: u32 = 2;
    let root = root.as_ref();
    let ctx_dir = crate::file_guard::internal_dir(root, CONTEXT_DIR)?;
    let _lock = crate::storage::lock_file(&ctx_dir.join("index.lock"))?;
    let ctx_path = ctx_dir.join(CONTEXT_FILE);
    let meta_path = ctx_dir.join(META_FILE);
    let (current_stats, current_hashes) = scan_sources(root)?;
    let saved = fs::read(&ctx_path)
        .ok()
        .and_then(|bytes| {
            serde_json::from_slice::<Context>(&bytes)
                .ok()
                .map(|context| (context, blake3::hash(&bytes).to_hex().to_string()))
        })
        .zip(
            fs::read(&meta_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Meta>(&bytes).ok()),
        );
    let ctx = if let Some(((mut ctx, _), meta)) = saved.filter(|((_, digest), meta)| {
        meta.index_version == INDEX_VERSION && *digest == meta.context_digest
    }) {
        if meta.file_hashes == current_hashes && meta.stats == current_stats {
            return Ok(ctx);
        }
        incremental_update(root, &mut ctx, &meta.file_hashes, &current_hashes);
        ctx.stats = current_stats.clone();
        ctx
    } else {
        build_context(root, current_stats.clone(), &current_hashes)
    };
    let context_bytes = serde_json::to_vec(&ctx).map_err(|e| e.to_string())?;
    let meta = Meta {
        index_version: INDEX_VERSION,
        context_digest: blake3::hash(&context_bytes).to_hex().to_string(),
        stats: current_stats,
        file_hashes: current_hashes,
    };
    crate::storage::atomic_write(&ctx_path, &context_bytes).map_err(|e| e.to_string())?;
    crate::storage::atomic_write(
        &meta_path,
        &serde_json::to_vec(&meta).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(ctx)
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

/* ======================= SOURCE SCAN ======================= */

fn read_source(path: &Path) -> std::io::Result<Vec<u8>> {
    const SOURCE_LIMIT: u64 = 20 * 1024 * 1024;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(SOURCE_LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > SOURCE_LIMIT {
        return Err(std::io::Error::other(format!(
            "{} exceeds the 20 MiB index budget",
            path.display()
        )));
    }
    Ok(bytes)
}

fn scan_sources(root: &Path) -> Result<(RepoStats, HashMap<String, String>), String> {
    let mut hashes = HashMap::new();
    let mut stats = RepoStats {
        file_count: 0,
        total_bytes: 0,
        total_lines: 0,
    };
    let entries = WalkDir::new(root).into_iter().filter_entry(|entry| {
        entry.depth() == 0
            || !should_ignore(entry.path().strip_prefix(root).unwrap_or(entry.path()))
    });
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !entry.file_type().is_file() || detect_language(path).is_none() {
            continue;
        }
        let bytes = read_source(path).map_err(|e| e.to_string())?;
        stats.file_count += 1;
        stats.total_bytes += bytes.len() as u64;
        stats.total_lines += std::str::from_utf8(&bytes)
            .map(|text| text.lines().count())
            .unwrap_or(0);
        hashes.insert(
            path.display().to_string(),
            blake3::hash(&bytes).to_hex().to_string(),
        );
    }
    Ok((stats, hashes))
}

/* ======================= INCREMENTAL ======================= */

fn incremental_update(
    _root: &Path,
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
        let Some(lang) = detect_language(p) else {
            continue;
        };
        let Ok(src) = read_source(p)
            .and_then(|bytes| String::from_utf8(bytes).map_err(std::io::Error::other))
        else {
            continue;
        };

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

fn build_context(_root: &Path, stats: RepoStats, hashes: &HashMap<String, String>) -> Context {
    let mut files = Vec::new();
    let mut symbols = Vec::new();

    for (path, _) in hashes {
        let p = Path::new(path);
        let Some(lang) = detect_language(p) else {
            continue;
        };
        let Ok(src) = read_source(p)
            .and_then(|bytes| String::from_utf8(bytes).map_err(std::io::Error::other))
        else {
            continue;
        };

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
        s.custom_calls.clear();
        s.lang_calls.clear();
        s.called_by.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_prunes_generated_trees_and_updates_changed_and_removed_sources() {
        let temporary = std::env::temp_dir().join(format!("indexer-{}", uuid::Uuid::new_v4()));
        let root = temporary.join("target/project");
        fs::create_dir_all(root.join("node_modules/dependency")).unwrap();
        fs::write(
            root.join("node_modules/dependency/ignored.py"),
            "def hidden(): pass\n",
        )
        .unwrap();
        let source = root.join("app.py");
        fs::write(&source, "def old(): pass\n").unwrap();
        let context = load_or_build(&root).unwrap();
        assert_eq!(context.stats.file_count, 1);
        assert!(context.symbols.iter().any(|symbol| symbol.name == "old"));
        fs::write(&source, "def changed_name():\n    return 123\n").unwrap();
        let updated = load_or_build(&root).unwrap();
        assert_eq!(updated.stats.total_lines, 2);
        assert!(updated
            .symbols
            .iter()
            .any(|symbol| symbol.name == "changed_name"));
        assert!(!updated.symbols.iter().any(|symbol| symbol.name == "old"));
        // A crash between cache writes must not make the next run trust a mismatched pair.
        let cache = root.join(".context/context.json");
        let mut broken: serde_json::Value =
            serde_json::from_slice(&fs::read(&cache).unwrap()).unwrap();
        broken["symbols"] = serde_json::json!([]);
        fs::write(&cache, serde_json::to_vec(&broken).unwrap()).unwrap();
        assert!(load_or_build(&root)
            .unwrap()
            .symbols
            .iter()
            .any(|symbol| symbol.name == "changed_name"));
        fs::remove_file(&source).unwrap();
        let removed = load_or_build(&root).unwrap();
        assert!(removed.files.is_empty() && removed.symbols.is_empty());
        assert_eq!(removed.stats.file_count, 0);
    }

    #[cfg(unix)]
    #[test]
    fn linked_cache_directory_cannot_redirect_index_writes() {
        let temporary = std::env::temp_dir().join(format!("indexer-link-{}", uuid::Uuid::new_v4()));
        let root = temporary.join("project");
        let outside = temporary.join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, root.join(".context")).unwrap();
        assert!(load_or_build(&root).is_err());
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}
