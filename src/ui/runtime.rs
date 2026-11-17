use std::{error::Error, io, time::Duration};

use ratatui::{backend::CrosstermBackend, layout::Rect, Terminal};

use crate::{
    agent::ModelConfig,
    state::{AgentState, LogLevel},
    ui::tui::draw_ui,
};

use super::frame::FrameClock;

pub struct TuiRuntime {
    frame: FrameClock,
    dirty: bool,
    input_rect: Rect,
    exec_rect: Rect,
    typescript: Option<super::typescript::TypeScriptUi>,
    renderer_error: Option<String>,
    animating: bool,
    last_visual: Option<std::time::Instant>,
    last_size: Rect,
    cursor_theme: Option<crate::state::UiTheme>,
    revision: u64,
    request_revision: Option<u64>,
}

impl Default for TuiRuntime {
    fn default() -> Self {
        let (typescript, renderer_error) =
            if std::env::var("OSMOGREP_UI").as_deref() == Ok("native") {
                (None, None)
            } else {
                match super::typescript::TypeScriptUi::start() {
                    Ok(ui) => (Some(ui), None),
                    Err(error) => (None, Some(error)),
                }
            };
        Self {
            frame: FrameClock::default(),
            dirty: true,
            input_rect: Rect::default(),
            exec_rect: Rect::default(),
            typescript,
            renderer_error,
            animating: false,
            last_visual: None,
            last_size: Rect::default(),
            cursor_theme: None,
            revision: 0,
            request_revision: None,
        }
    }
}

impl TuiRuntime {
    pub fn draw_if_due(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        state: &mut AgentState,
        model: &ModelConfig,
    ) -> Result<(), Box<dyn Error>> {
        let now = std::time::Instant::now();
        let size = terminal.size()?;
        if size != self.last_size {
            self.last_size = size;
            self.mark_dirty();
        }
        state.ui.presentation_model = Some(model.clone());
        if (!self.dirty && !self.animating) || !self.frame.draw_due(now) {
            return Ok(());
        }
        let animation_interval = self
            .typescript
            .as_ref()
            .and_then(|ui| ui.animation_interval_ms)
            .unwrap_or(
                if !state.ui.agent_running
                    || state.ui.panel.as_ref().is_some_and(|p| p.section == 8)
                {
                    32
                } else {
                    70
                },
            );
        if !self.dirty
            && self.animating
            && self.last_visual.is_some_and(|last| {
                now.duration_since(last) < Duration::from_millis(animation_interval)
            })
        {
            return Ok(());
        }
        if self.cursor_theme != Some(state.theme) {
            use std::io::Write;
            io::stdout().write_all(super::terminal::cursor_appearance(state.theme).as_bytes())?;
            io::stdout().flush()?;
            self.cursor_theme = Some(state.theme);
        }
        if let Some(ui) = self.typescript.as_mut() {
            self.request_revision.get_or_insert(self.revision);
            let size = terminal.size()?;
            match ui.draw(state, model, size.width, size.height) {
                Ok(Some((input, exec, editor_width, animate))) => {
                    self.input_rect = input;
                    self.exec_rect = exec;
                    state.ui.input_wrap_width = editor_width;
                    self.animating = animate;
                    self.last_visual = Some(now);
                    self.frame.mark_drawn(now);
                    self.dirty = self.request_revision.take() != Some(self.revision);
                    return Ok(());
                }
                Ok(None) => {
                    self.frame.mark_drawn(now);
                    return Ok(());
                }
                Err(error) => {
                    self.request_revision = None;
                    self.typescript = None;
                    self.renderer_error = Some(error);
                    self.animating = false;
                    terminal.clear()?;
                }
            }
        }
        if let Some(error) = self.renderer_error.take() {
            state.logs.push(
                LogLevel::Warn,
                format!("Native UI restored: {error}. OSMOGREP_UI=native selects it explicitly."),
            );
        }

        use std::io::Write;
        if super::terminal::artwork_mouse_enabled() {
            io::stdout().write_all(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h")?;
        }
        state.ui.exec_scroll_max = usize::MAX;
        let (input, _, exec) = draw_ui(terminal, state)?;
        self.animating = !state.preferences.reduce_motion
            && state
                .ui
                .pending_update
                .as_ref()
                .is_some_and(|update| update.installing);
        self.input_rect = input;
        self.exec_rect = exec;
        state.ui.input_wrap_width = input.width.saturating_sub(3).max(1) as usize;
        self.frame.mark_drawn(std::time::Instant::now());
        self.dirty = false;
        Ok(())
    }

    pub fn poll_timeout(&self, live_activity: bool) -> Duration {
        self.frame.poll_timeout(
            std::time::Instant::now(),
            self.dirty,
            live_activity || self.animating,
        )
    }

    pub fn input_rect(&self) -> Rect {
        self.input_rect
    }

    pub fn exec_rect(&self) -> Rect {
        self.exec_rect
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn handle_visual_event(
        &mut self,
        event: &crossterm::event::Event,
        state: &mut AgentState,
    ) -> bool {
        self.typescript
            .as_mut()
            .is_some_and(|ui| ui.handle_visual_event(event, state))
    }
}
