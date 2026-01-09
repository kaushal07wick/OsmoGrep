use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Paragraph},
    Terminal,
};

use crate::state::{AgentState, Focus, InputMode};
use crate::ui::{diff, execution, panels, status};


pub const PRIMARY_GREEN: Color  = Color::Rgb(0, 220, 140);
pub const PRIMARY_WHITE: Color  = Color::Rgb(235, 235, 235);

/// ascii header
pub fn render_header(f: &mut ratatui::Frame, area: Rect) {
    const HEADER: [&str; 6] = [
        " █████╗  ██████╗███╗   ███╗ █████╗  ██████╗ ██████╗ ███████╗██████╗ ",
        "██╔══██╗██╔════╝████╗ ████║██╔══██╗██╔════╝ ██╔══██╗██╔════╝██╔══██╗",
        "██║  ██║╚█████╗ ██╔████╔██║██║  ██║██║  ██╗ ██████╔╝█████╗  ██████╔╝",
        "██║  ██║ ╚═══██╗██║╚██╔╝██║██║  ██║██║  ╚██╗██╔══██╗██╔══╝  ██╔═══╝ ",
        "╚█████╔╝██████╔╝██║ ╚═╝ ██║╚█████╔╝╚██████╔╝██║  ██║███████╗██║     ",
        " ╚════╝ ╚═════╝ ╚═╝     ╚═╝ ╚════╝  ╚═════╝ ╚═╝  ╚═╝╚══════╝╚═╝     ",
    ];

    let header = Paragraph::new(
        HEADER
            .iter()
            .map(|l| {
                Line::from(Span::styled(
                    *l,
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ))
            })
            .collect::<Vec<_>>(),
    )
    .alignment(Alignment::Center);

    f.render_widget(header, area);
}


//command input line
fn render_command_input_line(state: &AgentState, prompt: &str) -> Line<'static> {
    let mut spans = vec![
        Span::styled(prompt.to_string(), Style::default().fg(PRIMARY_WHITE)),
    ];

    let in_api = matches!(state.ui.input_mode, InputMode::ApiKey { .. });

    if in_api {
        let len = state.ui.input.len();
        if len == 0 {
            spans.push(Span::styled("█", Style::default().fg(PRIMARY_WHITE)));
        } else {
            let frame = (state.ui.last_activity.elapsed().as_millis() / 120) as usize;
            let anim = ["•","••","•••","••••"][frame % 4];
            let obscured = if len == 1 { anim.to_string() } else {
                format!("{}{}", "•".repeat(len - 1), anim)
            };
            spans.push(Span::styled(obscured, Style::default().fg(PRIMARY_WHITE)));
        }
    } else {
        if state.ui.input.is_empty() {
            spans.push(Span::styled("█", Style::default().fg(PRIMARY_WHITE)));
        } else {
            spans.push(Span::styled(state.ui.input.clone(), Style::default().fg(PRIMARY_WHITE)));
            spans.push(Span::styled("█", Style::default().fg(PRIMARY_WHITE)));
        }
    }

    Line::from(spans)
}


/// cursor position
fn cursor_position(input_rect: Rect, state: &AgentState, prompt_len: usize) -> (u16, u16) {
    let visible = state.ui.input.chars().count() as u16;
    let cx = input_rect.x + prompt_len as u16 + visible;
    let cy = input_rect.y;
    (cx, cy)
}


/// main draw function
pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)>
{
    let mut input_rect = Rect::default();
    let mut diff_rect = Rect::default();
    let mut exec_rect = Rect::default();

    terminal.draw(|f| {
        let area = f.size();

        // landing screen
        if !state.ui.first_command_done {

            let landing = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(30),  // top padding
                    Constraint::Length(6),       // header
                    Constraint::Length(1),       // space
                    Constraint::Length(5),       // command box (3 rows)
                    Constraint::Length(2),       // hints below
                    Constraint::Min(1),          // flexible space
                    Constraint::Length(1),       // footer
                ])
                .split(area);

            // header
            render_header(f, landing[1]);

            let box_horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(landing[3]);

            let box_area = box_horizontal[1];

            /* Create 5 rows inside the box */
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // top padding
                    Constraint::Length(1), // input row
                    Constraint::Length(1), // spacer row
                    Constraint::Length(1), // model row
                    Constraint::Length(1), // bottom padding
                ])
                .split(box_area);

            let top_pad    = rows[0];
            let input_row  = rows[1];
            let spacer_row = rows[2];
            let model_row  = rows[3];
            let bottom_pad = rows[4];

            /* 1. DRAW FULL BACKGROUND FIRST */
            f.render_widget(
                Block::default().style(Style::default().bg(Color::Rgb(30,30,30))),
                box_area,
            );

            /* 2. DRAW FULL GREEN ACCENT BORDER (Continuous Height) */
            let border_rect = Rect {
                x: box_area.x,
                y: box_area.y,
                width: 1,
                height: box_area.height,
            };

            f.render_widget(
                Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default().bg(PRIMARY_GREEN)),
                border_rect,
            );

            /* 3. RENDER CONTENT ON TOP (Remove .bg() from Paragraphs to prevent "breaking" the border) */

            // ROW 2: INPUT
            let input_area = Rect {
                x: input_row.x + 2,
                y: input_row.y,
                width: input_row.width.saturating_sub(2),
                height: 1,
            };

            let prompt = ">_ ";
            let mut spans = vec![Span::styled(prompt, Style::default().fg(PRIMARY_WHITE))];

            if !state.ui.input.is_empty() {
                spans.push(Span::styled(state.ui.input.clone(), Style::default().fg(PRIMARY_WHITE)));
            }

            f.render_widget(
                Paragraph::new(Line::from(spans)), // Background removed
                input_area,
            );

            input_rect = input_area;
            if state.ui.focus == Focus::Input {
                let (cx, cy) = cursor_position(input_rect, state, prompt.len());
                f.set_cursor(cx, cy);
            }

            // ROW 4: MODEL
            let model_area = Rect {
                x: model_row.x + 2,
                y: model_row.y,
                width: model_row.width.saturating_sub(2),
                height: 1,
            };

            let model_name = match &state.llm_backend {
                crate::llm::backend::LlmBackend::Remote { client } =>
                    format!("{:?}", client.current_config().provider),
                crate::llm::backend::LlmBackend::Ollama { .. } =>
                    "Ollama".to_string(),
            };

            f.render_widget(
                Paragraph::new(
                    Line::from(vec![
                        Span::styled(model_name, Style::default().fg(PRIMARY_WHITE).add_modifier(Modifier::BOLD)),
                    ])
                ), // Background removed
                model_area,
            );

            /* ---------------------------------------------
               HINTS BELOW COMMAND BOX
            --------------------------------------------- */

            let hints_area = landing[4];
            
            let hints_horizontal = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(hints_area);

            let hints = Paragraph::new(
                Line::from(vec![
                    Span::styled("[enter]", Style::default().fg(PRIMARY_GREEN)),
                    Span::styled(" run", Style::default().fg(Color::Rgb(100, 100, 100))),
                    Span::raw("      "),
                    Span::styled("[esc]", Style::default().fg(PRIMARY_GREEN)),
                    Span::styled(" interrupt", Style::default().fg(Color::Rgb(100, 100, 100))),
                ])
            )
            .alignment(Alignment::Center);

            f.render_widget(hints, hints_horizontal[1]);

            /* ---------------------------------------------
               FOOTER: PATH LEFT, VERSION RIGHT
            --------------------------------------------- */

            let repo = state.repo_root.file_name().and_then(|s| s.to_str()).unwrap_or("repo");
            let branch = state.lifecycle.current_branch.as_deref().unwrap_or("main");

            let footer_cols = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ])
                .split(landing[6]);

            f.render_widget(
                Paragraph::new(
                    Line::from(vec![
                        Span::styled(format!("~/{}", repo), Style::default().fg(Color::Rgb(80, 80, 80))),
                        Span::styled(":", Style::default().fg(Color::Rgb(80, 80, 80))),
                        Span::styled(branch, Style::default().fg(Color::Rgb(80, 80, 80))),
                    ])
                )
                .alignment(Alignment::Left),
                footer_cols[0],
            );

            f.render_widget(
                Paragraph::new(
                    Line::from(vec![
                        Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::Rgb(80, 80, 80))),
                    ])
                )
                .alignment(Alignment::Right),
                footer_cols[1],
            );

            diff_rect = Rect::new(0, 0, 0, 0);
            exec_rect = Rect::new(0, 0, 0, 0);
            return;
        }

        /* =====================================================
           AGENT MODE (AFTER FIRST COMMAND)
        ===================================================== */

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area);

        let main_horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(30),
            ])
            .split(layout[1]);

        let left_side = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(main_horizontal[0]);

        let main_area = left_side[0];

        let mut rendered = false;

        if panels::render_panel(f, main_area, state) {
            rendered = true;
        } else if let Some(idx) = state.ui.selected_diff {
            if let Some(delta) = &state.context.diff_analysis[idx].delta {
                diff::render_side_by_side(f, main_area, delta, state);
                diff_rect = main_area;
                rendered = true;
            }
        }

        if !rendered {
            execution::render_execution(f, main_area, state);
        }

        exec_rect = main_area;

        status::render_side_status(f, main_horizontal[1], state);

        /* ---------------- COMMAND BOX ---------------- */

        let prompt = ">_ ";
        let input_line = render_command_input_line(state, prompt);

        let cmd_box = Paragraph::new(input_line)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(PRIMARY_WHITE))
                    .title("COMMAND")
                    .title_alignment(Alignment::Center),
            );

        f.render_widget(cmd_box, left_side[1]);
        input_rect = left_side[1];

        if state.ui.focus == Focus::Input {
            let (cx, cy) = cursor_position(input_rect, state, prompt.len());
            f.set_cursor(cx, cy);
        }
    })?;

    Ok((input_rect, diff_rect, exec_rect))
}