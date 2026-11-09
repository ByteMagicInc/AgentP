//! The episode search grammar, shared by the TUI's `/` search and
//! `agentp podcast episodes --search`.
//!
//! A query is whitespace-separated terms, and an episode matches when every
//! term does. A plain word matches anywhere in the title, the description, or
//! the publish date as the list shows it (`Sep 10, 2025`, plus the full month
//! name). A term written as a date, month first, matches episodes published in
//! that period: `9/10/2025`, `09-10-2025`, `2025-09-10`, `9/2025`, `2025-09`, or
//! `9/10` for that day in any year. `since:`, `until:`, `after:` and `before:`
//! take the same dates, or a bare year, and bound the publish date.

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
            text,
            date: parsed.map(|date| date.date_naive()),
            date_text,
        }
    }
}

#[derive(Clone, Debug)]
enum Term {
    Text(String),
    Date { period: DatePeriod, literal: String },
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

/// A parsed search query.
#[derive(Clone, Debug, Default)]
pub struct EpisodeQuery {
    terms: Vec<Term>,
    earliest: Option<NaiveDate>,
    latest: Option<NaiveDate>,
    notice: Option<String>,
}

impl EpisodeQuery {
    pub fn parse(query: &str) -> Self {
        let mut parsed = EpisodeQuery::default();
        for word in query.split_whitespace() {
            let lower = word.to_lowercase();
            if let Some((prefix, bound, value)) = BOUNDS.iter().find_map(|&(prefix, bound)| {
                lower
                    .strip_prefix(prefix)
                    .map(|value| (prefix, bound, value))
            }) {
                parsed.add_bound(prefix, bound, value);
                continue;
            }
            parsed.terms.push(match parse_date_period(&lower, false) {
                Some(period) => Term::Date {
                    period,
                    literal: lower,
                },
                None => Term::Text(lower),
            });
        }
        parsed
    }

    fn add_bound(&mut self, prefix: &str, bound: Bound, value: &str) {
        let Some((first, last)) = parse_date_period(value, true).and_then(|period| period.bounds())
        else {
            if self.notice.is_none() {
                self.notice = Some(format!(
                    "{prefix} needs a date, like {prefix}2025-09 or {prefix}9/10/2025"
                ));
            }
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

    /// A hint about a range term that is not a date yet, such as `after:20`
    /// while it is still being typed. Such a term is left out of the search.
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Whether the query names a publish-date range.
    pub fn has_range(&self) -> bool {
        self.earliest.is_some() || self.latest.is_some()
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
        self.terms.iter().all(|term| match term {
            Term::Text(word) => entry.text.contains(word) || entry.date_text.contains(word),
            Term::Date { period, literal } => {
                entry.date.is_some_and(|date| period.contains(date)) || entry.text.contains(literal)
            }
        })
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
}
