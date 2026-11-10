//! Episode list search and jump-to-number.
//!
//! `App::episodes` and `App::episode_selected` always hold the whole feed, so
//! downloads and sorting never see the filter. The filter is `episode_visible`,
//! a list of indices into `episodes`, and `episode_list_state` points into that
//! list rather than into `episodes`.

use super::state::{App, Screen};
use crate::podcast::{EpisodeQuery, SearchEntry};

/// Which one-line prompt is open over the episode list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EpisodePrompt {
    Search,
    Jump,
}

/// The longest episode number the go-to prompt accepts, in digits.
pub const MAX_JUMP_DIGITS: usize = 9;

impl App {
    /// Show every episode again and close any prompt; used when a list is loaded.
    pub fn reset_episode_view(&mut self) {
        self.episode_query.clear();
        self.episode_query_before.clear();
        self.episode_search_notice = None;
        self.episode_highlights.clear();
        self.episode_prompt = None;
        self.episode_jump_buffer.clear();
        self.episode_jump_error = None;
        self.episode_search_index = self.episodes.iter().map(SearchEntry::new).collect();
        self.episode_visible = (0..self.episodes.len()).collect();
    }

    /// Whether a search is narrowing the episode list.
    pub fn episode_filter_active(&self) -> bool {
        !self.episode_query.trim().is_empty()
    }

    /// Index into `episodes` of the episode under the cursor.
    pub fn current_episode_index(&self) -> Option<usize> {
        self.episode_list_state
            .selected()
            .and_then(|position| self.episode_visible.get(position).copied())
    }

    /// Recompute which episodes match the query, keeping the cursor on the
    /// same episode when it is still shown.
    pub fn refresh_episode_visible(&mut self) {
        let current = self.current_episode_index();
        let query = EpisodeQuery::parse(&self.episode_query);
        self.episode_search_notice = query.notice().map(str::to_string);
        self.episode_highlights = query.highlights().to_vec();
        self.episode_visible = self
            .episode_search_index
            .iter()
            .enumerate()
            .filter(|(_, entry)| query.matches(entry))
            .map(|(index, _)| index)
            .collect();
        let position = current
            .and_then(|index| {
                self.episode_visible
                    .iter()
                    .position(|&shown| shown == index)
            })
            .or(if self.episode_visible.is_empty() {
                None
            } else {
                Some(0)
            });
        self.episode_list_state.select(position);
    }

    /// Whether the episode list is loaded and non-empty, so search and go-to can act.
    pub(super) fn episode_prompt_available(&self) -> bool {
        self.screen == Screen::EpisodeSelect
            && !self.loading_episodes
            && self.episode_load_error.is_none()
            && !self.episodes.is_empty()
    }

    /// Open the search prompt, continuing from the current query.
    pub fn episode_search_open(&mut self) {
        if self.episode_prompt_available() {
            self.episode_prompt = Some(EpisodePrompt::Search);
            self.episode_query_before = self.episode_query.clone();
        }
    }

    /// Close the search prompt and keep the filter it built.
    pub fn episode_search_commit(&mut self) {
        self.episode_prompt = None;
    }

    /// Close the search prompt and put back the filter it opened with, which
    /// is no filter at all for a fresh search.
    pub fn episode_search_cancel(&mut self) {
        self.episode_prompt = None;
        self.episode_query = std::mem::take(&mut self.episode_query_before);
        self.refresh_episode_visible();
    }

    /// Add clipboard text to the search; line breaks become spaces, so a copied
    /// title still matches.
    pub fn episode_search_paste(&mut self, text: &str) {
        self.episode_query
            .extend(text.chars().map(|c| if c.is_control() { ' ' } else { c }));
        self.episode_prompt_edited();
    }

    /// Add clipboard text to the go-to prompt. Only a number, optionally led by
    /// `#`, is accepted; anything else, such as `-2` or `1 2`, is reported
    /// rather than reduced to its digits.
    pub fn episode_jump_paste(&mut self, text: &str) {
        let trimmed = text.trim();
        let digits = trimmed.strip_prefix('#').unwrap_or(trimmed);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            self.episode_jump_error = Some("Paste a plain episode number, such as 42".to_string());
            return;
        }
        if self.episode_jump_buffer.len() + digits.len() > MAX_JUMP_DIGITS {
            self.episode_jump_error = Some(format!(
                "Episode numbers are at most {MAX_JUMP_DIGITS} digits"
            ));
            return;
        }
        self.episode_jump_buffer.push_str(digits);
        self.episode_prompt_edited();
    }

    /// Show every episode again.
    pub fn clear_episode_filter(&mut self) {
        self.episode_query.clear();
        self.refresh_episode_visible();
    }

    /// Open the go-to-episode prompt, optionally seeded with its first digit.
    pub fn episode_jump_open(&mut self, first_digit: Option<char>) {
        if !self.episode_prompt_available() {
            return;
        }
        self.episode_prompt = Some(EpisodePrompt::Jump);
        self.episode_jump_buffer.clear();
        self.episode_jump_error = None;
        if let Some(digit) = first_digit.filter(char::is_ascii_digit) {
            self.episode_jump_buffer.push(digit);
        }
    }

    /// React to the open prompt's text having changed.
    pub fn episode_prompt_edited(&mut self) {
        if self.screen != Screen::EpisodeSelect {
            return;
        }
        match self.episode_prompt {
            Some(EpisodePrompt::Search) => {
                if self.episode_query.contains(char::is_control) {
                    self.episode_query = self
                        .episode_query
                        .chars()
                        .map(|c| if c.is_control() { ' ' } else { c })
                        .collect();
                }
                self.refresh_episode_visible();
            }
            Some(EpisodePrompt::Jump) => {
                self.episode_jump_buffer.retain(|c| c.is_ascii_digit());
                self.episode_jump_buffer.truncate(MAX_JUMP_DIGITS);
                self.episode_jump_error = None;
            }
            None => {}
        }
    }

    /// Move to the episode whose feed number was typed. A search that hides it
    /// is cleared; a number the feed lacks leaves the prompt open with a message.
    pub fn episode_jump_commit(&mut self) {
        let typed = self.episode_jump_buffer.trim().to_string();
        if typed.is_empty() {
            self.episode_jump_cancel();
            return;
        }
        let index = typed.parse::<usize>().ok().and_then(|number| {
            self.episodes
                .iter()
                .position(|episode| episode.feed_index == number)
        });
        let Some(index) = index else {
            let newest = self
                .episodes
                .iter()
                .map(|episode| episode.feed_index)
                .max()
                .unwrap_or(0);
            self.episode_jump_error = Some(format!(
                "No episode #{typed} in this feed (it has #1 to #{newest})"
            ));
            return;
        };
        if !self.episode_visible.contains(&index) {
            self.clear_episode_filter();
        }
        let position = self
            .episode_visible
            .iter()
            .position(|&shown| shown == index);
        self.episode_list_state.select(position);
        self.episode_jump_cancel();
    }

    /// Close the go-to-episode prompt without moving.
    pub fn episode_jump_cancel(&mut self) {
        self.episode_prompt = None;
        self.episode_jump_buffer.clear();
        self.episode_jump_error = None;
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app_one_podcast as test_app;
    use super::*;
    use crate::podcast::EpisodeInfo;

    fn episode(title: &str, feed_index: usize, description: &str) -> EpisodeInfo {
        EpisodeInfo {
            title: title.to_string(),
            feed_index,
            pub_date: Some("Wed, 10 Sep 2025 06:00:00 +0000".to_string()),
            description: description.to_string(),
        }
    }

    fn episodes_app() -> App {
        let mut app = test_app();
        app.screen = Screen::EpisodeSelect;
        app.set_episodes(vec![
            episode("Rust in production", 3, "How a team adopted Rust"),
            episode(
                "Gardening basics",
                2,
                "Tomatoes and soil, plus a tip on RUST on tools",
            ),
            episode("Cooking with fire", 1, "Grills"),
        ]);
        app
    }

    fn type_query(app: &mut App, text: &str) {
        app.episode_search_open();
        app.episode_query.push_str(text);
        app.episode_prompt_edited();
    }

    fn visible_titles(app: &App) -> Vec<&str> {
        app.episode_visible
            .iter()
            .map(|&i| app.episodes[i].title.as_str())
            .collect()
    }

    #[test]
    fn search_matches_title_and_description_ignoring_case() {
        let mut app = episodes_app();
        type_query(&mut app, "rust");
        assert_eq!(
            visible_titles(&app),
            vec!["Rust in production", "Gardening basics"]
        );
    }

    #[test]
    fn every_term_must_match_somewhere() {
        let mut app = episodes_app();
        type_query(&mut app, "rust tomatoes");
        assert_eq!(visible_titles(&app), vec!["Gardening basics"]);
    }

    #[test]
    fn search_matches_the_publish_date() {
        let mut app = episodes_app();
        type_query(&mut app, "sep 2025");
        assert_eq!(app.episode_visible.len(), 3);
        app.episode_query = "2024".into();
        app.episode_prompt_edited();
        assert!(app.episode_visible.is_empty());
        assert_eq!(app.episode_list_state.selected(), None);
    }

    fn dated_app() -> App {
        let mut app = test_app();
        app.screen = Screen::EpisodeSelect;
        app.set_episodes(vec![
            EpisodeInfo {
                pub_date: Some("Mon, 02 Nov 2026 09:00:00 +0000".to_string()),
                ..episode("November show", 3, "")
            },
            EpisodeInfo {
                pub_date: Some("Sat, 12 Dec 2026 09:00:00 +0000".to_string()),
                ..episode("December show", 2, "")
            },
            EpisodeInfo {
                pub_date: Some("Mon, 12 Jan 2026 09:00:00 +0000".to_string()),
                ..episode("January show", 1, "")
            },
        ]);
        app
    }

    #[test]
    fn a_typed_date_picks_out_its_episode() {
        let mut app = dated_app();
        type_query(&mut app, "11-02-2026");
        assert_eq!(visible_titles(&app), vec!["November show"]);
        app.episode_query = "12-2026".into();
        app.episode_prompt_edited();
        assert_eq!(visible_titles(&app), vec!["December show"]);
        app.episode_query = "show since:2026-11".into();
        app.episode_prompt_edited();
        assert_eq!(visible_titles(&app), vec!["November show", "December show"]);
        assert!(app.episode_search_notice.is_none());
    }

    #[test]
    fn an_unfinished_range_keeps_the_list_and_explains_itself() {
        let mut app = dated_app();
        type_query(&mut app, "before:20");
        assert_eq!(app.episode_visible.len(), 3);
        assert!(
            app.episode_search_notice
                .as_deref()
                .unwrap()
                .contains("before:")
        );
        app.episode_query.push_str("26");
        app.episode_prompt_edited();
        assert!(app.episode_search_notice.is_none());
        assert!(app.episode_visible.is_empty());
    }

    #[test]
    fn escape_puts_back_the_filter_the_search_opened_with() {
        let mut app = episodes_app();
        type_query(&mut app, "rust");
        app.episode_search_commit();
        type_query(&mut app, " tomatoes");
        assert_eq!(visible_titles(&app), vec!["Gardening basics"]);
        app.episode_search_cancel();
        assert_eq!(app.episode_query, "rust");
        assert_eq!(app.episode_visible.len(), 2);

        app.clear_episode_filter();
        type_query(&mut app, "cooking");
        app.episode_search_cancel();
        assert!(!app.episode_filter_active());
        assert_eq!(app.episode_visible.len(), 3);
    }

    #[test]
    fn pasted_line_breaks_become_spaces() {
        let mut app = episodes_app();
        app.episode_search_open();
        app.episode_search_paste("rust\nproduction\r\n");
        assert_eq!(app.episode_query, "rust production  ");
        assert_eq!(visible_titles(&app), vec!["Rust in production"]);
    }

    #[test]
    fn a_pasted_episode_number_must_be_a_plain_number() {
        let mut app = episodes_app();
        app.episode_jump_open(None);
        for bad in ["-2", "1 2", "two", ""] {
            app.episode_jump_paste(bad);
            assert_eq!(app.episode_jump_buffer, "", "{bad}");
            assert!(app.episode_jump_error.is_some(), "{bad}");
        }
        app.episode_jump_paste(" #2\n");
        assert_eq!(app.episode_jump_buffer, "2");
        assert!(app.episode_jump_error.is_none());
        app.episode_jump_commit();
        assert_eq!(app.current_episode_index(), Some(1));
    }

    #[test]
    fn the_go_to_number_is_capped_in_length() {
        let mut app = episodes_app();
        app.episode_jump_open(Some('9'));
        app.episode_jump_buffer.push_str(&"9".repeat(20));
        app.episode_prompt_edited();
        assert_eq!(app.episode_jump_buffer.len(), MAX_JUMP_DIGITS);
        app.episode_jump_paste("1");
        assert_eq!(app.episode_jump_buffer.len(), MAX_JUMP_DIGITS);
        assert!(
            app.episode_jump_error
                .as_deref()
                .unwrap()
                .contains("digits")
        );
    }

    #[test]
    fn a_number_too_large_for_the_feed_says_it_is_missing() {
        let mut app = episodes_app();
        app.episode_jump_open(Some('9'));
        app.episode_jump_buffer.push_str(&"9".repeat(30));
        app.episode_jump_commit();
        let error = app.episode_jump_error.as_deref().unwrap();
        assert!(error.starts_with("No episode #999"), "{error}");
        assert!(error.ends_with("#1 to #3)"), "{error}");
    }

    #[test]
    fn sorting_keeps_the_cursor_and_the_search_index_in_step() {
        let mut app = episodes_app();
        app.episode_list_state.select(Some(1));
        app.toggle_sort_order();
        assert_eq!(
            app.current_episode_index()
                .map(|i| app.episodes[i].title.as_str()),
            Some("Gardening basics")
        );
        type_query(&mut app, "cooking");
        assert_eq!(visible_titles(&app), vec!["Cooking with fire"]);
        let rebuilt: Vec<_> = app
            .episodes
            .iter()
            .map(|e| format!("{:?}", SearchEntry::new(e)))
            .collect();
        let kept: Vec<_> = app
            .episode_search_index
            .iter()
            .map(|e| format!("{e:?}"))
            .collect();
        assert_eq!(kept, rebuilt);
    }

    #[test]
    fn selection_survives_filtering_and_unfiltering() {
        let mut app = episodes_app();
        app.episode_list_state.select(Some(1));
        app.toggle_current_episode();
        type_query(&mut app, "cooking");
        assert_eq!(visible_titles(&app), vec!["Cooking with fire"]);
        app.toggle_current_episode();
        assert_eq!(app.selected_feed_indices(), vec![2, 1]);
        app.episode_search_cancel();
        assert_eq!(app.episode_visible.len(), 3);
        assert_eq!(app.selected_feed_indices(), vec![2, 1]);
    }

    #[test]
    fn the_cursor_stays_on_its_episode_while_the_filter_narrows() {
        let mut app = episodes_app();
        app.episode_list_state.select(Some(1));
        type_query(&mut app, "garden");
        assert_eq!(app.current_episode_index(), Some(1));
        assert_eq!(app.episode_list_state.selected(), Some(0));
    }

    #[test]
    fn select_all_and_navigation_stay_inside_the_filter() {
        let mut app = episodes_app();
        type_query(&mut app, "rust");
        app.episode_move_down();
        app.episode_move_down();
        assert_eq!(app.episode_list_state.selected(), Some(1));
        app.select_all_episodes();
        assert_eq!(app.selected_feed_indices(), vec![3, 2]);
        app.select_all_episodes();
        assert!(app.selected_feed_indices().is_empty());
    }

    #[test]
    fn sorting_keeps_the_filter() {
        let mut app = episodes_app();
        type_query(&mut app, "rust");
        app.toggle_sort_order();
        assert_eq!(
            visible_titles(&app),
            vec!["Gardening basics", "Rust in production"]
        );
    }

    #[test]
    fn jump_goes_by_feed_number_under_either_sort_order() {
        let mut app = episodes_app();
        app.episode_jump_open(Some('2'));
        app.episode_jump_commit();
        assert_eq!(
            app.current_episode_index()
                .map(|i| app.episodes[i].feed_index),
            Some(2)
        );
        assert!(app.episode_prompt.is_none());

        app.toggle_sort_order();
        app.episode_jump_open(Some('3'));
        app.episode_jump_commit();
        assert_eq!(
            app.current_episode_index()
                .map(|i| app.episodes[i].feed_index),
            Some(3)
        );
    }

    #[test]
    fn jump_clears_a_filter_that_hides_the_episode() {
        let mut app = episodes_app();
        type_query(&mut app, "rust");
        app.episode_search_commit();
        app.episode_jump_open(Some('1'));
        app.episode_jump_commit();
        assert!(!app.episode_filter_active());
        assert_eq!(app.current_episode_index(), Some(2));
    }

    #[test]
    fn jump_to_a_missing_number_keeps_the_prompt_and_the_cursor() {
        let mut app = episodes_app();
        app.episode_list_state.select(Some(1));
        app.episode_jump_open(Some('9'));
        app.episode_jump_buffer.push('9');
        app.episode_jump_commit();
        assert_eq!(app.episode_prompt, Some(EpisodePrompt::Jump));
        assert_eq!(app.episode_list_state.selected(), Some(1));
        assert!(app.episode_jump_error.as_deref().unwrap().contains("#99"));
        app.episode_jump_buffer.push('x');
        app.episode_prompt_edited();
        assert_eq!(app.episode_jump_buffer, "99");
        assert!(app.episode_jump_error.is_none());
    }

    #[test]
    fn prompts_do_not_open_while_the_list_is_loading() {
        let mut app = episodes_app();
        app.loading_episodes = true;
        app.episode_search_open();
        app.episode_jump_open(Some('1'));
        assert!(app.episode_prompt.is_none());
    }
}
