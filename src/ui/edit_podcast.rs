//! Renders the podcast editor and podcast selection screens.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, HighlightSpacing, List, ListItem, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};

use crate::app::{App, ConfigEditMode, PodcastEditorTarget, WIZARD_STEPS, WizardStepKind};
use crate::podcast::Podcast;

use super::theme::*;
use super::widgets::*;

pub(super) fn draw_edit_podcast_select(frame: &mut Frame, app: &mut App) {
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
        Span::styled("Config", Style::default().fg(PINK).bold()),
        Span::styled(" > ", Style::default().fg(ORANGE)),
        Span::styled("Edit Podcast", Style::default().fg(PINK).bold()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = app
        .config
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {:>2}  ", i + 1), Style::default().fg(ORANGE)),
                Span::styled(
                    if podcast.name.is_empty() {
                        "(unnamed)".to_string()
                    } else {
                        podcast.name.clone()
                    },
                    Style::default().fg(PINK).bold(),
                ),
            ]))
        })
        .collect();

    let item_count = items.len();
    let list = List::new(items)
        .block(styled_block("Select Podcast to Edit"))
        .highlight_style(
            Style::default()
                .bg(SELECTED_BG)
                .fg(HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("  > ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, chunks[1], &mut app.edit_podcast_select_state);

    let scrollbar =
        Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(BORDER));
    let mut scrollbar_state = ScrollbarState::new(item_count)
        .position(app.edit_podcast_select_state.selected().unwrap_or(0));
    frame.render_stateful_widget(
        scrollbar,
        chunks[1].inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut scrollbar_state,
    );

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}

fn podcast_field_lines<'a>(
    p: &Podcast,
    selected_fi: usize,
    in_edit: bool,
    text_buf: &str,
    first_field: usize,
    body_width: u16,
) -> Vec<Line<'a>> {
    let field_count = WIZARD_STEPS.len();
    let mut lines: Vec<Line> = Vec::new();

    for (si, ws) in WIZARD_STEPS
        .iter()
        .enumerate()
        .take(field_count)
        .skip(first_field)
    {
        let is_selected = si == selected_fi;

        let marker = if is_selected {
            Span::styled("  ▸ ", Style::default().fg(ACCENT).bold())
        } else {
            Span::styled("    ", Style::default().fg(TEXT_DIM))
        };

        let label_color = if is_selected { TEXT } else { TEXT_DIM };
        let label = Span::styled(
            format!("{:<26}", ws.label),
            Style::default().fg(label_color).bold(),
        );

        let mut spans = vec![marker, label];

        match ws.kind {
            WizardStepKind::Text => {
                if in_edit && is_selected {
                    spans.push(Span::styled(
                        format!("{}_", text_buf),
                        Style::default().fg(HIGHLIGHT),
                    ));
                } else {
                    let val = p.field_text(si);
                    let display = if val.is_empty() { "-".to_string() } else { val };
                    let color = if is_selected { PINK } else { TEXT_DIM };
                    spans.push(Span::styled(display, Style::default().fg(color)));
                }
            }
            WizardStepKind::Bool => {
                let val = p.field_bool(si);
                let (txt, color) = if val {
                    ("  yes", CHECK_ON)
                } else {
                    ("  no ", TEXT_DIM)
                };
                let style = if is_selected {
                    Style::default().fg(color).bold()
                } else {
                    Style::default().fg(color)
                };
                spans.push(Span::styled(txt.to_string(), style));
            }
            WizardStepKind::Usize => {
                if is_selected {
                    spans.push(Span::styled(
                        format!("< {} >", p.leading_zeros_amount),
                        Style::default().fg(HIGHLIGHT).bold(),
                    ));
                } else {
                    spans.push(Span::styled(
                        p.leading_zeros_amount.to_string(),
                        Style::default().fg(TEXT_DIM),
                    ));
                }
            }
        }

        lines.push(Line::from(spans));

        if is_selected {
            let indent = 7usize;
            let avail = (body_width as usize).saturating_sub(indent + 2);
            for chunk in wrap_words(ws.hint, avail) {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(indent)),
                    Span::styled(chunk, Style::default().fg(GAUGE_FG).italic()),
                ]));
            }
        }
    }
    lines
}

pub(super) fn draw_podcast_editor(frame: &mut Frame, app: &mut App) {
    let (hint_lines, status_height) = hint_bar(app, frame.area().width);

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(status_height),
    ])
    .split(frame.area());

    let dirty_marker = if app.podcast_editor.dirty { " *" } else { "" };
    let podcast_label = match &app.podcast_editor.target {
        PodcastEditorTarget::Template => "Default Template".to_string(),
        PodcastEditorTarget::Existing(_) => {
            let name = &app.podcast_editor.working_copy.name;
            if name.is_empty() {
                "(unnamed)".to_string()
            } else {
                name.clone()
            }
        }
    };

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" AgentP ", Style::default().fg(TITLE_FG).bold()),
        Span::styled(" > ", Style::default().fg(ORANGE)),
        Span::styled(
            format!("{}{}", podcast_label, dirty_marker),
            Style::default().fg(PINK).bold(),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let in_edit = app.podcast_editor.edit_mode == ConfigEditMode::EditingText;
    let selected_fi = app.podcast_editor.list_state.selected().unwrap_or(0);

    let first_field = app.podcast_editor_first_field();
    let lines = podcast_field_lines(
        &app.podcast_editor.working_copy,
        selected_fi,
        in_edit,
        &app.podcast_editor.text_buffer,
        first_field,
        chunks[1].width,
    );

    let body = Paragraph::new(lines).block(styled_block("Fields"));
    frame.render_widget(body, chunks[1]);

    if app.podcast_editor.show_confirm_discard {
        render_centered_dialog(
            frame,
            Line::from(vec![
                Span::styled("  Unsaved changes — ", Style::default().fg(TEXT)),
                Span::styled(" s ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Save  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" d ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Discard  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ]),
            ORANGE,
        );
    }

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}
