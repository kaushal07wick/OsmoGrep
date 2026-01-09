use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use regex::Regex;

use crate::context::ast::{parse_source};


#[derive(Debug, Clone)]
pub struct FailureInfo {
    pub test_file: String,
    pub test_name: String,
    pub traceback: String,
}

#[derive(Debug, Clone)]
pub struct FullSuiteContext {
    pub test_path: PathBuf,
    pub test_source: String,
    pub impl_path: PathBuf,
    pub impl_source: String,
    pub function_name: String,
    pub traceback: String,
}

fn extract_test_body(src: &str, method: &str) -> Option<String> {
    let tree = parse_source("test.py", src)?;
    let root = tree.root_node();

    let mut cur = root.walk();
    for node in root.children(&mut cur) {

        if node.kind() == "function_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if name_node.utf8_text(src.as_bytes()).ok()? == method {
                    return node.utf8_text(src.as_bytes()).ok().map(str::to_string);
                }
            }
        }

        if node.kind() == "class_definition" {
            let body = node.child_by_field_name("body")?;
            let mut c2 = body.walk();
            for ch in body.children(&mut c2) {
                if ch.kind() == "function_definition" {
                    if let Some(name_node) = ch.child_by_field_name("name") {
                        if name_node.utf8_text(src.as_bytes()).ok()? == method {
                            return ch.utf8_text(src.as_bytes()).ok().map(str::to_string);
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_called_function(block: &str) -> Option<String> {
    let tree = parse_source("block.py", block)?;
    let root = tree.root_node();
    let mut stack = vec![root];
    
    // Common test assertion methods to skip
    let skip_methods = [
        "assertEqual", "assertNotEqual", "assertTrue", "assertFalse",
        "assertAlmostEqual", "assertNotAlmostEqual", "assertRaises",
        "assertIn", "assertNotIn", "assertIsNone", "assertIsNotNone",
        "assertGreater", "assertLess", "assertGreaterEqual", "assertLessEqual",
        "assertRegex", "assertNotRegex", "assertCountEqual",
        "fail", "skipTest", "subTest",
    ];

    let mut candidates = Vec::new();

    while let Some(node) = stack.pop() {
        if node.kind() == "call" {
            if let Some(func) = node.child_by_field_name("function") {
                if let Ok(txt) = func.utf8_text(block.as_bytes()) {
                    if let Some(last) = txt.split('.').last() {
                        // Skip test assertions and common test utilities
                        if !skip_methods.contains(&last) && !last.starts_with("assert") {
                            candidates.push(last.to_string());
                        }
                    }
                }
            }
        }
        let mut c = node.walk();
        for child in node.children(&mut c) {
            stack.push(child);
        }
    }
    
    // Return the first non-assertion function found
    candidates.into_iter().next()
}

fn search_for_function(repo_root: &Path, fn_name: &str) -> Option<PathBuf> {
    let fn_regex = Regex::new(&format!(r"(?m)^\s*def\s+{}\s*\(", regex::escape(fn_name))).ok()?;
    let class_method_regex = Regex::new(&format!(r"(?m)^\s+def\s+{}\s*\(", regex::escape(fn_name))).ok()?;

    for entry in WalkDir::new(repo_root).min_depth(1).into_iter().filter_map(Result::ok) {
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                match name {
                    ".git" | ".venv" | "venv" | "env" | "__pycache__" | "tests" | "test" => continue,
                    _ => {}
                }
            }
            continue;
        }

        if path.extension().and_then(|s| s.to_str()) != Some("py") {
            continue;
        }

        let file = File::open(path).ok()?;
        let reader = BufReader::new(file);

        for line in reader.lines().flatten() {
            if fn_regex.is_match(&line) || class_method_regex.is_match(&line) {
                return Some(path.to_path_buf());
            }
        }
    }
    None
}

fn extract_impl_function(path: &Path, fn_name: &str) -> Option<String> {
    let src = fs::read_to_string(path).ok()?;
    let tree = parse_source(path.to_string_lossy().as_ref(), &src)?;
    let root = tree.root_node();

    let mut c = root.walk();
    for node in root.children(&mut c) {
        if node.kind() == "function_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                if name_node.utf8_text(src.as_bytes()).ok()? == fn_name {
                    return node.utf8_text(src.as_bytes()).ok().map(str::to_string);
                }
            }
        }

        if node.kind() == "class_definition" {
            if let Some(body) = node.child_by_field_name("body") {
                let mut c2 = body.walk();
                for ch in body.children(&mut c2) {
                    if ch.kind() == "function_definition" {
                        if let Some(name_node) = ch.child_by_field_name("name") {
                            if name_node.utf8_text(src.as_bytes()).ok()? == fn_name {
                                return ch.utf8_text(src.as_bytes()).ok().map(str::to_string);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn build_full_suite_context(
    repo_root: &Path,
    failure: &FailureInfo,
) -> Option<FullSuiteContext> {

    let test_path = repo_root.join(&failure.test_file);
    let test_source = fs::read_to_string(&test_path).ok()?;

    let parts: Vec<_> = failure.test_name.split("::").collect();
    let method = parts.last()?.to_string();

    let block = extract_test_body(&test_source, &method)?;

    let function_called = find_called_function(&block).unwrap_or_else(|| {
        method.clone()
    });

    let impl_path = match search_for_function(repo_root, &function_called) {
        Some(p) => p,
        None => {
            return Some(FullSuiteContext {
                test_path,
                test_source,
                impl_path: PathBuf::new(),
                impl_source: "# Implementation not found — infer behavior from traceback".into(),
                function_name: function_called,
                traceback: failure.traceback.clone(),
            });
        }
    };

    let impl_source = extract_impl_function(&impl_path, &function_called).unwrap_or_else(|| {
        fs::read_to_string(&impl_path).unwrap_or_default()
    });

    Some(FullSuiteContext {
        test_path,
        test_source,
        impl_path,
        impl_source,
        function_name: function_called,
        traceback: failure.traceback.clone(),
    })
}