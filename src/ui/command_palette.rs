//! Renders the command palette overlay with search filtering.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, HighlightSpacing, List, ListItem, Paragraph},
};

use crate::app::{App, COMMANDS, Screen};

use super::theme::*;

pub(super) fn draw_command_palette(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let width = ((area.width as u32 * 60 / 100) as u16)
        .max(40)
        .min(area.width);
    let max_visible: u16 = 12;
    let list_height = (app.palette.matches.len() as u16).min(max_visible);
    let height = (list_height + 4).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let palette_area = Rect::new(x, y, width, height);

    frame.render_widget(Clear, palette_area);

    let outer_block = Block::default()
        .title(" Command Palette ")
        .title_alignment(Alignment::Center)
        .title_style(Style::default().fg(TITLE_FG).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG));

    let inner_area = outer_block.inner(palette_area);
    frame.render_widget(outer_block, palette_area);

    let inner_chunks =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner_area);

    let cursor = if (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        / 500)
        .is_multiple_of(2)
    {
        "_"
    } else {
        " "
    };

    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(ACCENT).bold()),
        Span::styled(app.palette.query.clone(), Style::default().fg(TEXT)),
        Span::styled(cursor.to_string(), Style::default().fg(HIGHLIGHT)),
    ]));
    frame.render_widget(input, inner_chunks[0]);

    let cat_col = 10usize;
    let items: Vec<ListItem> = app
        .palette
        .matches
        .iter()
        .map(|&idx| {
            let cmd = &COMMANDS[idx];
            let desc = match (app.screen, cmd.description_podcast_list) {
                (Screen::PodcastList, Some(alt)) => alt,
                _ => cmd.description,
            };
            let cat = cmd.category;
            let pad = " ".repeat(cat_col.saturating_sub(cat.len()));
            let shortcut_str = cmd
                .shortcut_text(app)
                .map(|keys| format!("  {}", keys))
                .unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::styled(pad, Style::default().fg(TEXT_DIM)),
                Span::styled(cat.to_string(), Style::default().fg(PINK)),
                Span::styled("  ".to_string(), Style::default()),
                Span::styled(desc.to_string(), Style::default().fg(TEXT)),
                Span::styled(shortcut_str, Style::default().fg(TEXT_DIM)),
            ]))
        })
        .collect();

    let list = List::new(items)
        .highlight_style(Style::default().bg(SELECTED_BG).fg(HIGHLIGHT).bold())
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, inner_chunks[1], &mut app.palette.list_state);
}
