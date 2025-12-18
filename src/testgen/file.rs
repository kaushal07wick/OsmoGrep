use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::state::AgentState;
use crate::detectors::language::Language;
use crate::testgen::candidate::TestCandidate;
use crate::testgen::resolve::TestResolution;

/* ============================================================
   Public API
   ============================================================ */

pub fn materialize_test(
    state: &AgentState,
    candidate: &TestCandidate,
    resolution: &TestResolution,
    test_code: &str,
) -> io::Result<PathBuf> {
    match state.language.as_ref() {
        Some(Language::Python) => {
            write_python_test(candidate, resolution, test_code)
        }
        Some(Language::Rust) => {
            write_rust_test(candidate, resolution, test_code)
        }
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
                "Multiple possible Python test files found; user input required",
            ));
        }

        TestResolution::NotFound => default_python_test_path(candidate),
    };

    ensure_parent_dir(&path)?;
    append_or_create(&path, test_code)?;
    Ok(path)
}

fn default_python_test_path(c: &TestCandidate) -> PathBuf {
    let mut name = c.file.replace('/', "_");
    name = name.replace(".py", "");

    if let Some(sym) = &c.symbol {
        name.push('_');
        name.push_str(sym);
    }

    PathBuf::from(format!("tests/test_{}_regression.py", name))
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
        // NOTE: For Rust, `file` must be the source `.rs` file containing code
        TestResolution::Found { file, .. } => {
            let path = PathBuf::from(file);
            append_inline_rust_test(&path, test_code)?;
            Ok(path)
        }

        TestResolution::Ambiguous(_) => {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "Multiple possible Rust test locations found; user input required",
            ));
        }

        TestResolution::NotFound => {
            let path = default_rust_test_path(candidate);
            ensure_parent_dir(&path)?;
            append_or_create(&path, test_code)?;
            Ok(path)
        }
    }
}

fn default_rust_test_path(c: &TestCandidate) -> PathBuf {
    let mut name = c.file.replace('/', "_");
    name = name.replace(".rs", "");

    if let Some(sym) = &c.symbol {
        name.push('_');
        name.push_str(sym);
    }

    PathBuf::from(format!("tests/{}_regression.rs", name))
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

fn append_or_create(path: &Path, content: &str) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "\n{}", content.trim_end())?;
    Ok(())
}

fn append_inline_rust_test(path: &Path, test_code: &str) -> io::Result<()> {
    let src = fs::read_to_string(path)?;

    if src.contains("#[cfg(test)]") {
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(file, "\n{}", test_code.trim_end())?;
    } else {
        let mut file = fs::OpenOptions::new().append(true).open(path)?;
        writeln!(
            file,
            "\n\n#[cfg(test)]\nmod tests {{\n{}\n}}\n",
            indent(test_code.trim_end(), 4)
        )?;
    }

    Ok(())
}

fn indent(s: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    s.lines()
        .map(|l| format!("{pad}{l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
