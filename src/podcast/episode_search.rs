//! The episode search grammar, shared by the TUI's `/` search and
//! `agentp podcast episodes --search`.
//!
//! A query is words and `"quoted phrases"`, and an episode matches when every
//! term does; a `-` before a term excludes episodes that match it. A word or
//! phrase matches anywhere in the title, the description, or the publish date
//! as the list shows it (`Sep 10, 2025`, plus the full month name). A term
//! written as a date matches episodes published in that period: month first
//! with slashes or dashes (`9/10/2025`, `09-10-2025`, `9/2025`, and `9/10` for
//! that day in any year) or year first (`2025-09-10`, `2025-09`). `#500` and
//! `#100-120` match episode numbers. `since:`, `until:`, `after:` and
//! `before:` take the same dates, a bare year, or a relative day (`today`,
//! `yesterday`, `30d`, `2w`, `6m`, `1y`) and bound the publish date.

use chrono::{Datelike, Months, NaiveDate};

use super::{DISPLAY_DATE_FORMAT, EpisodeInfo};

/// A stretch of the calendar named by a date term.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePeriod {
    Day(NaiveDate),
    Month {
        year: i32,
        month: u32,
    },
    Year(i32),
    /// A month and day in any year, as in `9/10`.
    MonthDay {
        month: u32,
        day: u32,
    },
}

impl DatePeriod {
    fn contains(self, date: NaiveDate) -> bool {
        match self {
            DatePeriod::Day(day) => date == day,
            DatePeriod::Month { year, month } => date.year() == year && date.month() == month,
            DatePeriod::Year(year) => date.year() == year,
            DatePeriod::MonthDay { month, day } => date.month() == month && date.day() == day,
        }
    }

    /// The first and last day of the period; `None` for a day in any year.
    fn bounds(self) -> Option<(NaiveDate, NaiveDate)> {
        match self {
            DatePeriod::Day(day) => Some((day, day)),
            DatePeriod::Month { year, month } => {
                let first = NaiveDate::from_ymd_opt(year, month, 1)?;
                let last = first.checked_add_months(Months::new(1))?.pred_opt()?;
                Some((first, last))
            }
            DatePeriod::Year(year) => Some((
                NaiveDate::from_ymd_opt(year, 1, 1)?,
                NaiveDate::from_ymd_opt(year, 12, 31)?,
            )),
            DatePeriod::MonthDay { .. } => None,
        }
    }
}

fn number(part: &str, max_len: usize) -> Option<u32> {
    if part.is_empty() || part.len() > max_len || !part.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

fn year(part: &str) -> Option<i32> {
    if part.len() != 4 {
        return None;
    }
    number(part, 4).and_then(|y| i32::try_from(y).ok())
}

fn month(part: &str) -> Option<u32> {
    number(part, 2).filter(|m| (1..=12).contains(m))
}

/// Read `text` as a date period, month first. A bare four-digit year counts
/// only when `allow_year` is set, so a plain search for `2025` stays a word
/// that also finds titles mentioning it.
pub fn parse_date_period(text: &str, allow_year: bool) -> Option<DatePeriod> {
    let separator = match (text.contains('/'), text.contains('-')) {
        (true, false) => '/',
        (false, true) => '-',
        (false, false) => {
            return if allow_year {
                year(text).map(DatePeriod::Year)
            } else {
                None
            };
        }
        (true, true) => return None,
    };
    let parts: Vec<&str> = text.split(separator).collect();
    match parts.as_slice() {
        [y, m, d] if y.len() == 4 => {
            NaiveDate::from_ymd_opt(year(y)?, month(m)?, number(d, 2)?).map(DatePeriod::Day)
        }
        [m, d, y] => {
            NaiveDate::from_ymd_opt(year(y)?, month(m)?, number(d, 2)?).map(DatePeriod::Day)
        }
        [y, m] if y.len() == 4 => Some(DatePeriod::Month {
            year: year(y)?,
            month: month(m)?,
        }),
        [m, y] if y.len() == 4 => Some(DatePeriod::Month {
            year: year(y)?,
            month: month(m)?,
        }),
        [m, d] => {
            let (month, day) = (month(m)?, number(d, 2)?);
            NaiveDate::from_ymd_opt(2000, month, day)?;
            Some(DatePeriod::MonthDay { month, day })
        }
        _ => None,
    }
}

/// An episode's searchable text, lowercased once when the list loads.
#[derive(Clone, Debug)]
pub struct SearchEntry {
    feed_index: usize,
    text: String,
    date: Option<NaiveDate>,
    date_text: String,
}

impl SearchEntry {
    pub fn new(episode: &EpisodeInfo) -> Self {
        let text = format!("{}\n{}", episode.title, episode.description).to_lowercase();
        let parsed = episode
            .pub_date
            .as_deref()
            .and_then(|raw| chrono::DateTime::parse_from_rfc2822(raw).ok());
        let date_text = match (&parsed, episode.pub_date.as_deref()) {
            (Some(date), _) => {
                format!("{} {}", date.format(DISPLAY_DATE_FORMAT), date.format("%B")).to_lowercase()
            }
            (None, Some(raw)) => raw.to_lowercase(),
            (None, None) => String::new(),
        };
        SearchEntry {
            feed_index: episode.feed_index,
            text,
            date: parsed.map(|date| date.date_naive()),
            date_text,
        }
    }
}

/// What a single term tests an episode for.
#[derive(Clone, Debug)]
enum Matcher {
    /// A word or a quoted phrase, found anywhere in the title, the description
    /// or the shown date.
    Text(String),
    /// A date, month or month-and-day; also found when written in the text.
    Date { period: DatePeriod, literal: String },
    /// `#500` or `#100-120`: the episode number, inclusive.
    Number { low: usize, high: usize },
}

impl Matcher {
    fn matches(&self, entry: &SearchEntry) -> bool {
        match self {
            Matcher::Text(text) => entry.text.contains(text) || entry.date_text.contains(text),
            Matcher::Date { period, literal } => {
                entry.date.is_some_and(|date| period.contains(date)) || entry.text.contains(literal)
            }
            Matcher::Number { low, high } => (*low..=*high).contains(&entry.feed_index),
        }
    }
}

#[derive(Clone, Debug)]
struct Term {
    matcher: Matcher,
    /// Led by `-`: the episode must not match.
    excluded: bool,
}

/// Which side of the range a `since:`/`until:`/`after:`/`before:` term bounds.
#[derive(Clone, Copy)]
enum Bound {
    Since,
    Until,
    After,
    Before,
}

const BOUNDS: [(&str, Bound); 4] = [
    ("since:", Bound::Since),
    ("until:", Bound::Until),
    ("after:", Bound::After),
    ("before:", Bound::Before),
];

/// One piece of a query as typed: a word, or a phrase in double quotes.
struct Token {
    text: String,
    quoted: bool,
    excluded: bool,
}

/// Split a query into words and quoted phrases. A phrase runs to its closing
/// quote or, while it is still being typed, to the end of the query. A `-`
/// right before a word or phrase excludes it; a `-` on its own is ignored.
fn tokenize(query: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = query.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        let Some(&first) = chars.peek() else {
            return tokens;
        };
        let excluded = first == '-'
            && chars
                .clone()
                .nth(1)
                .is_some_and(|next| !next.is_whitespace());
        if excluded {
            chars.next();
        }
        let quoted = chars.next_if_eq(&'"').is_some();
        let text: String = if quoted {
            chars.by_ref().take_while(|&c| c != '"').collect()
        } else {
            let mut word = String::new();
            while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                word.push(c);
            }
            word
        };
        if text.trim().is_empty() || (!quoted && text == "-") {
            continue;
        }
        tokens.push(Token {
            text,
            quoted,
            excluded,
        });
    }
}

/// `#500` or `#100-120` as an inclusive range of episode numbers, in either order.
fn episode_numbers(word: &str) -> Option<(usize, usize)> {
    let digits = word.strip_prefix('#')?;
    let (low, high) = digits.split_once('-').unwrap_or((digits, digits));
    let number = |part: &str| {
        (!part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse::<usize>().ok())
            .flatten()
    };
    let (low, high) = (number(low)?, number(high)?);
    Some((low.min(high), low.max(high)))
}

/// A day counted back from `today`: `today`, `yesterday`, or a count of days,
/// weeks, months or years ago, as in `30d`, `2w`, `6m` or `1y`.
fn relative_day(value: &str, today: NaiveDate) -> Option<NaiveDate> {
    match value {
        "today" => return Some(today),
        "yesterday" => return today.pred_opt(),
        _ => {}
    }
    let unit = value.chars().last()?;
    let count: u32 = value[..value.len() - unit.len_utf8()].parse().ok()?;
    match unit {
        'd' => today.checked_sub_days(chrono::Days::new(count.into())),
        'w' => today.checked_sub_days(chrono::Days::new(u64::from(count) * 7)),
        'm' => today.checked_sub_months(Months::new(count)),
        'y' => today.checked_sub_months(Months::new(count.checked_mul(12)?)),
        _ => None,
    }
}

/// A parsed search query.
#[derive(Clone, Debug, Default)]
pub struct EpisodeQuery {
    terms: Vec<Term>,
    earliest: Option<NaiveDate>,
    latest: Option<NaiveDate>,
    notice: Option<String>,
    highlights: Vec<String>,
}

impl EpisodeQuery {
    /// Parse `query`, reading relative dates such as `since:30d` against today.
    pub fn parse(query: &str) -> Self {
        Self::parse_at(query, chrono::Local::now().date_naive())
    }

    /// Parse `query`, reading relative dates against `today`.
    pub fn parse_at(query: &str, today: NaiveDate) -> Self {
        let mut parsed = EpisodeQuery::default();
        for token in tokenize(query) {
            let lower = token.text.to_lowercase();
            let matcher = if token.quoted {
                Matcher::Text(lower)
            } else if let Some((prefix, bound, value)) =
                BOUNDS.iter().find_map(|&(prefix, bound)| {
                    lower
                        .strip_prefix(prefix)
                        .map(|value| (prefix, bound, value))
                })
            {
                if token.excluded {
                    parsed.set_notice(format!(
                        "{prefix} cannot be excluded; use the opposite range"
                    ));
                } else {
                    parsed.add_bound(prefix, bound, value, today);
                }
                continue;
            } else if let Some((low, high)) = episode_numbers(&lower) {
                Matcher::Number { low, high }
            } else if let Some(period) = parse_date_period(&lower, false) {
                Matcher::Date {
                    period,
                    literal: lower,
                }
            } else {
                Matcher::Text(lower)
            };
            if let (Matcher::Text(text), false) = (&matcher, token.excluded) {
                parsed.highlights.push(text.clone());
            }
            parsed.terms.push(Term {
                matcher,
                excluded: token.excluded,
            });
        }
        parsed
    }

    fn set_notice(&mut self, notice: String) {
        if self.notice.is_none() {
            self.notice = Some(notice);
        }
    }

    fn add_bound(&mut self, prefix: &str, bound: Bound, value: &str, today: NaiveDate) {
        let period = relative_day(value, today)
            .map(DatePeriod::Day)
            .or_else(|| parse_date_period(value, true));
        let Some((first, last)) = period.and_then(|period| period.bounds()) else {
            self.set_notice(format!(
                "{prefix} needs a date, like {prefix}2025-09 or {prefix}30d"
            ));
            return;
        };
        let (earliest, latest) = match bound {
            Bound::Since => (Some(first), None),
            Bound::Until => (None, Some(last)),
            Bound::After => (last.succ_opt(), None),
            Bound::Before => (None, first.pred_opt()),
        };
        if let Some(date) = earliest {
            self.earliest = Some(self.earliest.map_or(date, |current| current.max(date)));
        }
        if let Some(date) = latest {
            self.latest = Some(self.latest.map_or(date, |current| current.min(date)));
        }
    }

    /// A hint about a term the search had to leave out, such as `after:20`
    /// while it is still being typed.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Whether the query names a publish-date range.
    pub fn has_range(&self) -> bool {
        self.earliest.is_some() || self.latest.is_some()
    }

    /// The lowercased words and phrases worth marking where they appear in a
    /// title: every included text term, but no dates, numbers or exclusions.
    pub fn highlights(&self) -> &[String] {
        &self.highlights
    }

    pub fn matches(&self, entry: &SearchEntry) -> bool {
        if self.has_range() {
            let Some(date) = entry.date else {
                return false;
            };
            if self.earliest.is_some_and(|earliest| date < earliest)
                || self.latest.is_some_and(|latest| date > latest)
            {
                return false;
            }
        }
        self.terms
            .iter()
            .all(|term| term.matcher.matches(entry) != term.excluded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(title: &str, date: &str) -> SearchEntry {
        SearchEntry::new(&EpisodeInfo {
            title: title.to_string(),
            feed_index: 1,
            pub_date: Some(date.to_string()),
            description: String::new(),
        })
    }

    fn matches(query: &str, entry: &SearchEntry) -> bool {
        EpisodeQuery::parse(query).matches(entry)
    }

    const JAN_2: &str = "Fri, 02 Jan 2026 10:00:00 +0000";
    const JAN_12: &str = "Mon, 12 Jan 2026 10:00:00 +0000";
    const NOV_2: &str = "Mon, 02 Nov 2026 10:00:00 +0000";
    const DEC_12: &str = "Sat, 12 Dec 2026 10:00:00 +0000";

    #[test]
    fn a_date_matches_with_any_padding_and_either_separator() {
        let jan_2 = entry("Kickoff", JAN_2);
        for query in [
            "1/2/2026",
            "01/2/2026",
            "1/02/2026",
            "01/02/2026",
            "1-2-2026",
            "01-02-2026",
            "2026-01-02",
            "2026/1/2",
        ] {
            assert!(matches(query, &jan_2), "{query}");
        }
        assert!(!matches("2/1/2026", &jan_2));
    }

    #[test]
    fn the_users_examples_find_november_second() {
        let nov_2 = entry("Release", NOV_2);
        assert!(matches("11-02-2026", &nov_2));
        assert!(matches("11/02/2026", &nov_2));
        assert!(!matches("02/11/2026", &nov_2));
    }

    #[test]
    fn a_month_term_matches_only_that_month() {
        let jan_12 = entry("Mid January", JAN_12);
        let dec_12 = entry("Mid December", DEC_12);
        assert!(matches("12-2026", &dec_12));
        assert!(!matches("12-2026", &jan_12));
        assert!(matches("2026-01", &jan_12));
        assert!(matches("1/2026", &jan_12));
        assert!(!matches("1/2026", &dec_12));
    }

    #[test]
    fn a_month_and_day_match_in_any_year() {
        assert!(matches("1/2", &entry("New year", JAN_2)));
        assert!(matches(
            "1/2",
            &entry("Old", "Thu, 02 Jan 2020 10:00:00 +0000")
        ));
        assert!(!matches("1/2", &entry("Nope", JAN_12)));
    }

    #[test]
    fn a_date_term_written_in_a_title_still_matches_it() {
        let episode = entry("Remembering 9/11", JAN_2);
        assert!(matches("9/11", &episode));
        assert!(matches("re-run", &entry("A re-run of the show", JAN_2)));
    }

    #[test]
    fn the_calendar_date_is_the_feeds_own() {
        let late = entry("Late show", "Tue, 09 Sep 2025 23:30:00 -0500");
        assert!(matches("9/9/2025", &late));
        assert!(matches("sep 09", &late));
        assert!(!matches("9/10/2025", &late));
        assert!(!matches("sep 10", &late));
    }

    #[test]
    fn words_match_the_shown_date_and_the_full_month_name() {
        let episode = entry("Kickoff", JAN_2);
        assert!(matches("jan 02, 2026", &episode));
        assert!(matches("january", &episode));
        assert!(!matches("10:00", &episode));
        assert!(!matches("+0000", &episode));
        assert!(!matches("fri", &episode));
    }

    #[test]
    fn an_unparsable_date_is_searched_as_written() {
        let episode = entry("Old", "Spring 2019");
        assert!(matches("spring", &episode));
        assert!(!matches("since:2019", &episode));
    }

    #[test]
    fn range_terms_bound_the_publish_date() {
        let jan_2 = entry("A", JAN_2);
        let jan_12 = entry("B", JAN_12);
        let dec_12 = entry("C", DEC_12);
        assert!(matches("since:1/12/2026", &jan_12));
        assert!(!matches("since:1/12/2026", &jan_2));
        assert!(matches("until:1/12/2026", &jan_12));
        assert!(!matches("until:1/12/2026", &dec_12));
        assert!(!matches("after:1/12/2026", &jan_12));
        assert!(matches("after:1/12/2026", &dec_12));
        assert!(!matches("before:1/12/2026", &jan_12));
        assert!(matches("before:1/12/2026", &jan_2));
    }

    #[test]
    fn a_month_or_year_range_covers_the_whole_period() {
        let jan_12 = entry("B", JAN_12);
        let dec_12 = entry("C", DEC_12);
        assert!(!matches("after:2026-01", &jan_12));
        assert!(matches("since:2026-01", &jan_12));
        assert!(matches("until:2026-01", &jan_12));
        assert!(!matches("before:2026-01", &jan_12));
        assert!(matches("since:2026 until:2026", &dec_12));
        assert!(!matches("after:2026", &dec_12));
        assert!(matches("before:2027 jan", &jan_12));
        assert!(!matches("before:2027 jan", &dec_12));
    }

    #[test]
    fn an_unfinished_range_is_left_out_with_a_hint() {
        let jan_2 = entry("A", JAN_2);
        for query in ["after:", "after:20", "since:13/2026", "before:1/2"] {
            let parsed = EpisodeQuery::parse(query);
            assert!(parsed.notice().is_some(), "{query}");
            assert!(!parsed.has_range(), "{query}");
            assert!(parsed.matches(&jan_2), "{query}");
        }
        let parsed = EpisodeQuery::parse("After:2026");
        assert!(parsed.notice().is_none());
        assert!(!parsed.matches(&jan_2));
    }

    #[test]
    fn an_undated_episode_never_falls_in_a_range() {
        let undated = SearchEntry::new(&EpisodeInfo {
            title: "No date".to_string(),
            feed_index: 1,
            pub_date: None,
            description: String::new(),
        });
        assert!(matches("no date", &undated));
        assert!(!matches("since:2000", &undated));
    }

    #[test]
    fn invalid_dates_are_plain_words() {
        assert_eq!(parse_date_period("2/30/2026", false), None);
        assert_eq!(parse_date_period("13/2026", false), None);
        assert_eq!(parse_date_period("2026", false), None);
        assert_eq!(
            parse_date_period("2026", true),
            Some(DatePeriod::Year(2026))
        );
        assert_eq!(parse_date_period("1/2-2026", false), None);
        assert_eq!(parse_date_period("123/4", false), None);
    }

    fn numbered(title: &str, feed_index: usize, date: &str) -> SearchEntry {
        SearchEntry::new(&EpisodeInfo {
            title: title.to_string(),
            feed_index,
            pub_date: Some(date.to_string()),
            description: "Notes about GitHub tooling".to_string(),
        })
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap()
    }

    fn matches_today(query: &str, entry: &SearchEntry) -> bool {
        EpisodeQuery::parse_at(query, today()).matches(entry)
    }

    #[test]
    fn a_quoted_phrase_matches_as_a_whole() {
        let hit = entry("Revisiting GitHub Copilot", JAN_2);
        let miss = entry("Copilot on GitHub", JAN_2);
        assert!(matches("\"github copilot\"", &hit));
        assert!(!matches("\"github copilot\"", &miss));
        assert!(matches("copilot github", &miss));
        assert!(matches("\"revisiting git", &hit));
        assert!(matches("\"\"", &miss));
    }

    #[test]
    fn a_phrase_never_reads_as_a_date() {
        let episode = entry("Remembering 9/11", JAN_2);
        assert!(matches("\"9/11\"", &episode));
        assert!(!matches("\"1/2/2026\"", &episode));
    }

    #[test]
    fn a_minus_excludes_what_follows() {
        let copilot = entry("GitHub Copilot", JAN_2);
        let actions = entry("GitHub Actions", DEC_12);
        assert!(!matches("github -copilot", &copilot));
        assert!(matches("github -copilot", &actions));
        assert!(!matches("-\"github copilot\"", &copilot));
        assert!(matches("-\"github copilot\"", &actions));
        assert!(!matches("-12/2026", &actions));
        assert!(matches("-12/2026", &copilot));
        assert!(matches("github - copilot", &copilot));
        assert!(matches("github -", &actions));
    }

    #[test]
    fn a_range_cannot_be_excluded() {
        let parsed = EpisodeQuery::parse("-since:2025");
        assert!(!parsed.has_range());
        assert!(parsed.notice().unwrap().contains("cannot be excluded"));
    }

    #[test]
    fn episode_numbers_match_one_or_a_range() {
        let five_hundred = numbered("A", 500, JAN_2);
        let one_ten = numbered("B", 110, JAN_2);
        assert!(matches("#500", &five_hundred));
        assert!(!matches("#500", &one_ten));
        assert!(matches("#100-120", &one_ten));
        assert!(matches("#120-100", &one_ten));
        assert!(!matches("#100-120", &five_hundred));
        assert!(matches("-#500", &one_ten));
        assert!(!matches("#", &one_ten));
        assert!(matches(
            "#MacBookNeo",
            &entry("8GB of RAM #MacBookNeo", JAN_2)
        ));
    }

    #[test]
    fn relative_days_count_back_from_today() {
        let week_ago = entry("A", "Mon, 02 Nov 2026 09:00:00 +0000");
        let month_ago = entry("B", "Fri, 09 Oct 2026 09:00:00 +0000");
        let yesterday = entry("C", "Sun, 08 Nov 2026 09:00:00 +0000");
        assert!(matches_today("since:7d", &week_ago));
        assert!(!matches_today("since:6d", &week_ago));
        assert!(matches_today("since:1w", &week_ago));
        assert!(matches_today("since:1m", &month_ago));
        assert!(!matches_today("after:1m", &month_ago));
        assert!(matches_today("since:yesterday", &yesterday));
        assert!(!matches_today("since:today", &yesterday));
        assert!(matches_today("until:yesterday", &yesterday));
        assert!(matches_today("since:1y", &month_ago));
        assert!(!matches_today("before:2w", &week_ago));
    }

    #[test]
    fn a_relative_day_with_an_unknown_unit_is_explained() {
        let parsed = EpisodeQuery::parse_at("since:7x", today());
        assert!(!parsed.has_range());
        assert!(parsed.notice().unwrap().contains("since: needs a date"));
    }

    #[test]
    fn exclusions_ranges_and_phrases_combine() {
        let keep = numbered(
            "GitHub Copilot deep dive",
            548,
            "Mon, 02 Nov 2026 09:00:00 +0000",
        );
        let old = numbered(
            "GitHub Copilot intro",
            300,
            "Fri, 02 Jan 2026 09:00:00 +0000",
        );
        let other = numbered("GitHub Actions", 547, "Mon, 02 Nov 2026 09:00:00 +0000");
        let query = "\"github copilot\" -intro since:30d #500-600";
        assert!(matches_today(query, &keep));
        assert!(!matches_today(query, &old));
        assert!(!matches_today(query, &other));
    }

    #[test]
    fn only_included_words_and_phrases_are_highlighted() {
        let parsed = EpisodeQuery::parse("Copilot \"GitHub App\" -intro 11/2026 #500 since:2025");
        assert_eq!(parsed.highlights(), ["copilot", "github app"]);
    }
}
