use similar::{ChangeTag, TextDiff};

pub fn pretty_diff(
    before: Option<&str>,
    after: Option<&str>,
) -> String {
    match (before, after) {
        (Some(b), Some(a)) => {
            let diff = TextDiff::from_lines(b, a);
            let mut out = String::new();

            for change in diff.iter_all_changes() {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                out.push_str(sign);
                out.push_str(change.value());
            }

            out
        }

        (None, Some(a)) => format!("+{}", a),
        (Some(b), None) => format!("-{}", b),
        (None, None) => String::new(),
    }
}
