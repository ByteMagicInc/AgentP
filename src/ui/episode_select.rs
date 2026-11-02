//! Renders the episode selection screen with checkboxes and sorting.

use std::borrow::Cow;

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

use crate::app::{App, EpisodePrompt};

use super::theme::*;
use super::widgets::*;

const MIN_TITLE_WIDTH: usize = 12;
const CHECKBOX_WIDTH: usize = 5;
const HIGHLIGHT_SYMBOL: &str = "  > ";

/// The two text spans of one episode row.
struct EpisodeRow<'a> {
    title: Cow<'a, str>,
    date: String,
}

/// Lay out one episode row so the dates form a right-aligned column: the title
/// takes the space left over and the date is padded out to the right edge.
///
/// `text_width` is the width available after the checkbox. Unlike the podcast
/// list the title is the row's only text, so when the row is too narrow to keep
/// both it is the date column that goes, for every row at once.
fn episode_row<'a>(title: &'a str, date: &str, text_width: usize) -> EpisodeRow<'a> {
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

    let bar = episode_bar(app);
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(u16::from(bar.is_some())),
        Constraint::Min(5),
        Constraint::Length(status_height),
    ])
    .split(frame.area());

    let selected_count = app.selected_count();
    let total_count = app.episodes.len();
    let counter = if total_count == 0 {
        String::new()
    } else if app.episode_filter_active() {
        format!(
            "  {}/{}  ({} shown)",
            selected_count,
            total_count,
            app.episode_visible.len()
        )
    } else {
        format!("  {}/{}", selected_count, total_count)
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

    if let Some(bar) = bar {
        frame.render_widget(Paragraph::new(bar), chunks[1]);
    }

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
        frame.render_widget(loading, chunks[2]);
    } else if let Some(err) = &app.episode_load_error.clone() {
        let error = Paragraph::new(Line::from(vec![
            Span::styled("  Error: ", Style::default().fg(ERROR_FG).bold()),
            Span::styled(err.to_string(), Style::default().fg(ERROR_FG)),
        ]))
        .block(styled_block("Episodes"));
        frame.render_widget(error, chunks[2]);
    } else if app.episode_visible.is_empty() {
        let message = if app.episode_filter_active() {
            format!("  No episodes match \"{}\"", app.episode_query.trim())
        } else {
            "  No episodes found".to_string()
        };
        let empty = Paragraph::new(Line::from(Span::styled(
            message,
            Style::default().fg(TEXT_DIM).italic(),
        )))
        .block(styled_block("Episodes"));
        frame.render_widget(empty, chunks[2]);
    } else {
        let number_digits = app
            .episodes
            .iter()
            .map(|episode| episode.feed_index)
            .max()
            .unwrap_or(0)
            .to_string()
            .len();
        let inner_width = styled_block("Episodes").inner(chunks[2]).width as usize;
        let text_width = inner_width
            .saturating_sub(HIGHLIGHT_SYMBOL.width())
            .saturating_sub(CHECKBOX_WIDTH)
            .saturating_sub(number_digits + 1);
        let (visible, mut window_state) = list_window(
            &mut app.episode_list_state,
            app.episode_visible.len(),
            styled_block("Episodes").inner(chunks[2]),
        );
        let items: Vec<ListItem> = app
            .episode_visible
            .iter()
            .skip(visible.start)
            .take(visible.len())
            .map(|&i| {
                let ep = &app.episodes[i];
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

                let number = format!("{:>width$} ", ep.feed_index, width = number_digits);

                ListItem::new(Line::from(vec![
                    Span::styled(checkbox, check_style),
                    Span::styled(number, Style::default().fg(TEXT_DIM)),
                    Span::styled(row.title, title_style),
                    Span::styled(
                        row.date,
                        Style::default().fg(TEXT_DIM).add_modifier(Modifier::ITALIC),
                    ),
                ]))
            })
            .collect();

        let item_count = app.episode_visible.len();
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

        frame.render_stateful_widget(list, chunks[2], &mut window_state);

        let scrollbar =
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(BORDER));
        let mut scrollbar_state = ScrollbarState::new(item_count)
            .position(app.episode_list_state.selected().unwrap_or(0));
        frame.render_stateful_widget(
            scrollbar,
            chunks[2].inner(Margin {
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
    frame.render_widget(status, chunks[3]);
}

/// The one-line search or go-to prompt, or a note that a search is narrowing the list.
fn episode_bar(app: &App) -> Option<Line<'static>> {
    let total = app.episodes.len();
    let shown = app.episode_visible.len();
    let dim = Style::default().fg(TEXT_DIM);
    match app.episode_prompt {
        Some(EpisodePrompt::Search) => Some(Line::from(vec![
            Span::styled(" Search: ", Style::default().fg(ORANGE).bold()),
            Span::styled(
                format!("{}_", app.episode_query),
                Style::default().fg(HIGHLIGHT),
            ),
            Span::styled(
                format!("   {shown} of {total} episodes · title, description, date"),
                dim,
            ),
        ])),
        Some(EpisodePrompt::Jump) => {
            let newest = app
                .episodes
                .iter()
                .map(|episode| episode.feed_index)
                .max()
                .unwrap_or(0);
            let mut spans = vec![
                Span::styled(" Go to episode #: ", Style::default().fg(ORANGE).bold()),
                Span::styled(
                    format!("{}_", app.episode_jump_buffer),
                    Style::default().fg(HIGHLIGHT),
                ),
            ];
            match &app.episode_jump_error {
                Some(error) => spans.push(Span::styled(
                    format!("   {error}"),
                    Style::default().fg(ERROR_FG),
                )),
                None => spans.push(Span::styled(
                    format!("   1 = oldest, {newest} = newest"),
                    dim,
                )),
            }
            Some(Line::from(spans))
        }
        None if app.episode_filter_active() => Some(Line::from(vec![
            Span::styled(" Filter: ", Style::default().fg(ORANGE).bold()),
            Span::styled(
                app.episode_query.trim().to_string(),
                Style::default().fg(HIGHLIGHT),
            ),
            Span::styled(format!("   {shown} of {total} episodes"), dim),
        ])),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

    use crate::app::Screen;
    use crate::podcast::{BannerStyle, Config, DefaultMode, EpisodeInfo, Podcast};

    use super::*;

    fn total_width(row: &EpisodeRow<'_>) -> usize {
        row.title.width() + row.date.width()
    }

    fn episode_app(episodes: &[(&str, &str)]) -> App {
        let config = Config {
            download_dir_location: String::new(),
            podcasts: vec![Podcast {
                name: "Show".to_string(),
                ..Default::default()
            }],
            default_podcast: Podcast::default(),
            default_mode: DefaultMode::default(),
            banner_style: BannerStyle::default(),
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
                description: String::new(),
            })
            .collect();
        app.episode_selected = vec![false; episodes.len()];
        app.reset_episode_view();
        app.episode_list_state.select(Some(0));
        app
    }

    fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal
            .draw(|frame| draw_episode_select(frame, app))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn rendered_rows(width: u16, episodes: &[(&str, &str)]) -> Vec<String> {
        let mut app = episode_app(episodes);
        buffer_rows(&draw(&mut app, width, 40))
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

    #[test]
    fn a_scrolled_list_shows_the_rows_around_the_selection() {
        let titles: Vec<String> = (0..100).map(|i| format!("Episode {i:03}")).collect();
        let episodes: Vec<(&str, &str)> = titles
            .iter()
            .map(|title| (title.as_str(), "10 Sep 2026 10:00:00 +0000"))
            .collect();
        let mut app = episode_app(&episodes);
        app.episode_selected[75] = true;
        app.episode_selected[80] = true;
        app.episode_list_state.select(Some(80));

        let buffer = draw(&mut app, 100, 30);
        let rows = buffer_rows(&buffer);
        let row_of = |title: &str| rows.iter().position(|row| row.contains(title));

        assert_eq!(app.episode_list_state.selected(), Some(80));
        assert!(app.episode_list_state.offset() > 0);
        assert_eq!(row_of("Episode 000"), None);
        assert_eq!(row_of("Episode 081"), None);

        let shown: Vec<usize> = (0..100)
            .filter(|i| row_of(&format!("Episode {i:03}")).is_some())
            .collect();
        assert!(shown.len() > 1);
        assert_eq!(*shown.last().unwrap(), 80);
        assert!(
            shown.windows(2).all(|pair| pair[1] == pair[0] + 1),
            "visible episodes should be contiguous: {shown:?}"
        );
        for &i in &shown {
            let y = row_of(&format!("Episode {i:03}")).unwrap();
            let row = &rows[y];
            let checked = i == 75 || i == 80;
            assert_eq!(row.contains("[x]"), checked, "checkbox of episode {i}");
            assert_eq!(row.contains(" [ ] "), !checked, "checkbox of episode {i}");
            let highlighted = row.contains(HIGHLIGHT_SYMBOL);
            assert_eq!(highlighted, i == 80, "highlight on episode {i}");
            let title_x = row.find("Episode").unwrap() as u16;
            assert_eq!(
                buffer[(title_x, y as u16)].bg == SELECTED_BG,
                i == 80,
                "selected background on episode {i}"
            );
        }
    }

    fn numbered_app() -> App {
        let mut app = episode_app(&[
            ("Alpha", "Wed, 10 Sep 2025 06:00:00 +0000"),
            ("Beta", "Wed, 10 Sep 2025 06:00:00 +0000"),
            ("Gamma", "Wed, 10 Sep 2025 06:00:00 +0000"),
        ]);
        for (i, episode) in app.episodes.iter_mut().enumerate() {
            episode.feed_index = i + 1;
        }
        app.episodes[1].description = "mentions a Zebra".to_string();
        app
    }

    fn screen_text(app: &mut App) -> String {
        buffer_rows(&draw(app, 90, 14)).join(
            "
",
        )
    }

    #[test]
    fn every_row_shows_its_feed_number() {
        let mut app = numbered_app();
        let text = screen_text(&mut app);
        for (number, title) in [(1, "Alpha"), (2, "Beta"), (3, "Gamma")] {
            let row = text.lines().find(|row| row.contains(title)).unwrap();
            assert!(
                row.contains(&format!("{number} {title}")),
                "row for {title}: {row}"
            );
        }
    }

    #[test]
    fn a_filtered_list_draws_only_its_matches_with_their_own_checkboxes() {
        let mut app = numbered_app();
        app.episode_selected[1] = true;
        app.episode_search_open();
        app.episode_query = "zebra".to_string();
        app.episode_prompt_edited();
        let text = screen_text(&mut app);
        assert!(text.contains("Search: zebra_"), "{text}");
        assert!(text.contains("1 of 3 episodes"), "{text}");
        assert!(text.contains("1/3  (1 shown)"), "{text}");
        assert!(!text.contains("Alpha") && !text.contains("Gamma"), "{text}");
        let row = text.lines().find(|row| row.contains("Beta")).unwrap();
        assert!(
            row.contains("[x]"),
            "selection belongs to the episode: {row}"
        );
    }

    #[test]
    fn a_kept_filter_is_announced_and_an_empty_result_says_so() {
        let mut app = numbered_app();
        app.episode_search_open();
        app.episode_query = "nothing like this".to_string();
        app.episode_prompt_edited();
        app.episode_search_commit();
        let text = screen_text(&mut app);
        assert!(text.contains("Filter: nothing like this"), "{text}");
        assert!(text.contains("No episodes match"), "{text}");
    }

    #[test]
    fn the_go_to_prompt_explains_the_numbering_and_reports_a_missing_episode() {
        let mut app = numbered_app();
        app.episode_jump_open(Some('4'));
        let text = screen_text(&mut app);
        assert!(text.contains("Go to episode #: 4_"), "{text}");
        assert!(text.contains("1 = oldest, 3 = newest"), "{text}");

        app.episode_jump_commit();
        let text = screen_text(&mut app);
        assert!(text.contains("No episode #4"), "{text}");
    }
}
