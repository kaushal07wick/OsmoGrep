use std::{error::Error, io, time::Duration};

use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use crate::{state::AgentState, ui::tui::draw_ui};

use super::frame::FrameClock;

pub struct TuiRuntime {
    frame: FrameClock,
    dirty: bool,
    input_rect: Rect,
    exec_rect: Rect,
}

impl Default for TuiRuntime {
    fn default() -> Self {
        Self {
            frame: FrameClock::default(),
            dirty: true,
            input_rect: Rect::default(),
            exec_rect: Rect::default(),
        }
    }
}

impl TuiRuntime {
    pub fn draw_if_due(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        state: &AgentState,
    ) -> Result<(), Box<dyn Error>> {
        let now = std::time::Instant::now();
        if !self.dirty || !self.frame.draw_due(now) {
            return Ok(());
        }

        let (input, _, exec) = draw_ui(terminal, state)?;
        self.input_rect = input;
        self.exec_rect = exec;
        self.frame.mark_drawn(std::time::Instant::now());
        self.dirty = false;
        Ok(())
    }

    pub fn poll_timeout(&self, live_activity: bool) -> Duration {
        self.frame
            .poll_timeout(std::time::Instant::now(), self.dirty, live_activity)
    }

    pub fn input_rect(&self) -> Rect {
        self.input_rect
    }

    pub fn exec_rect(&self) -> Rect {
        self.exec_rect
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }
}
