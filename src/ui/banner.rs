//! Podcast-list banner: the mascot mic beside the wordmark, with a one-shot
//! typewriter reveal at startup and an idle blink, both derived from elapsed time
//! at draw time.

use std::time::Duration;

use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};

use super::theme::*;

const REVEAL_COLUMNS_PER_SECOND: u128 = 50;
const BLINK_PERIOD_MS: u128 = 4000;
const BLINK_CLOSED_MS: u128 = 150;

/// How many columns of the wordmark are visible after `elapsed` since startup.
pub fn revealed_columns(elapsed: Duration, total: usize) -> usize {
    let columns = elapsed.as_millis() * REVEAL_COLUMNS_PER_SECOND / 1000;
    usize::try_from(columns).unwrap_or(usize::MAX).min(total)
}

/// Whether the mascot's eyes are closed at `elapsed` since startup.
pub fn eyes_closed(elapsed: Duration) -> bool {
    let phase = elapsed.as_millis() % BLINK_PERIOD_MS;
    phase >= BLINK_PERIOD_MS - BLINK_CLOSED_MS
}

/// The banner rows for `elapsed` since startup.
pub fn banner_lines(elapsed: Duration) -> Vec<Line<'static>> {
    let mic_style = Style::default().fg(ACCENT).add_modifier(Modifier::BOLD);
    let text_style = Style::default().fg(TITLE_FG);
    let closed = eyes_closed(elapsed);
    let width = ASCII_ART_TEXT[0].chars().count();
    let visible = revealed_columns(elapsed, width);

    ASCII_ART_MIC
        .iter()
        .enumerate()
        .zip(ASCII_ART_TEXT.iter())
        .map(|((row, &mic), &text)| {
            let mic = if closed && row == ASCII_ART_MIC_EYES_ROW {
                ASCII_ART_MIC_EYES_CLOSED
            } else {
                mic
            };
            let text: String = text
                .chars()
                .enumerate()
                .map(|(column, ch)| if column < visible { ch } else { ' ' })
                .collect();
            Line::from(vec![
                Span::styled(mic, mic_style),
                Span::styled(text, text_style),
            ])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reveal_starts_empty_and_completes_within_a_second() {
        assert_eq!(revealed_columns(Duration::ZERO, 35), 0);
        assert_eq!(revealed_columns(Duration::from_millis(100), 35), 5);
        assert_eq!(revealed_columns(Duration::from_secs(1), 35), 35);
        assert_eq!(revealed_columns(Duration::from_secs(3600), 35), 35);
    }

    #[test]
    fn eyes_open_at_startup_and_close_briefly_each_period() {
        assert!(!eyes_closed(Duration::ZERO));
        assert!(!eyes_closed(Duration::from_millis(3849)));
        assert!(eyes_closed(Duration::from_millis(3850)));
        assert!(eyes_closed(Duration::from_millis(3999)));
        assert!(!eyes_closed(Duration::from_millis(4000)));
    }

    #[test]
    fn art_rows_share_one_width_so_centering_cannot_skew_them() {
        let mic_width = ASCII_ART_MIC[0].chars().count();
        assert!(
            ASCII_ART_MIC
                .iter()
                .all(|row| row.chars().count() == mic_width)
        );
        assert_eq!(ASCII_ART_MIC_EYES_CLOSED.chars().count(), mic_width);
        let text_width = ASCII_ART_TEXT[0].chars().count();
        assert!(
            ASCII_ART_TEXT
                .iter()
                .all(|row| row.chars().count() == text_width)
        );
    }

    #[test]
    fn banner_rows_keep_their_width_during_the_reveal() {
        let full_width = ASCII_ART_MIC[0].chars().count() + ASCII_ART_TEXT[0].chars().count();
        for elapsed in [
            Duration::ZERO,
            Duration::from_millis(300),
            Duration::from_secs(5),
        ] {
            let lines = banner_lines(elapsed);
            assert_eq!(lines.len(), ASCII_ART_MIC.len());
            for line in lines {
                assert_eq!(line.width(), full_width);
            }
        }
    }

    #[test]
    fn eyes_row_swaps_only_while_blinking() {
        let open = banner_lines(Duration::ZERO);
        let closed = banner_lines(Duration::from_millis(3900));
        let row = ASCII_ART_MIC_EYES_ROW;
        assert_eq!(open[row].spans[0].content, ASCII_ART_MIC[row]);
        assert_eq!(closed[row].spans[0].content, ASCII_ART_MIC_EYES_CLOSED);
        assert_eq!(closed[row + 1].spans[0].content, ASCII_ART_MIC[row + 1]);
    }
}
