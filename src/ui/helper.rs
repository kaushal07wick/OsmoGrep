use std::{process::Command, time::Instant};

pub fn running_pulse(start: Option<Instant>) -> Option<String> {
    let start = start?;
    let t = (start.elapsed().as_millis() / 90) as usize;
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    Some(format!(
        "{} {}",
        frames[t % frames.len()],
        elapsed_text(
            start.elapsed().as_millis().min(u64::MAX as u128) as u64,
            false
        )
    ))
}
pub fn elapsed_text(milliseconds: u64, precise: bool) -> String {
    let rounded = if precise {
        milliseconds.saturating_add(50) / 100 * 100
    } else {
        milliseconds / 1000 * 1000
    };
    let seconds = rounded / 1000;
    if seconds >= 3600 {
        format!(
            "{}h {}m {}s",
            seconds / 3600,
            seconds % 3600 / 60,
            seconds % 60
        )
    } else if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else if precise {
        format!("{:.1}s", rounded as f64 / 1000.0)
    } else {
        format!("{seconds}s")
    }
}

pub fn git_branch(repo_root: &std::path::Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(repo_root)
        .output()
        .ok()?;
    String::from_utf8(out.stdout)
        .ok()
        .map(|s| s.trim().to_string())
}

/// Calculate how many lines the input will take when rendered
pub fn calculate_input_lines(input: &str, width: usize, prompt_len: usize) -> usize {
    if input.is_empty() {
        return 1;
    }

    let text_width = width.saturating_sub(prompt_len).max(1);
    let mut line_count = 0;

    for line in input.lines() {
        if line.is_empty() {
            line_count += 1;
        } else {
            let chars = line.chars().count();
            line_count += (chars + text_width - 1) / text_width;
        }
    }

    line_count.max(1)
}

#[cfg(test)]
mod tests {
    use super::calculate_input_lines;
    #[test]
    fn elapsed_time_carries_seconds_into_minutes_and_hours() {
        for (milliseconds, expected) in [
            (3300, "3.3s"),
            (59900, "59.9s"),
            (59999, "1m 0s"),
            (60000, "1m 0s"),
            (92100, "1m 32s"),
            (3599999, "1h 0m 0s"),
            (3723400, "1h 2m 3s"),
        ] {
            assert_eq!(super::elapsed_text(milliseconds, true), expected);
        }
        assert_eq!(super::elapsed_text(59999, false), "59s");
    }

    #[test]
    fn calculate_input_lines_counts_multibyte_chars_not_bytes() {
        let masked = "•".repeat(26);

        assert_eq!(calculate_input_lines(&masked, 80, 3), 1);
        assert_eq!(calculate_input_lines(&masked, 28, 3), 2);
    }
}
