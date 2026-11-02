//! Episode list search and jump-to-number.
//!
//! `App::episodes` and `App::episode_selected` always hold the whole feed, so
//! downloads and sorting never see the filter. The filter is `episode_visible`,
//! a list of indices into `episodes`, and `episode_list_state` points into that
//! list rather than into `episodes`.

use super::state::{App, Screen};
use crate::podcast::EpisodeInfo;

/// Which one-line prompt is open over the episode list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EpisodePrompt {
    Search,
    Jump,
}

fn matches_terms(episode: &EpisodeInfo, terms: &[String]) -> bool {
    if terms.is_empty() {
        return true;
    }
    let title = episode.title.to_lowercase();
    let description = episode.description.to_lowercase();
    let date = episode.pub_date.as_deref().unwrap_or("").to_lowercase();
    terms.iter().all(|term| {
        title.contains(term.as_str())
            || description.contains(term.as_str())
            || date.contains(term.as_str())
    })
}

impl App {
    /// Show every episode again and close any prompt; used when a list is loaded.
    pub fn reset_episode_view(&mut self) {
        self.episode_query.clear();
        self.episode_prompt = None;
        self.episode_jump_buffer.clear();
        self.episode_jump_error = None;
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
        let terms: Vec<String> = self
            .episode_query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        self.episode_visible = self
            .episodes
            .iter()
            .enumerate()
            .filter(|(_, episode)| matches_terms(episode, &terms))
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
        }
    }

    /// Close the search prompt and keep the filter it built.
    pub fn episode_search_commit(&mut self) {
        self.episode_prompt = None;
    }

    /// Close the search prompt and drop the filter.
    pub fn episode_search_cancel(&mut self) {
        self.episode_prompt = None;
        self.clear_episode_filter();
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
            Some(EpisodePrompt::Search) => self.refresh_episode_visible(),
            Some(EpisodePrompt::Jump) => {
                self.episode_jump_buffer.retain(|c| c.is_ascii_digit());
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
        let Ok(number) = typed.parse::<usize>() else {
            self.episode_jump_error = Some(format!("\"{typed}\" is not an episode number"));
            return;
        };
        let Some(index) = self
            .episodes
            .iter()
            .position(|episode| episode.feed_index == number)
        else {
            let newest = self
                .episodes
                .iter()
                .map(|episode| episode.feed_index)
                .max()
                .unwrap_or(0);
            self.episode_jump_error = Some(format!(
                "No episode #{number} in this feed (it has #1 to #{newest})"
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
