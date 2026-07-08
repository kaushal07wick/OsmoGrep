use std::{error::Error, io};

use crossterm::{
    cursor::Show,
    event::{DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub struct TerminalSession {
    active: bool,
    mouse_capture: bool,
}

impl TerminalSession {
    fn restore_with<W: io::Write>(&mut self, mut writer: W) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }

        let raw_result = disable_raw_mode();
        let screen_result = if self.mouse_capture {
            execute!(
                writer,
                DisableBracketedPaste,
                DisableMouseCapture,
                LeaveAlternateScreen,
                Show
            )
        } else {
            execute!(writer, DisableBracketedPaste, LeaveAlternateScreen, Show)
        };
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

pub fn setup_terminal() -> Result<TerminalSession, Box<dyn Error>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    if let Err(err) = execute!(stdout, EnterAlternateScreen, EnableBracketedPaste) {
        let _ = disable_raw_mode();
        return Err(Box::new(err));
    }

    let mouse_capture = mouse_capture_enabled();
    if mouse_capture {
        if let Err(err) = execute!(stdout, EnableMouseCapture) {
            let _ = execute!(
                io::stdout(),
                DisableBracketedPaste,
                LeaveAlternateScreen,
                Show
            );
            let _ = disable_raw_mode();
            return Err(Box::new(err));
        }
    }

    Ok(TerminalSession {
        active: true,
        mouse_capture,
    })
}

pub fn teardown_terminal(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    terminal_session: &mut TerminalSession,
) -> Result<(), Box<dyn Error>> {
    terminal.show_cursor()?;
    terminal_session.restore_with(terminal.backend_mut())?;
    Ok(())
}

fn mouse_capture_enabled() -> bool {
    std::env::var("OSMOGREP_MOUSE")
        .ok()
        .is_some_and(|raw| parse_mouse_capture_setting(&raw))
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
    fn mouse_capture_setting_requires_explicit_enable() {
        assert!(!parse_mouse_capture_setting(""));
        assert!(!parse_mouse_capture_setting("off"));
        assert!(!parse_mouse_capture_setting("false"));
        assert!(parse_mouse_capture_setting("on"));
        assert!(parse_mouse_capture_setting("wheel"));
        assert!(parse_mouse_capture_setting("all"));
    }
}
