use std::{error::Error, io};

use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    event::{KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub(crate) fn kitty_terminal() -> bool {
    std::env::var_os("GHOSTTY_RESOURCES_DIR").is_some()
        || std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var_os("WEZTERM_PANE").is_some()
        || matches!(
            std::env::var("TERM_PROGRAM").as_deref(),
            Ok("ghostty" | "kitty" | "wezterm")
        )
}

pub(super) fn tmux_graphics_enabled() -> bool {
    if std::env::var_os("TMUX").is_none() {
        return false;
    }
    if std::env::var("OSMOGREP_TMUX_GRAPHICS").as_deref() == Ok("1") {
        return true;
    }
    if !kitty_terminal() {
        return false;
    }
    std::process::Command::new("tmux")
        .args(["show-options", "-wv", "allow-passthrough"])
        .output()
        .is_ok_and(|out| {
            out.status.success()
                && matches!(String::from_utf8_lossy(&out.stdout).trim(), "on" | "all")
        })
}

pub(super) fn graphic_sequence(sequence: &str, tmux: bool) -> std::borrow::Cow<'_, str> {
    if tmux {
        format!("\x1bPtmux;{}\x1b\\", sequence.replace('\x1b', "\x1b\x1b")).into()
    } else {
        sequence.into()
    }
}

pub struct TerminalSession {
    active: bool,
}

impl TerminalSession {
    fn restore_with<W: io::Write>(&mut self, mut writer: W) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }

        let raw_result = disable_raw_mode();
        let screen_result = execute!(
            writer,
            DisableBracketedPaste,
            DisableMouseCapture,
            PopKeyboardEnhancementFlags,
            crossterm::style::Print("\x1b]112\x07\x1b[0 q"),
            LeaveAlternateScreen,
            Show
        );
        self.active = false;

        raw_result?;
        screen_result?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.restore_with(io::stdout());
    }
}

pub(super) fn cursor_appearance(theme: crate::state::UiTheme) -> &'static str {
    match theme {
        crate::state::UiTheme::Light => "\x1b]12;#53331c\x07\x1b[5 q",
        crate::state::UiTheme::Dark => "\x1b]12;#f0d6be\x07\x1b[5 q",
    }
}

pub fn setup_terminal() -> Result<TerminalSession, Box<dyn Error>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    if let Err(err) = execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
    ) {
        let _ = execute!(
            io::stdout(),
            PopKeyboardEnhancementFlags,
            DisableBracketedPaste,
            LeaveAlternateScreen,
            Show
        );
        let _ = disable_raw_mode();
        return Err(Box::new(err));
    }

    let mouse_capture = mouse_capture_enabled();
    if mouse_capture {
        if let Err(err) = execute!(stdout, EnableMouseCapture) {
            let _ = execute!(
                io::stdout(),
                DisableBracketedPaste,
                PopKeyboardEnhancementFlags,
                LeaveAlternateScreen,
                Show
            );
            let _ = disable_raw_mode();
            return Err(Box::new(err));
        }
    }

    Ok(TerminalSession { active: true })
}

pub fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    terminal_session: &mut TerminalSession,
) -> Result<(), Box<dyn Error>> {
    terminal.show_cursor()?;
    terminal_session.restore_with(terminal.backend_mut())?;
    Ok(())
}

pub(super) fn mouse_capture_enabled() -> bool {
    std::env::var("OSMOGREP_MOUSE")
        .ok()
        .is_some_and(|raw| parse_mouse_capture_setting(&raw))
}

pub(super) fn artwork_mouse_enabled() -> bool {
    std::env::var("OSMOGREP_MOUSE")
        .map(|raw| parse_mouse_capture_setting(&raw))
        .unwrap_or(true)
}

fn parse_mouse_capture_setting(raw: &str) -> bool {
    matches!(
        raw.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on" | "all" | "buttons" | "button" | "wheel" | "scroll"
    )
}

#[cfg(test)]
mod tests {
    use super::parse_mouse_capture_setting;

    #[test]
    fn tmux_graphics_escape_each_inner_escape_and_leave_direct_terminals_unchanged() {
        let image = "\x1b_Ga=T;data\x1b\\";
        assert_eq!(super::graphic_sequence(image, false), image);
        assert_eq!(
            super::graphic_sequence(image, true),
            "\x1bPtmux;\x1b\x1b_Ga=T;data\x1b\x1b\\\x1b\\"
        );
    }

    #[test]
    fn both_themes_set_explicit_contrasting_cursor_colors() {
        assert!(super::cursor_appearance(crate::state::UiTheme::Light).contains("#53331c"));
        assert!(super::cursor_appearance(crate::state::UiTheme::Dark).contains("#f0d6be"));
        assert!(super::cursor_appearance(crate::state::UiTheme::Light).ends_with("\x1b[5 q"));
    }

    #[test]
    fn mouse_capture_setting_requires_explicit_enable() {
        assert!(!parse_mouse_capture_setting(""));
        assert!(!parse_mouse_capture_setting("off"));
        assert!(!parse_mouse_capture_setting("false"));
        assert!(parse_mouse_capture_setting("on"));
        assert!(parse_mouse_capture_setting("wheel"));
        assert!(parse_mouse_capture_setting("all"));
    }
}
