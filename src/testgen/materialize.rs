use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;

/// Generate a new single-test file for a candidate change
pub fn materialize_test(
    repo_root: &Path,
    language: Language,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    match language {
        Language::Python => write_python_test(repo_root, candidate, test_code),
        Language::Rust => write_rust_test(repo_root, candidate, test_code),
        _ => Err(io::Error::new(
            io::ErrorKind::Other,
            "Unsupported language for test generation",
        )),
    }
}

/// Fix an existing failing test file (full-suite flow)
/// ALWAYS overwrites the original failing file.
pub fn materialize_full_suite_test(
    repo_root: &Path,
    test_path: &Path,
    new_code: &str,
) -> io::Result<PathBuf> {
    let abs = repo_root.join(test_path);

    // If test already exists, overwrite it
    if abs.exists() {
        write_file_atomic(&abs, new_code)?;
        return Ok(abs);
    }

    // If not, create under tests/
    let tests_root = find_test_root(repo_root)?;
    let new_path = tests_root.join(
        test_path
            .file_name()
            .unwrap_or_else(|| "generated_test.py".as_ref()),
    );

    write_file_atomic(&new_path, new_code)?;
    Ok(new_path)
}

/// Python test writer
fn write_python_test(
    repo_root: &Path,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    let root = find_test_root(repo_root)?;
    let name = sanitize_name(&candidate.file, &candidate.symbol);
    let path = root.join(format!("test_{name}.py"));

    write_file_atomic(&path, test_code)?;
    Ok(path)
}

/// Rust test writer
fn write_rust_test(
    repo_root: &Path,
    candidate: &TestCandidate,
    test_code: &str,
) -> io::Result<PathBuf> {
    let root = find_test_root(repo_root)?;
    let name = sanitize_name(&candidate.file, &candidate.symbol);
    let path = root.join(format!("{name}.rs"));

    write_file_atomic(&path, test_code)?;
    Ok(path)
}

/// Ensure we have tests root
fn find_test_root(repo_root: &Path) -> io::Result<PathBuf> {
    for name in ["tests", "test"] {
        let p = repo_root.join(name);
        if p.is_dir() {
            return Ok(p);
        }
    }

    // fall back to creating tests/
    let default = repo_root.join("tests");
    fs::create_dir_all(&default)?;
    Ok(default)
}

/// Atomic write with parent creation
fn write_file_atomic(path: &Path, content: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut f = fs::File::create(path)?;
    f.write_all(content.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

/// Convert arbitrary file + symbol into safe test file name
fn sanitize_name(file: &str, symbol: &Option<String>) -> String {
    let mut name = Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("test")
        .to_string();

    if let Some(sym) = symbol {
        name.push('_');
        name.push_str(sym);
    }

    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}
