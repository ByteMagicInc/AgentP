//! Renders the podcast list screen with latest-episode previews.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, HighlightSpacing, List, ListItem, Padding, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState,
    },
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;

use super::banner::banner_lines;
use super::theme::*;
use super::widgets::*;

const MIN_TEXT_WIDTH: usize = 12;
const MIN_LATEST_TITLE_WIDTH: usize = 4;
const MIN_INDEX_WIDTH: usize = 2;
const INDEX_PADDING: usize = 3;
const HIGHLIGHT_SYMBOL: &str = "  > ";

/// The three text spans of one podcast row.
struct PodcastRow {
    name: String,
    latest: String,
    date: String,
}

/// Lay out one podcast row so the dates form a right-aligned column: the name
/// keeps its space, the latest-episode title takes what is left, and the date is
/// padded out to the right edge.
///
/// `row_width` is the width available after the index prefix. The date column is
/// dropped entirely when it would leave less than `MIN_TEXT_WIDTH` for the name,
/// and the latest-episode title is dropped rather than truncated to a stub.
fn podcast_row(name: &str, latest_title: &str, date: &str, row_width: usize) -> PodcastRow {
    let reserved = date_column_text_width(row_width, MIN_TEXT_WIDTH);
    let text_width = reserved.unwrap_or(row_width);

    let name = truncate_ellipsis(name, text_width);
    let remaining = text_width.saturating_sub(name.width());

    let latest = if latest_title.is_empty() || remaining < DATE_COLUMN_GAP + MIN_LATEST_TITLE_WIDTH
    {
        String::new()
    } else {
        format!(
            "{}{}",
            " ".repeat(DATE_COLUMN_GAP),
            truncate_ellipsis(latest_title, remaining - DATE_COLUMN_GAP)
        )
    };

    let date = if reserved.is_some() {
        date_column(date, name.width() + latest.width(), text_width)
    } else {
        String::new()
    };

    PodcastRow { name, latest, date }
}

pub(super) fn draw_podcast_list(frame: &mut Frame, app: &mut App) {
    let has_notice = app.config_notice.is_some();

    let (hint_lines, status_height) = hint_bar(app, frame.area().width);

    let chunks = Layout::vertical([
        Constraint::Length(ASCII_ART_MIC.len() as u16 + 2),
        if has_notice {
            Constraint::Length(3)
        } else {
            Constraint::Length(0)
        },
        Constraint::Min(5),
        Constraint::Length(status_height),
    ])
    .split(frame.area());

    let banner = Paragraph::new(banner_lines(app.banner_started.elapsed()))
        .alignment(ratatui::layout::Alignment::Center)
        .block(Block::default().padding(Padding::new(2, 2, 1, 0)));
    frame.render_widget(banner, chunks[0]);

    if let Some(notice) = &app.config_notice {
        let notice_widget = Paragraph::new(Line::from(vec![
            Span::styled(" ! ", Style::default().fg(BG).bg(HIGHLIGHT).bold()),
            Span::styled(format!(" {}  ", notice), Style::default().fg(HIGHLIGHT)),
            Span::styled(
                "Press any key to dismiss",
                Style::default().fg(TEXT_DIM).italic(),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(HIGHLIGHT))
                .padding(Padding::horizontal(1)),
        );
        frame.render_widget(notice_widget, chunks[1]);
    }

    let row_width = styled_block("Podcasts").inner(chunks[2]).width as usize;
    let index_width = app
        .config
        .podcasts
        .len()
        .to_string()
        .len()
        .max(MIN_INDEX_WIDTH);
    let available = row_width
        .saturating_sub(HIGHLIGHT_SYMBOL.width())
        .saturating_sub(index_width + INDEX_PADDING);

    let items: Vec<ListItem> = app
        .config
        .podcasts
        .iter()
        .enumerate()
        .map(|(i, podcast)| {
            let (latest_title, latest_date) = app
                .latest_episodes
                .get(i)
                .and_then(|e| e.as_ref())
                .map(|(title, date)| {
                    let date_str = format_date(date.as_deref());
                    (title.clone(), date_str)
                })
                .unwrap_or_default();

            let prefix = format!(" {:>width$}  ", i + 1, width = index_width);
            let row = podcast_row(&podcast.name, &latest_title, &latest_date, available);

            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(ORANGE)),
                Span::styled(
                    row.name,
                    Style::default().fg(PINK).add_modifier(Modifier::BOLD),
                ),
                Span::styled(row.latest, Style::default().fg(GAUGE_FG)),
                Span::styled(
                    row.date,
                    Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
                ),
            ]))
        })
        .collect();

    let item_count = items.len();
    let list = List::new(items)
        .block(styled_block("Podcasts"))
        .highlight_style(
            Style::default()
                .bg(SELECTED_BG)
                .fg(HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(HIGHLIGHT_SYMBOL)
        .highlight_spacing(HighlightSpacing::Always);

    frame.render_stateful_widget(list, chunks[2], &mut app.podcast_list_state);

    let scrollbar =
        Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(BORDER));
    let mut scrollbar_state =
        ScrollbarState::new(item_count).position(app.selected_podcast_index());
    frame.render_stateful_widget(
        scrollbar,
        chunks[2].inner(Margin {
            vertical: 1,
            horizontal: 0,
        }),
        &mut scrollbar_state,
    );

    if let Some(i) = app.podcast_delete_pending {
        let name = app
            .config
            .podcasts
            .get(i)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "podcast".to_string());
        render_centered_dialog(
            frame,
            Line::from(vec![
                Span::styled(
                    format!("  Delete \"{}\"? — ", name),
                    Style::default().fg(TEXT),
                ),
                Span::styled(" y ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Confirm  ", Style::default().fg(TEXT_DIM)),
                Span::styled(" Esc ", Style::default().fg(BG).bg(PINK).bold()),
                Span::styled(" Cancel", Style::default().fg(TEXT_DIM)),
            ]),
            ORANGE,
        );
    }

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[3]);
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use crate::podcast::{Config, DefaultMode, Podcast};

    use super::*;

    fn total_width(row: &PodcastRow) -> usize {
        row.name.width() + row.latest.width() + row.date.width()
    }

    #[test]
    fn dates_line_up_regardless_of_name_and_title_length() {
        let width = 80;
        let short = podcast_row("Pod", "Ep 1", "Sep 10, 2026", width);
        let long = podcast_row(
            "A Rather Long Podcast Name",
            "An extremely long latest episode title that will not fit in the row at all",
            "May 02, 2025",
            width,
        );
        assert_eq!(total_width(&short), total_width(&long));
        assert!(total_width(&short) < width);
        assert!(short.date.ends_with("Sep 10, 2026"));
        assert!(long.date.ends_with("May 02, 2025"));
    }

    #[test]
    fn wide_glyph_names_do_not_shift_the_column() {
        let width = 60;
        let ascii = podcast_row("Podcast", "Latest", "Sep 10, 2026", width);
        let wide = podcast_row("你好世界", "Latest", "Sep 10, 2026", width);
        assert_eq!(total_width(&ascii), total_width(&wide));
        assert!(total_width(&ascii) < width);
    }

    #[test]
    fn missing_date_leaves_the_column_blank_without_moving_the_title() {
        let width = 60;
        let with_date = podcast_row("Podcast", "Latest episode", "Sep 10, 2026", width);
        let without = podcast_row("Podcast", "Latest episode", "", width);
        assert_eq!(with_date.name, without.name);
        assert_eq!(with_date.latest, without.latest);
        assert!(without.date.is_empty());
    }

    #[test]
    fn narrow_rows_drop_the_date_column_before_the_name() {
        let row = podcast_row("Podcast Name", "Latest", "Sep 10, 2026", 20);
        assert!(row.date.is_empty());
        assert_eq!(row.name, "Podcast Name");
    }

    #[test]
    fn latest_title_is_dropped_rather_than_truncated_to_a_stub() {
        let row = podcast_row("A Rather Long Podcast Name", "Latest", "Sep 10, 2026", 42);
        assert!(row.latest.is_empty());
        assert!(row.date.ends_with("Sep 10, 2026"));
    }

    #[test]
    fn rows_never_exceed_the_available_width() {
        for width in 0..90 {
            let row = podcast_row(
                "A Rather Long Podcast Name",
                "A rather long latest episode title",
                "Sep 10, 2026",
                width,
            );
            assert!(
                total_width(&row) <= width,
                "row width {} exceeds available {}",
                total_width(&row),
                width
            );
        }
    }

    fn rendered_rows(width: u16, podcasts: &[(&str, &str, &str)]) -> Vec<String> {
        let config = Config {
            download_dir_location: String::new(),
            podcasts: podcasts
                .iter()
                .map(|(name, _, _)| Podcast {
                    name: (*name).to_string(),
                    ..Default::default()
                })
                .collect(),
            default_podcast: Podcast::default(),
            default_mode: DefaultMode::default(),
        };
        let mut app = App::new(config, None);
        app.latest_episodes = podcasts
            .iter()
            .map(|(_, title, date)| Some(((*title).to_string(), Some((*date).to_string()))))
            .collect();

        let mut terminal = Terminal::new(TestBackend::new(width, 40)).expect("test backend");
        terminal
            .draw(|frame| draw_podcast_list(frame, &mut app))
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

    #[test]
    fn rendered_dates_share_a_column() {
        let rows = rendered_rows(
            100,
            &[
                ("Pod", "Ep 1", "10 Sep 2026 10:00:00 +0000"),
                (
                    "A Rather Long Podcast Name",
                    "An extremely long latest episode title that keeps going and going and going",
                    "10 Sep 2026 10:00:00 +0000",
                ),
            ],
        );
        let columns: Vec<usize> = rows
            .iter()
            .filter_map(|row| row.find("Sep 10, 2026").map(|at| row[..at].chars().count()))
            .collect();
        assert_eq!(columns.len(), 2, "both dates should render in full");
        assert_eq!(columns[0], columns[1], "dates should start at one column");
    }
}
