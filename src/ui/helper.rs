use std::{process::Command, time::Instant};

pub fn running_pulse(start: Option<Instant>) -> Option<String> {
    let start = start?;
    let t = (start.elapsed().as_millis() / 90) as usize;
    let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    Some(format!(
        "{} {:>4.1}s",
        frames[t % frames.len()],
        start.elapsed().as_secs_f32()
    ))
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
    fn calculate_input_lines_counts_multibyte_chars_not_bytes() {
        let masked = "•".repeat(26);

        assert_eq!(calculate_input_lines(&masked, 80, 3), 1);
        assert_eq!(calculate_input_lines(&masked, 28, 3), 2);
    }
}
