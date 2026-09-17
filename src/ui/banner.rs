//! Podcast-list banner: the mascot mic beside the wordmark, with a one-shot
//! typewriter reveal at startup and an idle blink, both derived from elapsed time
//! at draw time.

use std::time::Duration;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph},
};

use super::theme::*;

const MIC: [&str; 6] = [
    " ▄██████▄ ",
    " █ ▀  ▀ █ ",
    " █ ▀▄▄▀ █ ",
    " ▀██▄▄██▀ ",
    "    ██    ",
    "  ▄████▄  ",
];

const WORDMARK: [&str; 6] = [
    "   _                    _   ____",
    "  / \\   __ _  ___ _ __ | |_|  _ \\",
    " / _ \\ / _` |/ _ \\ '_ \\| __| |_) |",
    "/ ___ \\ (_| |  __/ | | | |_|  __/",
    "/_/   \\_\\__, |\\___|_| |_|\\__|_|",
    "        |___/",
];

const EYES_ROW: usize = 1;
const VERTICAL_PADDING: u16 = 1;
const REVEAL_COLUMNS_PER_SECOND: u128 = 50;
const BLINK_PERIOD_MS: u128 = 4000;
const BLINK_CLOSED_MS: u128 = 150;

/// Rows the banner occupies, padding included.
pub(super) const BANNER_HEIGHT: u16 = MIC.len() as u16 + 2 * VERTICAL_PADDING;

fn widest(rows: &[&str]) -> usize {
    rows.iter()
        .map(|row| row.chars().count())
        .max()
        .unwrap_or(0)
}

fn revealed_columns(elapsed: Duration) -> usize {
    (elapsed.as_millis() * REVEAL_COLUMNS_PER_SECOND / 1000) as usize
}

fn eyes_closed(elapsed: Duration) -> bool {
    elapsed.as_millis() % BLINK_PERIOD_MS >= BLINK_PERIOD_MS - BLINK_CLOSED_MS
}

fn prefix(row: &'static str, columns: usize) -> &'static str {
    row.char_indices()
        .nth(columns)
        .map_or(row, |(byte, _)| &row[..byte])
}

fn banner_lines(elapsed: Duration) -> Vec<Line<'static>> {
    let mic_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(TITLE_FG);
    let visible = revealed_columns(elapsed);
    let closed = eyes_closed(elapsed);

    MIC.iter()
        .zip(WORDMARK)
        .enumerate()
        .map(|(row, (&mic, text))| {
            let mic = if closed && row == EYES_ROW {
                Span::styled(mic.replace('▀', "─"), mic_style)
            } else {
                Span::styled(mic, mic_style)
            };
            Line::from(vec![mic, Span::styled(prefix(text, visible), text_style)])
        })
        .collect()
}

/// Draw the banner centered in `area` as it looks `elapsed` after startup.
///
/// The lines are left-aligned inside a box as wide as the finished art, so rows
/// of different lengths and the growing reveal never shift horizontally.
pub(super) fn draw_banner(frame: &mut Frame, area: Rect, elapsed: Duration) {
    let width = (widest(&MIC) + widest(&WORDMARK)) as u16;
    let [area] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let banner = Paragraph::new(banner_lines(elapsed))
        .block(Block::default().padding(Padding::vertical(VERTICAL_PADDING)));
    frame.render_widget(banner, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mic_rows_share_one_width_so_the_wordmark_starts_in_one_column() {
        let width = widest(&MIC);
        assert!(MIC.iter().all(|row| row.chars().count() == width));
    }

    #[test]
    fn reveal_types_the_wordmark_in_one_column_at_a_time() {
        assert_eq!(prefix(WORDMARK[1], revealed_columns(Duration::ZERO)), "");
        assert_eq!(
            prefix(WORDMARK[1], revealed_columns(Duration::from_millis(100))),
            "  / \\"
        );
        assert_eq!(
            prefix(WORDMARK[1], revealed_columns(Duration::from_secs(1))),
            WORDMARK[1]
        );
    }

    #[test]
    fn eyes_close_briefly_at_the_end_of_each_period() {
        assert!(!eyes_closed(Duration::ZERO));
        assert!(!eyes_closed(Duration::from_millis(3849)));
        assert!(eyes_closed(Duration::from_millis(3850)));
        assert!(eyes_closed(Duration::from_millis(3999)));
        assert!(!eyes_closed(Duration::from_millis(4000)));
    }

    #[test]
    fn blinking_changes_only_the_eyes_row() {
        let open = banner_lines(Duration::from_secs(1));
        let closed = banner_lines(Duration::from_millis(3900));
        for row in 0..MIC.len() {
            assert_eq!(open[row] == closed[row], row != EYES_ROW);
        }
        assert_eq!(closed[EYES_ROW].width(), open[EYES_ROW].width());
    }
}
