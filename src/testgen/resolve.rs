use crate::state::AgentState;
use crate::testgen::candidate::TestCandidate;

/* ============================================================
   Resolution result
   ============================================================ */

#[derive(Debug, Clone)]
pub enum TestResolution {
    Found {
        file: String,
        test_fn: Option<String>, // Python may refine later
    },
    Ambiguous(Vec<String>),
    NotFound,
}

/* ============================================================
   Public entry
   ============================================================ */

pub fn resolve_test(
    state: &AgentState,
    c: &TestCandidate,
) -> TestResolution {
    match state.language.as_ref().map(|l| format!("{:?}", l)).as_deref() {
        Some("Rust") => resolve_rust_test(c),
        Some("Python") => resolve_python_test(c),
        _ => TestResolution::NotFound,
    }
}

/* ============================================================
   Rust resolution (tests live in same file)
   ============================================================ */

fn resolve_rust_test(c: &TestCandidate) -> TestResolution {
    let symbol = match &c.symbol {
        Some(s) => s,
        None => return TestResolution::NotFound,
    };

    let src = match std::fs::read_to_string(&c.file) {
        Ok(s) => s,
        Err(_) => return TestResolution::NotFound,
    };

    if !src.contains("#[cfg(test)]") {
        return TestResolution::NotFound;
    }

    // crude but safe: test must reference the symbol
    if src.contains(&format!("{}(", symbol)) {
        TestResolution::Found {
            file: c.file.clone(),
            test_fn: None, // inline tests
        }
    } else {
        TestResolution::NotFound
    }
}

/* ============================================================
   Python resolution (search tests/ recursively)
   ============================================================ */

fn resolve_python_test(c: &TestCandidate) -> TestResolution {
    let symbol = match &c.symbol {
        Some(s) => s,
        None => return TestResolution::NotFound,
    };

    let mut hits = Vec::new();

    // common pytest roots
    let roots = ["tests", "test", "testing"];

    for root in roots {
        if !std::path::Path::new(root).exists() {
            continue;
        }

        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
        {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("py") {
                continue;
            }

            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // pytest patterns:
            // test_xxx(), xxx_test(), test_xxx_yyy()
            if content.contains(&format!("{}(", symbol)) {
                hits.push(path.display().to_string());
            }
        }
    }

    match hits.len() {
        0 => TestResolution::NotFound,
        1 => TestResolution::Found {
            file: hits[0].clone(),
            test_fn: None, // refine later if needed
        },
        _ => TestResolution::Ambiguous(hits),
    }
}
