//! Renders the add-podcast wizard with step progress.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};

use super::theme::*;
use super::widgets::*;
use crate::app::{App, WIZARD_STEPS, WizardStepKind};

pub(super) fn draw_add_podcast(frame: &mut Frame, app: &mut App) {
    let step = app.add_podcast.step;
    let visible_steps: Vec<_> = WIZARD_STEPS
        .iter()
        .enumerate()
        .filter(|(step, _)| app.add_podcast_step_is_visible(*step))
        .collect();
    let total = visible_steps.len();
    let position = visible_steps
        .iter()
        .position(|(visible_step, _)| *visible_step == step)
        .unwrap_or(0);

    let (hint_lines, status_height) = hint_bar(app, frame.area().width);

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(status_height),
    ])
    .split(frame.area());

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" AgentP ", Style::default().fg(TITLE_FG).bold()),
        Span::styled(" > ", Style::default().fg(ORANGE)),
        Span::styled("Add Podcast", Style::default().fg(PINK).bold()),
        Span::styled(
            format!("  ({}/{})", position + 1, total),
            Style::default().fg(TEXT_DIM),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let p = &app.add_podcast.podcast;

    let mut lines: Vec<Line> = Vec::new();

    for (si, ws) in visible_steps {
        let is_current = si == step;
        let is_done = si < step;
        let is_upcoming = si > step;

        let marker = if is_current {
            Span::styled("  ▸ ", Style::default().fg(ACCENT).bold())
        } else if is_done {
            Span::styled("  ✓ ", Style::default().fg(CHECK_ON))
        } else {
            Span::styled("    ", Style::default().fg(TEXT_DIM))
        };

        let label_color = if is_current {
            TEXT
        } else if is_done {
            ACCENT_DIM
        } else {
            TEXT_DIM
        };
        let label = Span::styled(
            format!("{:<26}", ws.label),
            Style::default().fg(label_color).bold(),
        );

        let mut spans = vec![marker, label];

        if is_current {
            match ws.kind {
                WizardStepKind::Text => {
                    spans.push(Span::styled(
                        format!("{}_", app.add_podcast.text_buffer),
                        Style::default().fg(HIGHLIGHT),
                    ));
                }
                WizardStepKind::Bool => {
                    let val = p.field_bool(si);
                    let (txt, color) = if val {
                        ("  yes", CHECK_ON)
                    } else {
                        ("  no ", TEXT_DIM)
                    };
                    spans.push(Span::styled(
                        txt.to_string(),
                        Style::default().fg(color).bold(),
                    ));
                }
                WizardStepKind::Usize => {
                    spans.push(Span::styled(
                        format!("< {} >", p.leading_zeros_amount),
                        Style::default().fg(HIGHLIGHT).bold(),
                    ));
                }
            }
        } else if is_done {
            match ws.kind {
                WizardStepKind::Text => {
                    let val = p.field_text(si);
                    let display = if val.is_empty() { "-".to_string() } else { val };
                    spans.push(Span::styled(display, Style::default().fg(ACCENT_DIM)));
                }
                WizardStepKind::Bool => {
                    let val = p.field_bool(si);
                    let (txt, color) = if val {
                        ("yes", ACCENT_DIM)
                    } else {
                        ("no", TEXT_DIM)
                    };
                    spans.push(Span::styled(txt.to_string(), Style::default().fg(color)));
                }
                WizardStepKind::Usize => {
                    spans.push(Span::styled(
                        p.leading_zeros_amount.to_string(),
                        Style::default().fg(ACCENT_DIM),
                    ));
                }
            }
        } else if is_upcoming {
            match ws.kind {
                WizardStepKind::Text => {
                    let val = p.field_text(si);
                    if !val.is_empty() {
                        spans.push(Span::styled(
                            format!("({})", val),
                            Style::default().fg(BORDER),
                        ));
                    }
                }
                WizardStepKind::Bool => {
                    let val = p.field_bool(si);
                    let txt = if val { "(yes)" } else { "(no)" };
                    spans.push(Span::styled(txt.to_string(), Style::default().fg(BORDER)));
                }
                WizardStepKind::Usize => {
                    spans.push(Span::styled(
                        format!("({})", p.leading_zeros_amount),
                        Style::default().fg(BORDER),
                    ));
                }
            }
        }

        lines.push(Line::from(spans));

        if is_current {
            let hint_prefix = if ws.required {
                "required — "
            } else {
                "optional — "
            };
            let indent = 7usize;
            let avail = (chunks[1].width as usize).saturating_sub(indent + 2);
            let chunks_text = wrap_words(ws.hint, avail.saturating_sub(hint_prefix.len()));
            let mut first = true;
            for chunk in chunks_text {
                let mut spans = vec![Span::raw(" ".repeat(indent))];
                if first {
                    spans.push(Span::styled(
                        hint_prefix,
                        Style::default()
                            .fg(if ws.required { ORANGE } else { TEXT_DIM })
                            .italic(),
                    ));
                    first = false;
                } else {
                    spans.push(Span::raw(" ".repeat(hint_prefix.len())));
                }
                spans.push(Span::styled(chunk, Style::default().fg(GAUGE_FG).italic()));
                lines.push(Line::from(spans));
            }
        }
    }

    if app.add_podcast.loading_feed_info {
        lines.push(Line::from(vec![]));
        lines.push(Line::from(vec![
            Span::raw("       "),
            Span::styled(
                "fetching info from feed...",
                Style::default().fg(TEXT_DIM).italic(),
            ),
        ]));
    }

    let body = Paragraph::new(lines).block(styled_block("New Podcast"));
    frame.render_widget(body, chunks[1]);

    if app.add_podcast.mode_select {
        render_mode_select_overlay(frame);
    }

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}

fn render_mode_select_overlay(frame: &mut Frame) {
    let area = frame.area();
    let width = (area.width * 60 / 100).max(44).min(area.width);
    let height = 7u16;
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    let dialog_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, dialog_area);

    let lines = vec![
        Line::from(Span::styled(
            "How would you like to fill in the details?",
            Style::default().fg(TEXT),
        )),
        Line::from(vec![]),
        Line::from(vec![
            Span::styled("  1  ", Style::default().fg(ORANGE).bold()),
            Span::styled("Manual", Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  2  ", Style::default().fg(ORANGE).bold()),
            Span::styled("Prepopulate from feed", Style::default().fg(TEXT)),
            Span::styled("  (experimental)", Style::default().fg(TEXT_DIM).italic()),
        ]),
    ];

    let dialog = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(ORANGE))
            .style(Style::default().bg(BG))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(dialog, dialog_area);
}
