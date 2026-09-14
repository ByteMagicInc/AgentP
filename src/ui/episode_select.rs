//! Renders the episode selection screen with checkboxes and sorting.

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
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::theme::*;
use super::widgets::*;

const MIN_TITLE_WIDTH: usize = 12;
const CHECKBOX_WIDTH: usize = 5;
const HIGHLIGHT_SYMBOL: &str = "  > ";

/// The two text spans of one episode row.
struct EpisodeRow {
    title: String,
    date: String,
}

/// Lay out one episode row so the dates form a right-aligned column: the title
/// takes the space left over and the date is padded out to the right edge.
///
/// `text_width` is the width available after the checkbox. Unlike the podcast
/// list the title is the row's only text, so when the row is too narrow to keep
/// both it is the date column that goes, for every row at once.
fn episode_row(title: &str, date: &str, text_width: usize) -> EpisodeRow {
    let reserved = date_column_text_width(text_width, MIN_TITLE_WIDTH);
    let title_width = reserved.unwrap_or(text_width);

    let title = truncate_ellipsis(title, title_width);
    let date = if reserved.is_some() {
        date_column(date, title.width(), title_width)
    } else {
        String::new()
    };

    EpisodeRow { title, date }
}

pub(super) fn draw_episode_select(frame: &mut Frame, app: &mut App) {
    let (hint_lines, status_height) = hint_bar(app, frame.area().width);

    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(status_height),
    ])
    .split(frame.area());

    let selected_count = app.selected_count();
    let total_count = app.episodes.len();
    let counter = if total_count > 0 {
        format!("  {}/{}", selected_count, total_count)
    } else {
        String::new()
    };

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" AgentP ", Style::default().fg(TITLE_FG).bold()),
        Span::styled(" > ", Style::default().fg(ORANGE)),
        Span::styled(
            app.selected_podcast_name().to_string(),
            Style::default().fg(PINK).bold(),
        ),
        Span::styled(counter, Style::default().fg(ACCENT).bold()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    if app.loading_episodes {
        let dots = match (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            / 400)
            % 4
        {
            0 => ".",
            1 => "..",
            2 => "...",
            _ => "",
        };
        let loading = Paragraph::new(Line::from(vec![Span::styled(
            format!("  Loading episodes{}", dots),
            Style::default().fg(HIGHLIGHT),
        )]))
        .block(styled_block("Episodes"));
        frame.render_widget(loading, chunks[1]);
    } else if let Some(err) = &app.episode_load_error.clone() {
        let error = Paragraph::new(Line::from(vec![
            Span::styled("  Error: ", Style::default().fg(ERROR_FG).bold()),
            Span::styled(err.to_string(), Style::default().fg(ERROR_FG)),
        ]))
        .block(styled_block("Episodes"));
        frame.render_widget(error, chunks[1]);
    } else {
        let inner_width = styled_block("Episodes").inner(chunks[1]).width as usize;
        let text_width = inner_width
            .saturating_sub(HIGHLIGHT_SYMBOL.width())
            .saturating_sub(CHECKBOX_WIDTH);
        let items: Vec<ListItem> = app
            .episodes
            .iter()
            .enumerate()
            .map(|(i, ep)| {
                let selected = app.episode_selected[i];
                let checkbox = if selected { " [x] " } else { " [ ] " };
                let check_style = if selected {
                    Style::default().fg(CHECK_ON).bold()
                } else {
                    Style::default().fg(TEXT_DIM)
                };

                let title_style = if selected {
                    Style::default().fg(PINK).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(GAUGE_FG).add_modifier(Modifier::BOLD)
                };

                let row = episode_row(&ep.title, &format_date(ep.pub_date.as_deref()), text_width);

                ListItem::new(Line::from(vec![
                    Span::styled(checkbox.to_string(), check_style),
                    Span::styled(row.title, title_style),
                    Span::styled(
                        row.date,
                        Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
                    ),
                ]))
            })
            .collect();

        let item_count = items.len();
        let list = List::new(items)
            .block(styled_block("Episodes"))
            .highlight_style(
                Style::default()
                    .bg(SELECTED_BG)
                    .fg(HIGHLIGHT)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(HIGHLIGHT_SYMBOL)
            .highlight_spacing(HighlightSpacing::Always);

        frame.render_stateful_widget(list, chunks[1], &mut app.episode_list_state);

        let scrollbar =
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(BORDER));
        let mut scrollbar_state = ScrollbarState::new(item_count)
            .position(app.episode_list_state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            scrollbar,
            chunks[1].inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }

    if app.pending_open_folder.is_some() {
        render_centered_dialog(
            frame,
            Line::from(vec![
                Span::styled("  Folder does not exist ", Style::default().fg(TEXT)),
                Span::styled(
                    "(created automatically when you download this podcast)",
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

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::Screen;
    use crate::podcast::{Config, DefaultMode, EpisodeInfo, Podcast};

    use super::*;

    fn total_width(row: &EpisodeRow) -> usize {
        row.title.width() + row.date.width()
    }

    fn rendered_rows(width: u16, episodes: &[(&str, &str)]) -> Vec<String> {
        let config = Config {
            download_dir_location: String::new(),
            podcasts: vec![Podcast {
                name: "Show".to_string(),
                ..Default::default()
            }],
            default_podcast: Podcast::default(),
            default_mode: DefaultMode::default(),
        };
        let mut app = App::new(config, None);
        app.screen = Screen::EpisodeSelect;
        app.episodes = episodes
            .iter()
            .enumerate()
            .map(|(i, (title, date))| EpisodeInfo {
                title: (*title).to_string(),
                feed_index: i,
                pub_date: Some((*date).to_string()),
            })
            .collect();
        app.episode_selected = vec![false; episodes.len()];
        app.episode_list_state.select(Some(0));

        let mut terminal = Terminal::new(TestBackend::new(width, 40)).expect("test backend");
        terminal
            .draw(|frame| draw_episode_select(frame, &mut app))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn date_columns(rows: &[String], date: &str) -> Vec<usize> {
        rows.iter()
            .filter_map(|row| row.find(date).map(|at| row[..at].chars().count()))
            .collect()
    }

    #[test]
    fn dates_line_up_regardless_of_title_length() {
        let width = 80;
        let short = episode_row("Ep 1", "Sep 10, 2026", width);
        let long = episode_row(
            "An extremely long episode title that will not fit in the row at all",
            "May 02, 2025",
            width,
        );
        assert_eq!(total_width(&short), total_width(&long));
        assert!(total_width(&short) < width);
        assert!(short.date.ends_with("Sep 10, 2026"));
        assert!(long.title.ends_with('…'));
    }

    #[test]
    fn missing_date_leaves_the_column_blank_without_moving_the_title() {
        let with_date = episode_row("Episode title", "Sep 10, 2026", 60);
        let without = episode_row("Episode title", "", 60);
        assert_eq!(with_date.title, without.title);
        assert!(without.date.is_empty());
    }

    #[test]
    fn narrow_rows_drop_the_date_column_and_widen_the_title() {
        let narrow = episode_row("A fairly long episode title", "Sep 10, 2026", 20);
        assert!(narrow.date.is_empty());
        assert_eq!(narrow.title.width(), 20);
    }

    #[test]
    fn rows_never_exceed_the_available_width() {
        for width in 0..90 {
            let row = episode_row("A rather long episode title", "Sep 10, 2026", width);
            assert!(
                total_width(&row) <= width,
                "row width {} exceeds available {}",
                total_width(&row),
                width
            );
        }
    }

    #[test]
    fn rendered_dates_share_a_column() {
        let rows = rendered_rows(
            100,
            &[
                ("Ep 1", "10 Sep 2026 10:00:00 +0000"),
                (
                    "An extremely long episode title that keeps going and going and going and going",
                    "10 Sep 2026 10:00:00 +0000",
                ),
            ],
        );
        let columns = date_columns(&rows, "Sep 10, 2026");
        assert_eq!(columns.len(), 2, "both dates should render in full");
        assert_eq!(columns[0], columns[1], "dates should start at one column");
    }

    #[test]
    fn a_narrow_terminal_drops_the_date_from_every_row_at_once() {
        let rows = rendered_rows(
            34,
            &[
                ("Ep 1", "10 Sep 2026 10:00:00 +0000"),
                (
                    "An extremely long episode title",
                    "10 Sep 2026 10:00:00 +0000",
                ),
            ],
        );
        assert!(date_columns(&rows, "Sep 10, 2026").is_empty());
    }
}
