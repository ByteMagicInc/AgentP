use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout},
    style::Style,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph},
};
use unicode_width::UnicodeWidthStr;

use crate::app::{APP_DESCRIPTION, APP_VERSION, App, ISSUES_URL, REPOSITORY_URL, build_summary};

use super::banner::{BANNER_HEIGHT, draw_banner};
use super::theme::*;
use super::widgets::*;

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.as_ref().width()).sum()
}

fn centered(mut spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let pad = width.saturating_sub(spans_width(&spans)) / 2;
    if pad > 0 {
        let mut padded = vec![Span::raw(" ".repeat(pad))];
        padded.append(&mut spans);
        Line::from(padded)
    } else {
        Line::from(spans)
    }
}

pub(super) fn draw_about(frame: &mut Frame, app: &mut App) {
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
        Span::styled("About", Style::default().fg(PINK).bold()),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(BORDER)),
    );
    frame.render_widget(title, chunks[0]);

    let rows: Vec<(&str, Span<'static>)> = vec![
        (
            "Version",
            Span::styled(APP_VERSION, Style::default().fg(HIGHLIGHT).bold()),
        ),
        (
            "Build",
            Span::styled(build_summary(), Style::default().fg(TEXT)),
        ),
        (
            "Repository",
            Span::styled(REPOSITORY_URL, Style::default().fg(ORANGE)),
        ),
        (
            "Issues",
            Span::styled(ISSUES_URL, Style::default().fg(ORANGE)),
        ),
    ];
    let label_width = rows.iter().map(|(label, _)| label.len()).max().unwrap_or(0) + 2;
    let info_lines: Vec<Vec<Span<'static>>> = rows
        .into_iter()
        .map(|(label, value)| {
            vec![
                Span::styled(
                    format!("{label:<label_width$}"),
                    Style::default().fg(TEXT_DIM),
                ),
                value,
            ]
        })
        .collect();
    let info_width = info_lines
        .iter()
        .map(|spans| spans_width(spans))
        .max()
        .unwrap_or(0);

    let tagline = vec![
        Span::styled("AgentP", Style::default().fg(TITLE_FG).bold()),
        Span::styled(
            format!("  {APP_DESCRIPTION}"),
            Style::default().fg(TEXT_DIM).italic(),
        ),
    ];
    let content_width = info_width.max(spans_width(&tagline));
    let info_pad = content_width.saturating_sub(info_width) / 2;
    let pad_spans = |spans: Vec<Span<'static>>| {
        let mut padded = vec![Span::raw(" ".repeat(info_pad))];
        padded.extend(spans);
        Line::from(padded)
    };

    let mut lines = vec![
        centered(tagline, content_width),
        Line::from(""),
        Line::from(vec![Span::styled(
            "─".repeat(content_width),
            Style::default().fg(BORDER),
        )]),
        Line::from(""),
    ];
    lines.extend(info_lines.into_iter().map(pad_spans));
    if let Some(notice) = app.about_notice.as_deref() {
        lines.push(Line::from(""));
        lines.push(centered(
            vec![Span::styled(
                notice.to_string(),
                Style::default().fg(ACCENT).bold(),
            )],
            content_width,
        ));
    }

    let card_width = (content_width as u16 + 6).min(chunks[1].width).max(1);
    let card_height = (BANNER_HEIGHT + lines.len() as u16 + 2)
        .min(chunks[1].height)
        .max(1);
    let [card] = Layout::horizontal([Constraint::Length(card_width)])
        .flex(Flex::Center)
        .areas(chunks[1]);
    let [card] = Layout::vertical([Constraint::Length(card_height)])
        .flex(Flex::Center)
        .areas(card);
    let block = Block::default()
        .title(" About ")
        .title_style(Style::default().fg(ACCENT).bold())
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER))
        .padding(Padding::horizontal(2));
    let inner = block.inner(card);
    frame.render_widget(block, card);
    let [banner_area, body] =
        Layout::vertical([Constraint::Length(BANNER_HEIGHT), Constraint::Min(0)]).areas(inner);
    draw_banner(
        frame,
        banner_area,
        app.banner_started.elapsed(),
        app.config.banner_style,
    );
    frame.render_widget(Paragraph::new(lines), body);

    let status = Paragraph::new(hint_lines).block(status_bar_block());
    frame.render_widget(status, chunks[2]);
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use ratatui::{Terminal, backend::TestBackend};

    use crate::app::test_app_one_podcast as test_app;

    use super::*;

    fn render_about() -> Vec<String> {
        let mut app = test_app();
        app.banner_started = Instant::now() - Duration::from_secs(60);
        app.enter_about();
        let backend = TestBackend::new(80, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw_about(f, &mut app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn value_column(screen: &[String], label: &str, value: &str) -> usize {
        let line = screen
            .iter()
            .find(|line| line.contains(label))
            .unwrap_or_else(|| panic!("{label} row missing"));
        line.find(value)
            .unwrap_or_else(|| panic!("{value} missing from {label} row"))
    }

    #[test]
    fn about_renders_version_and_links() {
        let joined = render_about().join("\n");
        assert!(joined.contains(APP_VERSION));
        assert!(joined.contains(REPOSITORY_URL));
        assert!(joined.contains(ISSUES_URL));
        assert!(joined.contains("▄██████▄"));
    }

    #[test]
    fn info_values_share_one_column() {
        let screen = render_about();
        let build = build_summary();
        let columns = [
            value_column(&screen, "Version", APP_VERSION),
            value_column(&screen, "Build", &build),
            value_column(&screen, "Repository", REPOSITORY_URL),
            value_column(&screen, "Issues", ISSUES_URL),
        ];
        assert!(
            columns.iter().all(|&c| c == columns[0]),
            "info values misaligned: {columns:?}"
        );
    }

    #[test]
    fn card_is_horizontally_centered() {
        const WIDTH: usize = 80;
        let screen = render_about();
        assert!(screen.iter().all(|line| line.chars().count() == WIDTH));
        let border = screen
            .iter()
            .find(|line| line.contains('╭') && line.contains(" About "))
            .expect("card top border missing");
        let left = border
            .find('╭')
            .map(|b| border[..b].width())
            .expect("card left corner missing");
        let right = border
            .rfind('╮')
            .map(|b| border[..b].width())
            .expect("card right corner missing");
        let right_margin = WIDTH - 1 - right;
        assert!(
            left.abs_diff(right_margin) <= 1,
            "card is not centered: left {left}, right {right_margin}"
        );
    }
}
