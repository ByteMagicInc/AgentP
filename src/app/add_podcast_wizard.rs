//! Multistep wizard for adding a new podcast.

use anyhow::{Result, bail};
use ratatui::widgets::ListState;

use crate::podcast::{Podcast, sanitize_filename, save_config};

use super::state::*;

/// The type of input a wizard step expects.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WizardStepKind {
    Text,
    Bool,
    Usize,
}

/// Definition of a single step in the add-podcast wizard.
pub struct WizardStep {
    pub label: &'static str,
    pub hint: &'static str,
    pub required: bool,
    pub kind: WizardStepKind,
}

/// Ordered list of all wizard steps for adding a podcast.
pub const WIZARD_STEPS: &[WizardStep] = &[
    WizardStep {
        label: "Feed URL",
        hint: "Paste the RSS feed URL here — press Enter to continue",
        required: true,
        kind: WizardStepKind::Text,
    },
    WizardStep {
        label: "Name",
        hint: "What do you want to call this podcast?",
        required: true,
        kind: WizardStepKind::Text,
    },
    WizardStep {
        label: "Album Name",
        hint: "Used as the download folder and album tag",
        required: true,
        kind: WizardStepKind::Text,
    },
    WizardStep {
        label: "Artist",
        hint: "Only written to MP3s when tag writing is on — leave blank to skip",
        required: false,
        kind: WizardStepKind::Text,
    },
    WizardStep {
        label: "Write Tags",
        hint: "Write ID3 metadata (album, artist, title) into downloaded MP3s",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Remove Images",
        hint: "Remove embedded cover art from downloaded audio files",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Remove Existing Tags",
        hint: "Remove the source file's existing ID3 tag before writing — turn on if the source has huge embedded metadata that hides the title in your player",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Write Album Tag",
        hint: "Write the album name into the MP3 album tag",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Write Artist Tag",
        hint: "Write the artist name into the MP3 artist tag",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Rename Files",
        hint: "Rename downloaded files to match the episode title",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Write Title Tag",
        hint: "Write the episode title into the MP3 title tag",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Append Number To Title",
        hint: "Prepend episode number to the title tag (e.g. 001 Episode)",
        required: false,
        kind: WizardStepKind::Bool,
    },
    WizardStep {
        label: "Leading Zeros Amount",
        hint: "Number of zeros before the episode number — e.g. 3 gives 0001, 0002 (0 = off)",
        required: false,
        kind: WizardStepKind::Usize,
    },
    WizardStep {
        label: "User-Agent",
        hint: "Custom User-Agent for this podcast's downloads — leave blank to use the default (AgentP/<version>)",
        required: false,
        kind: WizardStepKind::Text,
    },
];

/// UI state for the add-podcast wizard.
pub struct AddPodcastState {
    pub step: usize,
    pub podcast: Podcast,
    pub text_buffer: String,
    pub loading_feed_info: bool,
    /// true while the Manual / Prepopulate choice overlay is shown.
    pub mode_select: bool,
}

impl App {
    /// Start the add-podcast wizard with defaults from the template.
    pub fn enter_add_podcast(&mut self) {
        let mut podcast = self.config.default_podcast.clone();
        podcast.name = String::new();
        podcast.feed_url = String::new();
        self.add_podcast = AddPodcastState {
            step: 0,
            podcast,
            text_buffer: String::new(),
            loading_feed_info: false,
            mode_select: false,
        };
        self.screen = Screen::AddPodcast;
    }

    /// Whether a wizard step is visible based on the current toggle state.
    pub fn add_podcast_step_is_visible(&self, step: usize) -> bool {
        let p = &self.add_podcast.podcast;
        match step {
            0..=5 | 13 => true,
            6..=10 => p.overwrite_tags,
            11 => p.overwrite_tags && p.overwrite_title,
            12 => p.overwrite_tags && p.overwrite_title && p.append_number_to_title,
            _ => false,
        }
    }

    /// Whether the current step is the last visible one, so Enter saves.
    pub fn add_podcast_on_last_visible_step(&self) -> bool {
        (self.add_podcast.step + 1..WIZARD_STEPS.len())
            .all(|step| !self.add_podcast_step_is_visible(step))
    }

    fn add_podcast_commit_text(&mut self) {
        let buf = self.add_podcast.text_buffer.clone();
        match self.add_podcast.step {
            0 => self.add_podcast.podcast.feed_url = buf,
            1 => self.add_podcast.podcast.name = buf,
            2 => self.add_podcast.podcast.album_name = buf,
            3 => self.add_podcast.podcast.artist = buf,
            13 => self.add_podcast.podcast.user_agent = buf,
            _ => {}
        }
    }

    fn add_podcast_load_buffer(&mut self) {
        self.add_podcast.text_buffer = self.add_podcast.podcast.field_text(self.add_podcast.step);
    }

    /// Validate and commit the Feed URL, then show the mode-select overlay.
    /// Returns `false` if the URL field is empty (stay on step 0).
    pub fn add_podcast_enter_mode_select(&mut self) -> bool {
        if self.add_podcast.text_buffer.trim().is_empty() {
            return false;
        }
        self.add_podcast.podcast.feed_url = self.add_podcast.text_buffer.clone();
        self.add_podcast.mode_select = true;
        true
    }

    /// User chose "Manual" — close mode-select overlay and advance to step 1.
    pub fn add_podcast_choose_manual(&mut self) {
        self.add_podcast.mode_select = false;
        self.add_podcast.step = 1;
        self.add_podcast_load_buffer();
    }

    /// User pressed Esc on the mode-select overlay — go back to editing the URL.
    pub fn add_podcast_cancel_mode_select(&mut self) {
        self.add_podcast.mode_select = false;
        self.add_podcast.text_buffer = self.add_podcast.podcast.feed_url.clone();
    }

    /// Cancel an in-progress feed fetch and return to the mode-select overlay.
    pub fn add_podcast_cancel_fetch(&mut self) {
        self.add_podcast.loading_feed_info = false;
        self.add_podcast.mode_select = true;
    }

    /// Called when feed metadata arrives from the background task.
    /// Pre-populates empty fields and advances to step 1.
    pub fn add_podcast_feed_loaded(&mut self, title: String, author: String) {
        self.add_podcast.loading_feed_info = false;
        let p = &mut self.add_podcast.podcast;
        if p.name.is_empty() {
            p.name = title.clone();
        }
        if p.album_name.is_empty() {
            p.album_name = sanitize_filename(&title);
        }
        if p.artist.is_empty() && !author.is_empty() {
            p.artist = author;
        }
        self.add_podcast.step = 1;
        self.add_podcast_load_buffer();
    }

    /// Called when the feed metadata fetch fails — return to mode-select.
    pub fn add_podcast_feed_failed(&mut self) {
        self.add_podcast_cancel_fetch();
    }

    /// Advance to the next wizard step; returns `true` when all steps are complete.
    pub fn add_podcast_next(&mut self) -> bool {
        let step = self.add_podcast.step;
        if WIZARD_STEPS[step].kind == WizardStepKind::Text {
            if WIZARD_STEPS[step].required && self.add_podcast.text_buffer.trim().is_empty() {
                return false;
            }
            self.add_podcast_commit_text();
        }
        let next = (step + 1..WIZARD_STEPS.len())
            .find(|next_step| self.add_podcast_step_is_visible(*next_step));
        let Some(next) = next else {
            return true;
        };
        self.add_podcast.step = next;
        self.add_podcast_load_buffer();
        false
    }

    /// Go back to the previous wizard step.
    pub fn add_podcast_prev(&mut self) {
        if self.add_podcast.step > 0 {
            if WIZARD_STEPS[self.add_podcast.step].kind == WizardStepKind::Text {
                self.add_podcast_commit_text();
            }
            self.add_podcast.step = (0..self.add_podcast.step)
                .rev()
                .find(|previous_step| self.add_podcast_step_is_visible(*previous_step))
                .unwrap_or(0);
            self.add_podcast_load_buffer();
        }
    }

    /// Toggle the boolean value for the current wizard step.
    pub fn add_podcast_toggle_bool(&mut self) {
        let p = &mut self.add_podcast.podcast;
        match self.add_podcast.step {
            4 => p.overwrite_tags = !p.overwrite_tags,
            5 => p.remove_images = !p.remove_images,
            6 => p.remove_existing_tags = !p.remove_existing_tags,
            7 => p.overwrite_album_name = !p.overwrite_album_name,
            8 => p.overwrite_artist = !p.overwrite_artist,
            9 => p.overwrite_file_name = !p.overwrite_file_name,
            10 => p.overwrite_title = !p.overwrite_title,
            11 => p.append_number_to_title = !p.append_number_to_title,
            _ => {}
        }
    }

    /// Adjust the leading-zeros value by the given delta.
    pub fn add_podcast_adjust_usize(&mut self, delta: i64) {
        if self.add_podcast.step == 12 {
            let current = self.add_podcast.podcast.leading_zeros_amount as i64;
            self.add_podcast.podcast.leading_zeros_amount = (current + delta).max(0) as usize;
        }
    }

    /// Save the new podcast to config and return to the podcast list.
    pub fn add_podcast_save(&mut self) -> Result<()> {
        if self.add_podcast.podcast.name.is_empty() {
            bail!("Podcast name is required");
        }
        self.config.podcasts.push(self.add_podcast.podcast.clone());
        save_config(&self.config)?;
        let count = self.config.podcasts.len();
        self.latest_episodes = vec![None; count];
        let mut ls = ListState::default();
        ls.select(Some(count.saturating_sub(1)));
        self.podcast_list_state = ls;
        self.screen = Screen::PodcastList;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app_one_podcast as test_app;
    use super::*;

    #[test]
    fn wizard_has_fourteen_steps() {
        assert_eq!(WIZARD_STEPS.len(), 14);
    }

    #[test]
    fn wizard_steps_match_field_index_constants() {
        use crate::app::{PODCAST_FIELD_BOOL_START, PODCAST_FIELD_COUNT, PODCAST_FIELD_USIZE};
        assert_eq!(WIZARD_STEPS.len(), PODCAST_FIELD_COUNT);
        assert_eq!(
            WIZARD_STEPS
                .iter()
                .map(|step| step.label)
                .collect::<Vec<_>>(),
            vec![
                "Feed URL",
                "Name",
                "Album Name",
                "Artist",
                "Write Tags",
                "Remove Images",
                "Remove Existing Tags",
                "Write Album Tag",
                "Write Artist Tag",
                "Rename Files",
                "Write Title Tag",
                "Append Number To Title",
                "Leading Zeros Amount",
                "User-Agent",
            ]
        );
        assert!(
            WIZARD_STEPS[..PODCAST_FIELD_BOOL_START]
                .iter()
                .all(|ws| ws.kind == WizardStepKind::Text),
            "the identity fields before PODCAST_FIELD_BOOL_START must all be Text"
        );
        assert!(
            WIZARD_STEPS[PODCAST_FIELD_BOOL_START..PODCAST_FIELD_USIZE]
                .iter()
                .all(|ws| ws.kind == WizardStepKind::Bool),
            "fields in the bool range must all be Bool"
        );
        assert_eq!(
            WIZARD_STEPS[PODCAST_FIELD_USIZE].kind,
            WizardStepKind::Usize
        );
        let last = PODCAST_FIELD_COUNT - 1;
        assert_eq!(
            WIZARD_STEPS[last].kind,
            WizardStepKind::Text,
            "user_agent is the last field and is a Text field"
        );
    }

    #[test]
    fn field_accessors_read_user_agent_at_last_index() {
        let p = Podcast {
            user_agent: "Custom/1.0".into(),
            overwrite_tags: false,
            remove_images: true,
            ..Podcast::default()
        };
        assert_eq!(p.field_text(13), "Custom/1.0");
        assert!(p.field_bool(5), "remove_images must read true at index 5");
        assert!(
            !p.field_bool(4),
            "index 4 is overwrite_tags (false here), not remove_images"
        );
    }

    #[test]
    fn first_step_is_required_feed_url() {
        assert!(WIZARD_STEPS[0].required);
        assert_eq!(WIZARD_STEPS[0].kind, WizardStepKind::Text);
        assert_eq!(WIZARD_STEPS[0].label, "Feed URL");
    }

    #[test]
    fn second_step_is_required_name() {
        assert!(WIZARD_STEPS[1].required);
        assert_eq!(WIZARD_STEPS[1].kind, WizardStepKind::Text);
        assert_eq!(WIZARD_STEPS[1].label, "Name");
    }

    #[test]
    fn enter_mode_select_fails_on_empty_url() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.text_buffer.clear();
        assert!(!app.add_podcast_enter_mode_select());
        assert!(!app.add_podcast.mode_select);
    }

    #[test]
    fn enter_mode_select_commits_url_and_sets_flag() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.text_buffer = "https://example.com/feed.rss".into();
        assert!(app.add_podcast_enter_mode_select());
        assert!(app.add_podcast.mode_select);
        assert_eq!(
            app.add_podcast.podcast.feed_url,
            "https://example.com/feed.rss"
        );
    }

    #[test]
    fn choose_manual_advances_to_step_1() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.podcast.feed_url = "https://example.com/feed.rss".into();
        app.add_podcast.mode_select = true;
        app.add_podcast_choose_manual();
        assert!(!app.add_podcast.mode_select);
        assert_eq!(app.add_podcast.step, 1);
    }

    #[test]
    fn feed_loaded_prefills_and_advances() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.podcast.feed_url = "https://example.com/feed.rss".into();
        app.add_podcast_feed_loaded("The Show".into(), "Host Name".into());
        assert_eq!(app.add_podcast.step, 1);
        assert_eq!(app.add_podcast.podcast.name, "The Show");
        assert_eq!(app.add_podcast.podcast.album_name, "The Show");
        assert_eq!(app.add_podcast.podcast.artist, "Host Name");
        assert_eq!(app.add_podcast.text_buffer, "The Show");
    }

    #[test]
    fn feed_loaded_does_not_overwrite_existing_name() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.podcast.name = "My Custom Name".into();
        app.add_podcast_feed_loaded("Feed Title".into(), "Author".into());
        assert_eq!(app.add_podcast.podcast.name, "My Custom Name");
    }

    #[test]
    fn prev_at_zero_stays() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast_prev();
        assert_eq!(app.add_podcast.step, 0);
    }

    #[test]
    fn user_agent_remains_reachable_when_tag_steps_are_hidden() {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.podcast.overwrite_tags = false;
        app.add_podcast.step = 5;

        assert!(!app.add_podcast_next());
        assert_eq!(app.add_podcast.step, 13);

        app.add_podcast.text_buffer = "Custom/1.0".into();
        assert!(app.add_podcast_next());
        assert_eq!(app.add_podcast.podcast.user_agent, "Custom/1.0");

        app.add_podcast_prev();
        assert_eq!(app.add_podcast.step, 5);
    }

    #[test]
    fn user_agent_follows_the_last_enabled_tag_step() {
        for (overwrite_title, append_number_to_title, last_tag_step) in
            [(false, true, 10), (true, false, 11), (true, true, 12)]
        {
            let mut app = test_app();
            app.enter_add_podcast();
            app.add_podcast.podcast.overwrite_tags = true;
            app.add_podcast.podcast.overwrite_title = overwrite_title;
            app.add_podcast.podcast.append_number_to_title = append_number_to_title;
            app.add_podcast.step = last_tag_step;

            assert!(!app.add_podcast_next());
            assert_eq!(app.add_podcast.step, 13);
        }
    }
}
