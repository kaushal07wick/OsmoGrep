//! detectors/framework.rs
//!
//! Test framework detection based on repository structure.

use std::path::Path;

#[derive(Debug, Clone)]
pub enum TestFramework {
    Pytest,
    Unittest,
    CargoTest,
    Jest,
    GoTest,
    None,
}

/* ============================================================
   Public API
   ============================================================ */

pub fn detect_framework(root: &Path) -> TestFramework {
    // Fast path: Rust
    if exists(root, "Cargo.toml") {
        return TestFramework::CargoTest;
    }

    // Python (prefer pytest)
    if exists(root, "pytest.ini") || exists(root, "conftest.py") {
        return TestFramework::Pytest;
    }

    if exists(root, "pyproject.toml") || exists(root, "setup.py") {
        return TestFramework::Unittest;
    }

    // JavaScript / TypeScript
    if exists(root, "package.json") {
        if package_uses_jest(root) {
            return TestFramework::Jest;
        }
    }

    // Go
    if exists(root, "go.mod") {
        return TestFramework::GoTest;
    }

    TestFramework::None
}

/* ============================================================
   Helpers
   ============================================================ */

#[inline]
fn exists(root: &Path, file: &str) -> bool {
    root.join(file).exists()
}

fn package_uses_jest(root: &Path) -> bool {
    let pkg = root.join("package.json");
    let Ok(contents) = std::fs::read_to_string(pkg) else {
        return false;
    };

    // Cheap string scan; avoids JSON parsing cost
    contents.contains("jest") || contents.contains("vitest")
}
