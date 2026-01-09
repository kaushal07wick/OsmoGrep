use std::io;

use ratatui::{
    backend::Backend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Modifier},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Terminal,
};

use crate::state::{AgentState, Focus, InputMode};
use crate::ui::{diff, execution, panels, status};


pub const PRIMARY_GREEN: Color  = Color::Rgb(0, 220, 140);
pub const PRIMARY_WHITE: Color  = Color::Rgb(235, 235, 235);


/* ==========================================================
   ASCII HEADER (NO GENERIC)
========================================================== */
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
        HEADER.iter().map(|l| {
            Line::from(Span::styled(
                *l,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ))
        }).collect::<Vec<_>>()
    )
    .alignment(Alignment::Center);

    f.render_widget(header, area);
}


/* ==========================================================
   COMMAND INPUT LINE (NO FAKE CURSOR)
========================================================== */
fn render_command_input_line(state: &AgentState, prompt: &str) -> Line<'static> {
    let mut spans = vec![
        Span::styled(prompt.to_string(), Style::default().fg(PRIMARY_WHITE)),
    ];

    if matches!(state.ui.input_mode, InputMode::ApiKey { .. }) {
        let len = state.ui.input.len();
        if len > 0 {
            let frame = (state.ui.last_activity.elapsed().as_millis() / 120) as usize;
            let anim = ["•", "••", "•••", "••••"][frame % 4];
            let obscured = if len == 1 {
                anim.to_string()
            } else {
                format!("{}{}", "•".repeat(len - 1), anim)
            };
            spans.push(Span::styled(obscured, Style::default().fg(PRIMARY_WHITE)));
        }
    } else if !state.ui.input.is_empty() {
        spans.push(Span::styled(state.ui.input.clone(), Style::default().fg(PRIMARY_WHITE)));
    }

    Line::from(spans)
}


/* ==========================================================
   REAL TERMINAL CURSOR POSITION
========================================================== */
fn cursor_position(input_rect: Rect, state: &AgentState, prompt_len: usize) -> (u16, u16) {
    let visible = state.ui.input.chars().count() as u16;
    (input_rect.x + prompt_len as u16 + visible, input_rect.y)
}


/* ==========================================================
   UNIFIED COMMAND BOX (LANDING + AGENT MODE)
========================================================== */
fn render_unified_command_box(
    f: &mut ratatui::Frame,
    area: Rect,
    state: &AgentState,
    prompt: &str,
    output_input_rect: &mut Rect,
) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top pad
            Constraint::Length(1), // input
            Constraint::Length(1), // spacer
            Constraint::Length(1), // model
            Constraint::Length(1), // bottom pad
        ])
        .split(area);

    let top_pad    = rows[0];
    let input_row  = rows[1];
    let spacer_row = rows[2];
    let model_row  = rows[3];
    let bottom_pad = rows[4];

    // FULL GRAY BACKGROUND
    f.render_widget(
        Block::default().style(Style::default().bg(Color::Rgb(30,30,30))),
        area,
    );

    // GREEN BORDER
    let border = Rect {
        x: area.x,
        y: area.y,
        width: 1,
        height: area.height,
    };
    f.render_widget(
        Block::default().style(Style::default().bg(PRIMARY_GREEN)),
        border,
    );

    // TOP PAD
    f.render_widget(Paragraph::new(" "), top_pad);

    // INPUT ROW
    let input_area = Rect {
        x: input_row.x + 2,
        y: input_row.y,
        width: input_row.width.saturating_sub(2),
        height: 1,
    };

    f.render_widget(
        Paragraph::new(render_command_input_line(state, prompt)),
        input_area,
    );

    *output_input_rect = input_area;

    if state.ui.focus == Focus::Input {
        let (cx, cy) = cursor_position(input_area, state, prompt.len());
        f.set_cursor(cx, cy);
    }

    // SPACER
    f.render_widget(Paragraph::new(" "), spacer_row);

    // MODEL ROW / HINT ROW
    let model_area = Rect {
        x: model_row.x + 2,
        y: model_row.y,
        width: model_row.width.saturating_sub(2),
        height: 1,
    };

    // LANDING MODE → SHOW MODEL  
    if !state.ui.first_command_done {
        let model_name = match &state.llm_backend {
            crate::llm::backend::LlmBackend::Remote { client } =>
                format!("{:?}", client.current_config().provider),
            crate::llm::backend::LlmBackend::Ollama { .. } =>
                "Ollama".to_string(),
        };

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(model_name, Style::default()
                    .fg(PRIMARY_WHITE)
                    .add_modifier(Modifier::BOLD)
                ),
            ])),
            model_area,
        );
    } else {
        // AGENT MODE → SHOW HINTS
        f.render_widget(
            Paragraph::new(
                Line::from(vec![
                    Span::styled("[enter]", Style::default().fg(PRIMARY_GREEN)),
                    Span::styled(" run   ", Style::default().fg(Color::Rgb(150,150,150))),
                    Span::styled("[esc]", Style::default().fg(PRIMARY_GREEN)),
                    Span::styled(" interrupt", Style::default().fg(Color::Rgb(150,150,150))),
                ])
            ),
            model_area,
        );
    }

    // BOTTOM PAD
    f.render_widget(Paragraph::new(" "), bottom_pad);
}


/* ==========================================================
   MAIN DRAW FUNCTION
========================================================== */
pub fn draw_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    state: &AgentState,
) -> io::Result<(Rect, Rect, Rect)>
{
    let mut input_rect = Rect::default();
    let mut diff_rect  = Rect::default();
    let mut exec_rect  = Rect::default();

    terminal.draw(|f| {
        let area = f.size();

        /* ---------------------------------------------
           LANDING SCREEN
        --------------------------------------------- */
        if !state.ui.first_command_done {
            let landing = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Length(6),
                    Constraint::Length(1),
                    Constraint::Length(5),
                    Constraint::Length(2),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .split(area);

            render_header(f, landing[1]);

            let box_area = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(15),
                    Constraint::Percentage(70),
                    Constraint::Percentage(15),
                ])
                .split(landing[3])[1];

            render_unified_command_box(f, box_area, state, ">_ ", &mut input_rect);

            // HINTS
            {
                let hints_area = landing[4];
                let center = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(15),
                        Constraint::Percentage(70),
                        Constraint::Percentage(15),
                    ])
                    .split(hints_area)[1];

                f.render_widget(
                    Paragraph::new(
                        Line::from(vec![
                            Span::styled("[enter]", Style::default().fg(PRIMARY_GREEN)),
                            Span::styled(" run", Style::default().fg(Color::Rgb(120,120,120))),
                            Span::raw("      "),
                            Span::styled("[esc]", Style::default().fg(PRIMARY_GREEN)),
                            Span::styled(" interrupt", Style::default().fg(Color::Rgb(120,120,120))),
                        ])
                    ).alignment(Alignment::Center),
                    center
                );
            }

            // FOOTER
            {
                let footer_cols = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(landing[6]);

                let repo   = state.repo_root.file_name().and_then(|s| s.to_str()).unwrap_or("repo");
                let branch = state.lifecycle.current_branch.as_deref().unwrap_or("main");

                f.render_widget(
                    Paragraph::new(
                        Line::from(vec![
                            Span::styled(format!("~/{}", repo), Style::default().fg(Color::Rgb(90,90,90))),
                            Span::raw(":"),
                            Span::styled(branch, Style::default().fg(Color::Rgb(90,90,90))),
                        ])
                    ),
                    footer_cols[0]
                );

                f.render_widget(
                    Paragraph::new(
                        Line::from(vec![
                            Span::styled(format!("v{}", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::Rgb(90,90,90))),
                        ])
                    ).alignment(Alignment::Right),
                    footer_cols[1]
                );
            }

            return;
        }

        /* ---------------------------------------------
        AGENT MODE
        --------------------------------------------- */

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(1), // top status bar
                Constraint::Min(1),    // main content
            ])
            .split(area);

        let main_horizontal = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(1),      // execution or diff
                Constraint::Length(30),  // right sidebar
            ])
            .split(layout[1]);

        let left_side = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),   // execution
                Constraint::Length(5) // command box area
            ])
            .split(main_horizontal[0]);

        let main_area = left_side[0];

        let mut rendered = false;

        /* ---------------------------------------------
        DIFF VIEW (SAFE ACCESS)
        --------------------------------------------- */
        if let Some(idx) = state.ui.selected_diff {
            if idx < state.context.diff_analysis.len() {
                if let Some(delta) = &state.context.diff_analysis[idx].delta {
                    diff::render_side_by_side(f, main_area, delta, state);
                    diff_rect = main_area;
                    rendered = true;
                }
            }
        }

        /* ---------------------------------------------
        EXECUTION PANEL + SPACING FIX
        --------------------------------------------- */
        if !rendered {
            if !panels::render_panel(f, main_area, state) {

                // Add safe margin around execution similar to sidebar
                let exec_padded = Rect {
                    x: main_area.x + 1,                     // left padding
                    y: main_area.y + 1,                     // top padding
                    width: main_area.width.saturating_sub(2), // right padding
                    height: main_area.height.saturating_sub(2), // bottom padding
                };

                execution::render_execution(f, exec_padded, state);
                exec_rect = exec_padded;
            } else {
                exec_rect = main_area;
            }
        } else {
            exec_rect = main_area;
        }

        /* ---------------------------------------------
        RIGHT SIDEBAR
        --------------------------------------------- */
        status::render_side_status(f, main_horizontal[1], state);

        /* ---------------------------------------------
        COMMAND BOX (UNIFIED)
        --------------------------------------------- */
        render_unified_command_box(f, left_side[1], state, ">_ ", &mut input_rect);



    })?;

    Ok((input_rect, diff_rect, exec_rect))
}
