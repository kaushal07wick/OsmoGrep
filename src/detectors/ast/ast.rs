//! detectors/ast/ast.rs
//!
//! AST-based symbol detection utilities.

use tree_sitter::{Node, Parser};
use std::cell::RefCell;

thread_local! {
    static PY_PARSER: RefCell<Parser> = RefCell::new(make_python_parser());
    static RS_PARSER: RefCell<Parser> = RefCell::new(make_rust_parser());
}

fn make_python_parser() -> Parser {
    let mut p = Parser::new();
    p.set_language(&tree_sitter_python::language()).unwrap();
    p
}

fn make_rust_parser() -> Parser {
    let mut p = Parser::new();
    p.set_language(&tree_sitter_rust::language()).unwrap();
    p
}

/// Detect symbol affected by a diff hunk.
///
/// `source` must be the full file content.
/// `hunks` is the unified diff chunk.
pub fn detect_symbol(
    source: &str,
    hunks: &str,
    file: &str,
) -> Option<String> {
    if !is_supported(file) {
        return None;
    }

    let line_offsets = compute_line_offsets(source);
    let ranges = changed_byte_ranges(hunks, &line_offsets);
    if ranges.is_empty() {
        return None;
    }

    let tree = parse_source(file, source)?;
    let root = tree.root_node();

    let mut best: Option<(usize, String)> = None;

    for (start, end) in ranges {
        collect_enclosing_symbols(root, source, start, end, &mut best);
    }

    best.map(|(_, name)| name)
}

pub fn extract_symbol_source(
    source: &str,
    file: &str,
    symbol: &str,
) -> Option<String> {
    let tree = parse_source(file, source)?;
    let root = tree.root_node();

    find_symbol_node(root, source, symbol)
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .map(str::to_owned)
}

pub fn is_supported(file: &str) -> bool {
    file.ends_with(".py") || file.ends_with(".rs")
}

pub fn parse_source(file: &str, source: &str) -> Option<tree_sitter::Tree> {
    if file.ends_with(".py") {
        PY_PARSER.with(|p| p.borrow_mut().parse(source, None))
    } else if file.ends_with(".rs") {
        RS_PARSER.with(|p| p.borrow_mut().parse(source, None))
    } else {
        None
    }
}

pub fn compute_line_offsets(src: &str) -> Vec<usize> {
    let mut offsets = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

pub fn changed_byte_ranges(
    hunks: &str,
    offsets: &[usize],
) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();

    for line in hunks.lines() {
        if let Some((start, len)) = parse_hunk_header(line) {
            let s = start.saturating_sub(1);
            let e = s + len;

            let start_byte = *offsets.get(s).unwrap_or(&0);
            let end_byte = *offsets.get(e).unwrap_or_else(|| offsets.last().unwrap());

            ranges.push((start_byte, end_byte));
        }
    }

    ranges
}

pub fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
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

fn collect_enclosing_symbols(
    node: Node,
    source: &str,
    start: usize,
    end: usize,
    best: &mut Option<(usize, String)>,
) {
    if node.end_byte() < start || node.start_byte() > end {
        return;
    }

    if let Some(name) = symbol_name(node, source) {
        let span = node.end_byte() - node.start_byte();
        match best {
            Some((best_span, _)) if *best_span <= span => {}
            _ => *best = Some((span, name)),
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_enclosing_symbols(child, source, start, end, best);
    }
}

fn symbol_name(node: Node, source: &str) -> Option<String> {
    match node.kind() {
        "function_definition" | "class_definition" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(str::to_owned)
        }
        "function_item" | "struct_item" | "enum_item" => {
            node.child_by_field_name("name")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(str::to_owned)
        }
        "impl_item" => {
            node.child_by_field_name("type")
                .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                .map(|t| format!("impl {}", t))
        }
        _ => None,
    }
}

pub fn find_symbol_node<'a>(
    node: Node<'a>,
    source: &str,
    symbol: &str,
) -> Option<Node<'a>> {
    if let Some(name) = symbol_name(node, source) {
        if name == symbol {
            return Some(node);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(n) = find_symbol_node(child, source, symbol) {
            return Some(n);
        }
    }

    None
}
