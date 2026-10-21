//! Shared UI helpers: styled blocks, status bar, key hints, centered dialogs, date formatting.

use std::ops::Range;

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, ListState, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, hint_bar_toggle_key, hints};

use super::theme::*;

/// Display width of every non-empty `format_date` result.
pub(super) const DATE_COLUMN_WIDTH: usize = 12;
/// Blank columns kept between a row's text and its date column.
pub(super) const DATE_COLUMN_GAP: usize = 2;
/// Blank columns kept to the right of the date column. Without them a date sits
/// against the block's edge and reads as if it had been clipped.
pub(super) const DATE_COLUMN_TRAIL: usize = 1;

/// Visible indices for a top-to-bottom list of one-line items with no scroll padding.
pub(super) fn visible_list_range(state: &ListState, len: usize, area: Rect) -> Range<usize> {
    if len == 0 || area.is_empty() {
        return 0..0;
    }
    let height = usize::from(area.height);
    let mut start = state.offset().min(len - 1);
    if let Some(selected) = state.selected().map(|selected| selected.min(len - 1)) {
        if selected < start {
            start = selected;
        } else if selected - start >= height {
            start = selected + 1 - height;
        }
    }
    start..start.saturating_add(height).min(len)
}

pub(super) fn styled_block(title: &str) -> Block<'_> {
    Block::default()
        .title(format!(" {} ", title))
        .title_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(1))
}

pub(super) fn status_bar_block() -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(1))
}

/// Word-wrap `text` on whitespace into chunks that fit within `max_width` characters.
/// Width is measured in Unicode scalar values (`chars().count()`), not display columns,
/// so wide (e.g. CJK) glyphs count as one. Returns at least one chunk (empty if `text` is empty).
pub(super) fn wrap_words(text: &str, max_width: usize) -> Vec<String> {
    let max_width = max_width.max(1);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;
    for word in text.split_whitespace() {
        let w = word.chars().count();
        if current.is_empty() {
            current.push_str(word);
            current_width = w;
        } else if current_width + 1 + w <= max_width {
            current.push(' ');
            current.push_str(word);
            current_width += 1 + w;
        } else {
            lines.push(std::mem::take(&mut current));
            current.push_str(word);
            current_width = w;
        }
    }
    if !current.is_empty() || lines.is_empty() {
        lines.push(current);
    }
    lines
}

/// Truncate `text` to at most `max_width` display columns, appending '…' when shortened.
/// Returns `text` unchanged when it already fits; returns an empty string when `max_width` is 0.
pub(super) fn truncate_ellipsis(text: &str, max_width: usize) -> String {
    if text.bytes().all(|byte| (b' '..=b'~').contains(&byte)) {
        if text.len() <= max_width {
            return text.to_string();
        }
        if max_width == 0 {
            return String::new();
        }
        let mut truncated = String::with_capacity(max_width.saturating_add(2));
        truncated.push_str(&text[..max_width - 1]);
        truncated.push('…');
        return truncated;
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }

    let content_width = max_width - 1;
    let mut s = String::new();
    for ch in text.chars() {
        s.push(ch);
        if s.width() > content_width {
            s.pop();
            break;
        }
    }
    s.push('…');
    s
}

pub(super) fn key_hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {} ", key),
            Style::default().fg(BG).bg(PINK).bold(),
        ),
        Span::styled(format!(" {}  ", label), Style::default().fg(TEXT_DIM)),
    ]
}

fn more_suffix() -> Vec<Span<'static>> {
    key_hint(&hint_bar_toggle_key(), "more")
}

fn less_suffix() -> Vec<Span<'static>> {
    key_hint(&hint_bar_toggle_key(), "less")
}

fn spans_width(spans: &[Span<'_>]) -> u16 {
    spans.iter().map(|s| s.width() as u16).sum()
}

fn suffix_reserve() -> u16 {
    spans_width(&more_suffix())
}

/// The current screen's hints as one flat span list, for fixed-height bars.
pub(super) fn hint_bar_spans(app: &App) -> Vec<Span<'static>> {
    hints(app)
        .into_iter()
        .flat_map(|hint| key_hint(&hint.key, hint.label))
        .collect()
}

/// Render the hint bar for the screen the app is showing.
///
/// The entries, and the order they collapse in, come from the keymap.
pub(super) fn hint_bar(app: &App, total_width: u16) -> (Vec<Line<'static>>, u16) {
    let spans = hints(app)
        .into_iter()
        .map(|hint| key_hint(&hint.key, hint.label))
        .collect();
    build_hint_bar(spans, total_width, app.hint_bar_expanded)
}

fn build_hint_bar(
    hints: Vec<Vec<Span<'static>>>,
    total_width: u16,
    expanded: bool,
) -> (Vec<Line<'static>>, u16) {
    let lines = wrap_hints(hints, hint_inner_width(total_width), expanded);
    let height = hint_bar_height(lines.len());
    (lines, height)
}

pub(super) fn wrap_hints(
    hints: Vec<Vec<Span<'static>>>,
    inner_width: u16,
    expanded: bool,
) -> Vec<Line<'static>> {
    let total_width: u16 = hints.iter().map(|h| spans_width(h)).sum();

    if total_width <= inner_width {
        let spans: Vec<Span<'static>> = hints.into_iter().flatten().collect();
        if spans.is_empty() {
            return vec![Line::default()];
        }
        return vec![Line::from(spans)];
    }

    if !expanded {
        let reserve = suffix_reserve();
        let budget = inner_width.saturating_sub(reserve);
        let mut current: Vec<Span<'static>> = Vec::new();
        let mut current_width: u16 = 0;
        for hint in hints {
            let hint_width = spans_width(&hint);
            if current_width + hint_width > budget {
                break;
            }
            current.extend(hint);
            current_width += hint_width;
        }
        current.extend(more_suffix());
        return vec![Line::from(current)];
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut current: Vec<Span<'static>> = Vec::new();
    let mut current_width: u16 = 0;
    for hint in hints {
        let hint_width = spans_width(&hint);
        if current_width + hint_width > inner_width && !current.is_empty() {
            lines.push(Line::from(std::mem::take(&mut current)));
            current_width = 0;
        }
        current.extend(hint);
        current_width += hint_width;
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    if lines.is_empty() {
        lines.push(Line::default());
    }
    let less = less_suffix();
    let less_width = spans_width(&less);
    if current_width + less_width <= inner_width {
        lines
            .last_mut()
            .expect("lines is non-empty after the default push above")
            .spans
            .extend(less);
    } else {
        lines.push(Line::from(less));
    }
    lines
}

fn hint_bar_height(line_count: usize) -> u16 {
    (line_count.max(1) as u16) + 2
}

fn hint_inner_width(total_width: u16) -> u16 {
    total_width.saturating_sub(4)
}

pub(super) fn render_centered_dialog(frame: &mut Frame, content: Line, border_color: Color) {
    let area = frame.area();
    let width = ((area.width as u32 * 60 / 100) as u16)
        .max(40)
        .min(area.width);
    let height = 5u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let dialog_area = Rect::new(x, y, width, height);
    frame.render_widget(Clear, dialog_area);
    let dialog = Paragraph::new(content).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border_color))
            .style(Style::default().bg(BG))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(dialog, dialog_area);
}

pub(super) fn format_date(raw: Option<&str>) -> String {
    raw.and_then(|d| {
        chrono::DateTime::parse_from_rfc2822(d)
            .ok()
            .map(|dt| dt.format("%b %d, %Y").to_string())
    })
    .unwrap_or_default()
}

/// Columns left for a row's text once a right-aligned date column is reserved,
/// or `None` when `row_width` cannot spare `min_text_width` for the text as well.
///
/// The answer depends only on the row width, so a screen asks once and drops the
/// date column for every row together rather than row by row.
pub(super) fn date_column_text_width(row_width: usize, min_text_width: usize) -> Option<usize> {
    let text_width =
        row_width.saturating_sub(DATE_COLUMN_GAP + DATE_COLUMN_WIDTH + DATE_COLUMN_TRAIL);
    (text_width >= min_text_width).then_some(text_width)
}

/// `date` padded so it ends flush at the right edge of a row whose text occupies
/// `used` of `text_width` columns. Empty when there is no date, which leaves the
/// column blank without moving the text beside it.
pub(super) fn date_column(date: &str, used: usize, text_width: usize) -> String {
    if date.is_empty() {
        return String::new();
    }
    let pad = text_width.saturating_sub(used) + DATE_COLUMN_GAP;
    format!(
        "{}{:>width$}",
        " ".repeat(pad),
        date,
        width = DATE_COLUMN_WIDTH
    )
}

#[cfg(test)]
mod tests {
    use ratatui::{
        buffer::Buffer,
        widgets::{HighlightSpacing, List, ListItem, StatefulWidget},
    };

    use super::*;

    fn test_list(len: usize, visible: Range<usize>) -> List<'static> {
        let items = (0..len).map(|index| {
            if !visible.contains(&index) {
                return ListItem::new(Line::default());
            }
            let title = ["plain", "你好 👩🏽‍💻", "line\nbreak", "", "e\u{301} 🇮🇱"][index % 5];
            ListItem::new(Line::from(vec![
                Span::styled(format!("{index}: {title}"), Style::default().fg(PINK)),
                Span::styled(" tail", Style::default().fg(TEXT_DIM).italic()),
            ]))
        });
        List::new(items)
            .block(styled_block("Rows"))
            .highlight_symbol("  > ")
            .highlight_spacing(HighlightSpacing::Always)
            .highlight_style(Style::default().bg(SELECTED_BG).bold())
    }

    #[test]
    fn visible_rows_match_full_ratatui_buffers_and_states() {
        for len in [0, 1, 2, 5, 17, 80] {
            for offset in [0, 1, 4, 16, 79, usize::MAX] {
                for selected in [
                    None,
                    Some(0),
                    Some(1),
                    Some(4),
                    Some(16),
                    Some(79),
                    Some(usize::MAX),
                ] {
                    for width in [0, 1, 3, 8, 40] {
                        for height in [0, 1, 2, 3, 5, 12] {
                            let area = Rect::new(2, 3, width, height);
                            let mut expected_state = ListState::default();
                            expected_state.select(selected);
                            *expected_state.offset_mut() = offset;
                            let mut actual_state = expected_state;
                            let visible = visible_list_range(
                                &actual_state,
                                len,
                                styled_block("Rows").inner(area),
                            );
                            let mut expected = Buffer::empty(area);
                            let mut actual = Buffer::empty(area);
                            test_list(len, 0..len).render(area, &mut expected, &mut expected_state);
                            test_list(len, visible).render(area, &mut actual, &mut actual_state);
                            assert_eq!(
                                actual, expected,
                                "len {len}, offset {offset}, selected {selected:?}, area {area:?}"
                            );
                            assert_eq!(actual_state, expected_state);
                        }
                    }
                }
            }
        }
    }

    fn hint(label: &'static str) -> Vec<Span<'static>> {
        vec![Span::raw(label)]
    }

    #[test]
    fn fits_on_one_line_no_indicator() {
        let hints = vec![hint("a"), hint("b")];
        let lines = wrap_hints(hints, 100, false);
        assert_eq!(lines.len(), 1);
        let joined: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(!joined.contains("more"));
        assert!(!joined.contains("less"));
    }

    #[test]
    fn overflow_collapsed_shows_more() {
        let hints = vec![hint("aaaaaaaaaa"); 10];
        let lines = wrap_hints(hints, 30, false);
        assert_eq!(lines.len(), 1);
        let joined: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.contains("more"));
        assert!(!joined.contains("less"));
    }

    #[test]
    fn overflow_expanded_shows_less() {
        let hints = vec![hint("aaaaaaaaaa"); 10];
        let lines = wrap_hints(hints, 30, true);
        assert!(lines.len() >= 2);
        let last: String = lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(last.contains("less"));
    }

    fn line_width(line: &Line<'_>) -> u16 {
        spans_width(&line.spans)
    }

    #[test]
    fn truncate_ellipsis_leaves_short_text_unchanged() {
        assert_eq!(truncate_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_ellipsis_shortens_long_text() {
        let out = truncate_ellipsis("a very long episode title", 10);
        assert_eq!(out.chars().count(), 10);
        assert!(out.ends_with('…'));
        assert_eq!(out, "a very lo…");
    }

    #[test]
    fn truncate_ellipsis_handles_tiny_widths() {
        assert_eq!(truncate_ellipsis("abc", 0), "");
        assert_eq!(truncate_ellipsis("abc", 1), "…");
    }

    #[test]
    fn truncate_ellipsis_respects_display_width() {
        assert_eq!(truncate_ellipsis("你好世界", 5), "你好…");
        assert_eq!(truncate_ellipsis("你好", 4), "你好");
        assert_eq!(truncate_ellipsis("👨‍👩‍👧‍👦 family", 3), "👨‍👩‍👧‍👦…");
        assert_eq!(truncate_ellipsis("\r\nabc", 3), "\r\na…");
    }

    fn original_truncate_ellipsis(text: &str, max_width: usize) -> String {
        if text.width() <= max_width {
            return text.to_string();
        }
        if max_width == 0 {
            return String::new();
        }
        let mut result = String::new();
        for ch in text.chars() {
            result.push(ch);
            if result.width() > max_width - 1 {
                result.pop();
                break;
            }
        }
        result.push('…');
        result
    }

    #[test]
    fn truncation_matches_original_for_ascii_unicode_and_controls() {
        let printable: String = (b' '..=b'~').map(char::from).collect();
        let controls: String = (0..=127u8).map(char::from).collect();
        let corpus = [
            "",
            "one short title",
            &printable,
            &controls,
            "你好世界 科学与历史",
            "👨‍👩‍👧‍👦 family 👩🏽‍💻 technology ❤️ news",
            "e\u{301}ducation and cafe\u{301}",
            "🇺🇸🇮🇱🇯🇵 flags",
            "\r\nabc\tdef\0ghi",
            "\u{200d}\u{fe0f}\u{301}",
        ];
        for text in corpus {
            for width in (0..=128).chain([usize::MAX]) {
                assert_eq!(
                    truncate_ellipsis(text, width),
                    original_truncate_ellipsis(text, width),
                    "text {text:?}, width {width}"
                );
            }
        }
        for byte in 0..=127u8 {
            let text = format!("prefix{}suffix", char::from(byte));
            for width in 0..=text.len() + 1 {
                assert_eq!(
                    truncate_ellipsis(&text, width),
                    original_truncate_ellipsis(&text, width),
                    "byte {byte}, width {width}"
                );
            }
        }
    }

    #[test]
    fn formatted_dates_match_the_reserved_column_width() {
        for raw in [
            "10 Sep 2026 10:00:00 +0000",
            "2 May 2025 08:30:00 +0200",
            "Mon, 31 Dec 2001 23:59:00 -0500",
        ] {
            assert_eq!(format_date(Some(raw)).width(), DATE_COLUMN_WIDTH, "{}", raw);
        }
    }

    #[test]
    fn date_column_reserved_only_when_the_text_still_fits() {
        let reserved = DATE_COLUMN_GAP + DATE_COLUMN_WIDTH + DATE_COLUMN_TRAIL;
        assert_eq!(date_column_text_width(60, 12), Some(60 - reserved));
        assert_eq!(date_column_text_width(reserved + 12, 12), Some(12));
        assert_eq!(date_column_text_width(reserved + 11, 12), None);
        assert_eq!(date_column_text_width(0, 12), None);
    }

    #[test]
    fn date_column_pads_out_to_the_right_edge() {
        let text_width = 40;
        let short = date_column("Sep 10, 2026", 5, text_width);
        let long = date_column("May 02, 2025", 38, text_width);
        assert_eq!(
            5 + short.width(),
            text_width + DATE_COLUMN_GAP + DATE_COLUMN_WIDTH
        );
        assert_eq!(
            38 + long.width(),
            text_width + DATE_COLUMN_GAP + DATE_COLUMN_WIDTH
        );
        assert!(short.ends_with("Sep 10, 2026"));
    }

    #[test]
    fn date_column_is_empty_without_a_date() {
        assert!(date_column("", 5, 40).is_empty());
    }

    #[test]
    fn collapsed_narrow_does_not_overflow_with_first_oversized_hint() {
        let inner_width = 15u16;
        let hints = vec![hint("this_is_a_very_long_hint")];
        let lines = wrap_hints(hints, inner_width, false);
        assert_eq!(lines.len(), 1);
        assert!(
            line_width(&lines[0]) <= inner_width,
            "collapsed line width {} must not exceed {}",
            line_width(&lines[0]),
            inner_width
        );
        let joined: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(joined.contains("more"));
    }

    #[test]
    fn expanded_less_suffix_fits_on_new_line_when_last_is_full() {
        let inner_width = 30u16;
        let mut hints = vec![hint("aaaaaaaaaa"); 3];
        hints.push(hint("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        let lines = wrap_hints(hints, inner_width, true);
        for l in &lines {
            assert!(
                line_width(l) <= inner_width,
                "line width {} must not exceed inner_width {}",
                line_width(l),
                inner_width
            );
        }
        let last: String = lines
            .last()
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(last.contains("less"));
    }
}
