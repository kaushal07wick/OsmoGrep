//! diff_analyzer.rs — EXACT per-function diffs (Python only)

use crate::git;
use crate::state::{ChangeSurface, DiffAnalysis, SymbolDelta};

pub fn analyze_diff() -> Vec<DiffAnalysis> {
    let raw = git::diff_cached();
    if raw.is_empty() {
        return Vec::new();
    }

    let diff = String::from_utf8_lossy(&raw);
    let mut results = Vec::new();

    for file in split_diff_by_file(&diff) {
        if !file.path.ends_with(".py") {
            continue;
        }
        results.extend(extract_function_diffs(&file));
    }

    results
}

struct FileDiff {
    path: String,
    hunks: Vec<String>,
}

fn split_diff_by_file(diff: &str) -> Vec<FileDiff> {
    let mut out = Vec::new();
    let mut current: Option<FileDiff> = None;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if let Some(f) = current.take() {
                out.push(f);
            }
            let path = line.split_whitespace()
                .nth(2)
                .unwrap_or("a/unknown")
                .trim_start_matches("a/")
                .to_string();

            current = Some(FileDiff {
                path,
                hunks: Vec::new(),
            });
            continue;
        }

        if let Some(ref mut f) = current {
            if line.starts_with("@@") {
                f.hunks.push(format!("{}\n", line));
            } else if let Some(last) = f.hunks.last_mut() {
                last.push_str(line);
                last.push('\n');
            }
        }
    }

    if let Some(f) = current {
        out.push(f);
    }
    out
}

fn extract_function_diffs(file: &FileDiff) -> Vec<DiffAnalysis> {
    let old_src = git::show_head(&file.path).unwrap_or_default();
    let new_src = git::show_index(&file.path).unwrap_or_default();

    let old_lines: Vec<&str> = old_src.lines().collect();
    let new_lines: Vec<&str> = new_src.lines().collect();

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for hunk in &file.hunks {
        let funcs = detect_functions_for_hunk(&new_lines, hunk);

        for func in funcs {
            if seen.contains(&func) {
                continue;
            }
            seen.insert(func.clone());

            let old_block = match extract_full_function(&old_lines, &func) {
                Some(b) => b,
                None => continue,
            };
            
            let new_block = match extract_full_function(&new_lines, &func) {
                Some(b) => b,
                None => continue,
            };

            if old_block.trim() == new_block.trim() {
                continue;
            }

            out.push(DiffAnalysis {
                file: file.path.clone(),
                symbol: Some(func.clone()),
                surface: detect_surface(&file.path, &new_block),
                delta: Some(SymbolDelta {
                    old_source: old_block,
                    new_source: new_block,
                }),
                summary: None,
            });
        }
    }

    out
}

fn detect_functions_for_hunk(lines: &[&str], hunk: &str) -> Vec<String> {
    let changed_lines = extract_changed_new_lines(hunk);
    let mut out = Vec::new();

    for idx in changed_lines {
        if let Some(func) = nearest_def(lines, idx) {
            if !out.contains(&func) {
                out.push(func);
            }
        }
    }
    out
}

fn extract_changed_new_lines(hunk: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut new_line = None;

    for line in hunk.lines() {
        if line.starts_with("@@") {
            let plus = line.split('+').nth(1).unwrap_or("");
            let start = plus.split(',').next().unwrap_or("0");
            new_line = start.trim().parse::<usize>().ok();
            continue;
        }
        if line.starts_with('+') && !line.starts_with("+++") {
            if let Some(n) = new_line {
                out.push(n.saturating_sub(1));
            }
        }
        if !line.starts_with('-') && new_line.is_some() {
            new_line = new_line.map(|n| n + 1);
        }
    }
    out
}

fn nearest_def(lines: &[&str], mut idx: usize) -> Option<String> {
    if idx >= lines.len() {
        idx = lines.len().saturating_sub(1);
    }
    while idx > 0 {
        let l = lines[idx].trim();
        if l.starts_with("@") {
            idx = idx.saturating_sub(1);
            continue;
        }
        if l.starts_with("def ") {
            return Some(l[4..].split('(').next()?.trim().to_string());
        }
        idx = idx.saturating_sub(1);
    }
    None
}

fn extract_full_function(lines: &[&str], func: &str) -> Option<String> {
    let mut start = None;

    for i in 0..lines.len() {
        let line = lines[i].trim();

        if line.starts_with("@") {
            let mut j = i;
            while j < lines.len() && lines[j].trim().starts_with("@") {
                j += 1;
            }
            if j < lines.len() && lines[j].trim().starts_with(&format!("def {}", func)) {
                start = Some(i);
                break;
            }
        } else if line.starts_with(&format!("def {}", func)) {
            start = Some(i);
            break;
        }
    }

    let start = start?;
    
    let def_line = lines.iter().skip(start).position(|l| l.trim().starts_with("def "))?;
    let def_idx = start + def_line;
    let base_indent = lines[def_idx].chars().take_while(|c| c.is_whitespace()).count();

    let mut end = def_idx + 1;
    while end < lines.len() {
        let l = lines[end];
        if l.trim().is_empty() {
            end += 1;
            continue;
        }
        let indent = l.chars().take_while(|c| c.is_whitespace()).count();
        if indent <= base_indent {
            break;
        }
        end += 1;
    }

    Some(lines[start..end].join("\n"))
}

fn detect_surface(_file: &str, code: &str) -> ChangeSurface {
    if code.contains("if ") || code.contains("for ") || code.contains("while ") {
        ChangeSurface::Branching
    } else if code.contains("self.") {
        ChangeSurface::State
    } else {
        ChangeSurface::PureLogic
    }
}