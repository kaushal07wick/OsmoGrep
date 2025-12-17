use crate::git;
use std::collections::HashSet;
use tree_sitter::{Node, Parser};

/* ============================================================
   Public API
   ============================================================ */

/// Detects the enclosing symbol affected by a diff hunk.
/// Works on **STAGED CODE**, not working tree.
pub fn detect_symbol(file: &str, hunks: &str) -> Option<String> {
    // Only analyze source files
    if !(file.ends_with(".py") || file.ends_with(".rs")) {
        return None;
    }

    // 🔥 IMPORTANT:
    // Read from INDEX first (staged), fallback to HEAD
    let source = git::show_index(file)
        .or_else(|| git::show_head(file))?;

    let line_offsets = compute_line_offsets(&source);
    let ranges = changed_byte_ranges(hunks, &line_offsets);

    if ranges.is_empty() {
        return None;
    }

    let mut parser = Parser::new();

    if file.ends_with(".py") {
        parser
            .set_language(&tree_sitter_python::language())
            .ok()?;
    } else if file.ends_with(".rs") {
        parser
            .set_language(&tree_sitter_rust::language())
            .ok()?;
    }

    let tree = parser.parse(&source, None)?;
    let root = tree.root_node();

    let mut seen = HashSet::new();

    for (start, end) in ranges {
        if let Some(sym) = find_enclosing_symbol(root, &source, start, end) {
            if seen.insert(sym.clone()) {
                return Some(sym);
            }
        }
    }

    None
}

/* ============================================================
   Line → byte mapping
   ============================================================ */

fn compute_line_offsets(src: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

fn changed_byte_ranges(
    hunks: &str,
    line_offsets: &[usize],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    for line in hunks.lines() {
        if let Some((start_line, count)) = parse_hunk_header(line) {
            let start_idx = start_line.saturating_sub(1);
            let end_idx = start_idx + count;

            let start = *line_offsets.get(start_idx).unwrap_or(&0);
            let end = *line_offsets
                .get(end_idx)
                .unwrap_or_else(|| line_offsets.last().unwrap_or(&0));

            ranges.push((start, end));
        }
    }

    ranges
}

fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    // @@ -x,y +a,b @@
    if !line.starts_with("@@") {
        return None;
    }

    let plus = line.split('+').nth(1)?;
    let nums = plus.split_whitespace().next()?;
    let mut it = nums.split(',');

    let start = it.next()?.parse().ok()?;
    let len = it.next().unwrap_or("1").parse().ok()?;

    Some((start, len))
}

/* ============================================================
   AST traversal
   ============================================================ */

fn find_enclosing_symbol(
    node: Node,
    source: &str,
    start: usize,
    end: usize,
) -> Option<String> {
    if node.end_byte() < start || node.start_byte() > end {
        return None;
    }

    match node.kind() {
        // Python
        "function_definition" => return extract_python_function(node, source),
        "class_definition" => return extract_python_class(node, source),

        // Rust
        "function_item" => return extract_rust_named(node, source, "fn "),
        "struct_item" => return extract_rust_named(node, source, "struct "),
        "enum_item" => return extract_rust_named(node, source, "enum "),
        "impl_item" => return extract_rust_impl(node, source),

        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(sym) = find_enclosing_symbol(child, source, start, end) {
            return Some(sym);
        }
    }

    None
}

/* ============================================================
   Python extraction
   ============================================================ */

fn extract_python_function(node: Node, source: &str) -> Option<String> {
    let name = python_identifier(node, source)?;
    let class = enclosing_python_class(node, source);

    Some(match class {
        Some(cls) => format!("{}.{}", cls, name),
        None => name,
    })
}

fn extract_python_class(node: Node, source: &str) -> Option<String> {
    python_identifier(node, source)
}

fn python_identifier(node: Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child
                .utf8_text(source.as_bytes())
                .ok()
                .map(|s| s.to_string());
        }
    }
    None
}

fn enclosing_python_class(node: Node, source: &str) -> Option<String> {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "class_definition" {
            return python_identifier(n, source);
        }
        cur = n.parent();
    }
    None
}

/* ============================================================
   Rust extraction
   ============================================================ */

fn extract_rust_named(
    node: Node,
    source: &str,
    prefix: &str,
) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|n| format!("{}{}", prefix, n))
}

fn extract_rust_impl(node: Node, source: &str) -> Option<String> {
    node.child_by_field_name("type")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(|t| format!("impl {}", t))
}
