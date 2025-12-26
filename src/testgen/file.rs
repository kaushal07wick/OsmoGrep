use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::TestResolution;

/* ============================================================
   Public entry
   ============================================================ */

pub fn materialize_test(
    language: Language,
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    match language {
        Language::Python => write_python_test(candidate, resolution, test_code),
        Language::Rust => write_rust_test(candidate, resolution, test_code),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Unsupported language for test generation",
        )),
    }
}

/* ============================================================
   Python
   ============================================================ */

fn write_python_test(
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    let path = match resolution {
        TestResolution::Found { file, .. } => PathBuf::from(file),

        TestResolution::Ambiguous(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Multiple possible Python test files found",
            ));
        }

        TestResolution::NotFound => {
            let root = find_or_create_test_root(&candidate.file)?;
            let name = sanitize(&candidate.file, &candidate.symbol);
            root.join(format!("test_{name}.py"))
        }
    };

    ensure_parent_dir(&path)?;
    append_if_missing(&path, test_code)?;
    Ok(path)
}

/* ============================================================
   Rust
   ============================================================ */

fn write_rust_test(
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    match resolution {
        TestResolution::Found { file, .. } => {
            let path = PathBuf::from(file);
            append_rust_module(&path, test_code)?;
            Ok(path)
        }

        TestResolution::Ambiguous(_) => Err(io::Error::new(
            io::ErrorKind::Other,
            "Multiple possible Rust test locations found",
        )),

        TestResolution::NotFound => {
            let root = find_or_create_test_root(&candidate.file)?;
            let name = sanitize(&candidate.file, &candidate.symbol);
            let path = root.join(format!("{name}_test.rs"));
            ensure_parent_dir(&path)?;
            append_if_missing(&path, test_code)?;
            Ok(path)
        }
    }
}

/* ============================================================
   Test root resolution
   ============================================================ */

fn find_or_create_test_root(src_file: &str) -> io::Result<PathBuf> {
    let mut cur = Path::new(src_file).parent();

    while let Some(dir) = cur {
        let candidate = dir.join("tests");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        cur = dir.parent();
    }

    // fallback: repo_root/tests
    let root = std::env::current_dir()?.join("tests");
    fs::create_dir_all(&root)?;
    Ok(root)
}

/* ============================================================
   Helpers
   ============================================================ */

fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn append_if_missing(path: &Path, content: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();

    if existing.contains(content.trim()) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "\n{}", content.trim())?;
    Ok(())
}

fn append_rust_module(path: &Path, code: &str) -> io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();

    if existing.contains(code.trim()) {
        return Ok(());
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(
        file,
        "\n\n#[cfg(test)]\nmod osmogrep_tests {{\n{}\n}}",
        indent(code, 4)
    )?;

    Ok(())
}

fn indent(s: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    s.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn sanitize(file: &str, symbol: &Option<String>) -> String {
    let mut name = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test")
        .to_string();

    if let Some(sym) = symbol {
        name.push('_');
        name.push_str(sym);
    }

    name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")
}
