// src/detectors/framework.rs
use std::path::Path;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum TestFramework {
    Pytest,
    Unittest,
    CargoTest,
    Jest,
    GoTest,
    None,
}

pub fn detect_framework(root: &Path) -> TestFramework {
    let has = |p: &str| root.join(p).exists();

    /* ---------- Rust (authoritative) ---------- */
    // Cargo.toml alone implies `cargo test`
    if has("Cargo.toml") {
        return TestFramework::CargoTest;
    }

    /* ---------- Python ---------- */
    if has("pytest.ini") || has("conftest.py") {
        return TestFramework::Pytest;
    }

    // Fallback Python unittest detection
    if has("setup.py") || has("pyproject.toml") {
        // Heuristic: assume unittest if pytest not found
        return TestFramework::Unittest;
    }

    /* ---------- JavaScript / TypeScript ---------- */
    if has("package.json") {
        let pkg = root.join("package.json");
        if let Ok(c) = std::fs::read_to_string(pkg) {
            if c.contains("jest") || c.contains("vitest") {
                return TestFramework::Jest;
            }
        }
    }

    /* ---------- Go ---------- */
    if has("go.mod") {
        return TestFramework::GoTest;
    }

    TestFramework::None
}
