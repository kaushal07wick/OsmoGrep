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
            self.dirty = true;
        }
        state.ui.presentation_model = Some(model.clone());
        if (!self.dirty && !self.animating) || !self.frame.draw_due(now) {
            return Ok(());
        }
        let animation_interval = if state.ui.panel.as_ref().is_some_and(|p| p.section == 8) {
            32
        } else {
            70
        };
        if !self.dirty
            && self.animating
            && self.last_visual.is_some_and(|last| {
                now.duration_since(last) < Duration::from_millis(animation_interval)
            })
        {
            return Ok(());
        }
        if let Some(ui) = self.typescript.as_mut() {
            let size = terminal.size()?;
            match ui.draw(state, model, size.width, size.height) {
                Ok((input, exec, editor_width, animate)) => {
                    self.input_rect = input;
                    self.exec_rect = exec;
                    state.ui.input_wrap_width = editor_width;
                    self.animating = animate;
                    self.last_visual = Some(now);
                    self.frame.mark_drawn(now);
                    self.dirty = false;
                    return Ok(());
                }
                Err(error) => {
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

        let (input, _, exec) = draw_ui(terminal, state)?;
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
