//! TUI rendering: screen dispatcher, theme, shared widgets, and per-screen draw functions.

mod about;
mod add_podcast;
mod banner;
mod command_palette;
mod config;
mod download;
mod edit_podcast;
mod episode_select;
mod podcast_list;
mod theme;
mod widgets;

use ratatui::{
    Frame,
    style::Style,
    text::{Line, Span},
    widgets::Block,
};

use crate::app::{App, Screen};

use about::draw_about;
use add_podcast::draw_add_podcast;
use command_palette::draw_command_palette;
use config::draw_config;
use download::draw_download_progress;
use edit_podcast::{draw_edit_podcast_select, draw_podcast_editor};
use episode_select::draw_episode_select;
use podcast_list::draw_podcast_list;
use theme::*;
use widgets::*;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    match app.screen {
        Screen::PodcastList => draw_podcast_list(frame, app),
        Screen::EpisodeSelect => draw_episode_select(frame, app),
        Screen::Downloading => draw_download_progress(frame, app),
        Screen::Config => draw_config(frame, app),
        Screen::EditPodcast => draw_podcast_editor(frame, app),
        Screen::EditPodcastSelect => draw_edit_podcast_select(frame, app),
        Screen::AddPodcast => draw_add_podcast(frame, app),
        Screen::About => draw_about(frame, app),
    }

    if app.pending_open_folder.is_some()
        && app.screen != Screen::EpisodeSelect
        && app.screen != Screen::Config
    {
        render_centered_dialog(
            frame,
            Line::from(vec![
                Span::styled("  Folder does not exist ", Style::default().fg(TEXT)),
                Span::styled(
                    "(created automatically on first download)",
                    Style::default().fg(TEXT_DIM).italic(),
                ),
                Span::styled("  — ", Style::default().fg(TEXT_DIM)),
                Span::styled(" y ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Create & open  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ]),
            ORANGE,
        );
    }

    if app.palette.open {
        draw_command_palette(frame, app);
    }
}
