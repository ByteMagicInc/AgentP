//! Renders the download progress screen with gauge and completed list.

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph},
};

use crate::app::App;

use super::theme::*;
use super::widgets::*;

pub(super) fn draw_download_progress(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(3),
    ])
    .split(frame.area());

    let (title_text, title_color) = if app.download_progress.finished {
        (
            format!(" Download Complete - {}", app.selected_podcast_name()),
            CHECK_ON,
        )
    } else {
        (
            format!(" Downloading: {}", app.selected_podcast_name()),
            HIGHLIGHT,
        )
    };
    let title = Paragraph::new(Line::styled(
        title_text,
        Style::default().fg(title_color).bold(),
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let progress = &app.download_progress;
    let ratio = if progress.total > 0 {
        (progress.completed.len() as f64 / progress.total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let gauge_label = if progress.finished {
        format!(
            "{}/{} episodes complete",
            progress.completed.len(),
            progress.total
        )
    } else if !progress.current_episode.is_empty() {
        format!(
            "{}/{}  {}",
            progress.current_index, progress.total, progress.current_episode
        )
    } else {
        "Fetching feed...".to_string()
    };

    let gauge_style = if progress.finished {
        Style::default().fg(CHECK_ON)
    } else {
        Style::default().fg(GAUGE_FG)
    };

    let gauge = Gauge::default()
        .block(styled_block("Progress"))
        .gauge_style(gauge_style)
        .ratio(ratio)
        .label(Span::styled(gauge_label, Style::default().fg(TEXT)));
    frame.render_widget(gauge, chunks[1]);

    let mut items: Vec<ListItem> = progress
        .completed
        .iter()
        .map(|name| {
            let tagged = if progress.tagged.contains(name) {
                "  tagged"
            } else {
                ""
            };
            ListItem::new(Line::from(vec![
                Span::styled("  + ", Style::default().fg(CHECK_ON)),
                Span::styled(name.clone(), Style::default().fg(TEXT)),
                Span::styled(tagged.to_string(), Style::default().fg(ACCENT_DIM).italic()),
            ]))
        })
        .collect();

    let err_text_width = chunks[2].width.saturating_sub(6) as usize;
    for err in &progress.errors {
        let wrapped = wrap_words(err, err_text_width);
        let lines: Vec<Line> = wrapped
            .into_iter()
            .enumerate()
            .map(|(i, segment)| {
                let prefix = if i == 0 { "  ! " } else { "    " };
                Line::from(vec![
                    Span::styled(prefix, Style::default().fg(ERROR_FG).bold()),
                    Span::styled(segment, Style::default().fg(ERROR_FG)),
                ])
            })
            .collect();
        items.push(ListItem::new(Text::from(lines)));
    }

    let completed_list = List::new(items).block(styled_block("Completed"));
    frame.render_widget(completed_list, chunks[2]);

    let mut spans: Vec<Span> = hint_bar_spans(app);
    if spans.is_empty() {
        spans.push(Span::styled(
            "  Downloading... ",
            Style::default()
                .fg(HIGHLIGHT)
                .add_modifier(Modifier::ITALIC),
        ));
    }

    let status = Paragraph::new(Line::from(spans)).block(status_bar_block());
    frame.render_widget(status, chunks[3]);
}
