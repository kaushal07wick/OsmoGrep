//! detectors/language.rs
//!
//! Heuristic language detection based on repository contents.

use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Language {
    Python,
    JavaScript,
    TypeScript,
    Rust,
    Go,
    Java,
    Unknown,
}

/* ============================================================
   Public API
   ============================================================ */

pub fn detect_language(root: &Path) -> Language {
    let mut py = 0usize;
    let mut js = 0usize;
    let mut ts = 0usize;
    let mut rs = 0usize;
    let mut go = 0usize;
    let mut java = 0usize;

    for entry in WalkDir::new(root)
        .max_depth(6) // enough signal, avoids full scan
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter(|e| !is_ignored(e.path()))
    {
        match entry.path().extension().and_then(|e| e.to_str()) {
            Some("rs") => rs += 1,
            Some("py") => py += 1,
            Some("ts") => ts += 1,
            Some("js") => js += 1,
            Some("go") => go += 1,
            Some("java") => java += 1,
            _ => {}
        }

        // Early exit for strong Rust or Python repos
        if rs >= 10 {
            return Language::Rust;
        }
        if py >= 10 {
            return Language::Python;
        }
    }

    dominant_language(py, js, ts, rs, go, java)
}

/* ============================================================
   Helpers
   ============================================================ */

fn dominant_language(
    py: usize,
    js: usize,
    ts: usize,
    rs: usize,
    go: usize,
    java: usize,
) -> Language {
    let mut best = (Language::Unknown, 0);

    for (lang, count) in [
        (Language::Python, py),
        (Language::TypeScript, ts),
        (Language::JavaScript, js),
        (Language::Rust, rs),
        (Language::Go, go),
        (Language::Java, java),
    ] {
        if count > best.1 {
            best = (lang, count);
        }
    }

    best.0
}

fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("target")
                | Some("node_modules")
                | Some(".git")
                | Some(".venv")
                | Some("dist")
                | Some("build")
        )
    })
}

use std::fmt;

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Language::Python => "python",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Rust => "rust",
            Language::Go => "go",
            Language::Java => "java",
            Language::Unknown => "unknown",
        };
        f.write_str(s)
    }
}
