//! Renders the config menu screen and confirmation dialogs.

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, HighlightSpacing, List, ListItem, Paragraph},
};

use crate::app::{App, ConfigDialog, ConfigEditMode};

use super::theme::*;
use super::widgets::*;

pub(super) fn draw_config(frame: &mut Frame, app: &mut App) {
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
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let in_edit = app.config_editor.edit_mode == ConfigEditMode::EditingText;
    let selected_fi = app.config_editor.menu_state.selected().unwrap_or(0);

    let dir_val_str = if in_edit {
        format!("{}_", app.config_editor.text_buffer)
    } else {
        app.config.download_dir_location.clone()
    };
    let dir_val_color = if in_edit { HIGHLIGHT } else { TEXT_DIM };
    let saved_hint = if in_edit && !app.config.download_dir_location.is_empty() {
        format!("  (was: {})", app.config.download_dir_location)
    } else {
        String::new()
    };

    let menu_labels: [&str; 11] = [
        "  Add New Podcast         ",
        "  Download Folder         ",
        "  Default Mode:           ",
        "  Banner Style:           ",
        "  New Podcast Defaults    ",
        "  Edit Existing Podcasts ",
        "  Open Config File        ",
        "  Open Podcasts Config File",
        "  Open Download Folder    ",
        "  About AgentP            ",
        "  Report Issue            ",
    ];

    let menu_items: Vec<ListItem> = menu_labels
        .iter()
        .enumerate()
        .map(|(fi, label)| {
            let label_color = if fi == selected_fi { TEXT } else { TEXT_DIM };
            let arrow_color = if fi == selected_fi { ACCENT } else { TEXT_DIM };
            if fi == 1 {
                let mut spans = vec![
                    Span::styled(label.to_string(), Style::default().fg(label_color)),
                    Span::styled(dir_val_str.clone(), Style::default().fg(dir_val_color)),
                    Span::styled(saved_hint.clone(), Style::default().fg(TEXT_DIM).italic()),
                ];
                if fi == selected_fi && !in_edit {
                    spans.push(Span::styled("   ", Style::default()));
                    spans.push(Span::styled(" r ", Style::default().fg(BG).bg(PINK).bold()));
                    spans.push(Span::styled(
                        " reset to default ",
                        Style::default().fg(TEXT_DIM),
                    ));
                }
                ListItem::new(Line::from(spans))
            } else if fi == 2 || fi == 3 {
                let value = if fi == 2 {
                    app.config.default_mode.to_string()
                } else {
                    app.config.banner_style.to_string()
                };
                ListItem::new(Line::from(vec![
                    Span::styled(label.to_string(), Style::default().fg(label_color)),
                    Span::styled(value, Style::default().fg(ORANGE)),
                    Span::styled("  →".to_string(), Style::default().fg(arrow_color)),
                ]))
            } else {
                ListItem::new(Line::from(vec![
                    Span::styled(label.to_string(), Style::default().fg(label_color)),
                    Span::styled("  →".to_string(), Style::default().fg(arrow_color)),
                ]))
            }
        })
        .collect();

    let menu_list = List::new(menu_items)
        .block(styled_block("Config"))
        .highlight_style(Style::default().bg(SELECTED_BG).fg(HIGHLIGHT).bold())
        .highlight_symbol("> ")
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(menu_list, chunks[1], &mut app.config_editor.menu_state);

    if app.config_editor.dialog != ConfigDialog::Closed {
        draw_config_dialog(frame, app);
    }

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}

pub(super) fn draw_config_dialog(frame: &mut Frame, app: &App) {
    let content_line = match app.config_editor.dialog {
        ConfigDialog::ConfirmDirChange => {
            let new_val = app
                .config_editor
                .pending_dir_change
                .as_deref()
                .unwrap_or("")
                .to_string();
            Line::from(vec![
                Span::styled("  Change dir → ", Style::default().fg(TEXT)),
                Span::styled(new_val, Style::default().fg(ORANGE)),
                Span::styled("  — ", Style::default().fg(TEXT_DIM)),
                Span::styled(" y ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Confirm  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ])
        }
        ConfigDialog::ConfirmDirDefault => {
            let default_val = crate::podcast::default_download_dir_location();
            Line::from(vec![
                Span::styled("  Reset dir to default → ", Style::default().fg(TEXT)),
                Span::styled(default_val, Style::default().fg(ORANGE)),
                Span::styled("  — ", Style::default().fg(TEXT_DIM)),
                Span::styled(" y ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Confirm  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ])
        }
        ConfigDialog::ConfirmCreateDir => {
            let folder = app.pending_open_folder.as_deref().unwrap_or("").to_string();
            Line::from(vec![
                Span::styled("  Folder does not exist ", Style::default().fg(TEXT)),
                Span::styled(
                    "(created automatically on first download)",
                    Style::default().fg(TEXT_DIM).italic(),
                ),
                Span::styled(" → ", Style::default().fg(TEXT_DIM)),
                Span::styled(folder, Style::default().fg(ORANGE)),
                Span::styled("  — ", Style::default().fg(TEXT_DIM)),
                Span::styled(" y ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Create & open  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ])
        }
        ConfigDialog::Closed => return,
    };
    render_centered_dialog(frame, content_line, ORANGE);
}
