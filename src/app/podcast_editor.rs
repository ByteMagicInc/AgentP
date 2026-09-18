//! Podcast editor: field navigation, text/bool/usize editing, save, and discard.

use anyhow::Result;
use ratatui::widgets::ListState;

use crate::podcast::{Podcast, save_config};

use super::state::*;

impl App {
    pub fn enter_edit_podcast_select(&mut self) {
        let mut ls = ListState::default();
        if !self.config.podcasts.is_empty() {
            ls.select(Some(0));
        }
        self.edit_podcast_select_state = ls;
        self.screen = Screen::EditPodcastSelect;
    }

    /// Move the cursor up in the pick-a-podcast-to-edit list.
    pub fn edit_select_move_up(&mut self) {
        let i = self.edit_podcast_select_state.selected().unwrap_or(0);
        if i > 0 {
            self.edit_podcast_select_state.select(Some(i - 1));
        }
    }

    /// Move the cursor down in the pick-a-podcast-to-edit list.
    pub fn edit_select_move_down(&mut self) {
        let i = self.edit_podcast_select_state.selected().unwrap_or(0);
        if i < self.config.podcasts.len().saturating_sub(1) {
            self.edit_podcast_select_state.select(Some(i + 1));
        }
    }

    pub fn enter_podcast_editor_from_edit_select(&mut self, idx: usize) {
        self.enter_podcast_editor(
            PodcastEditorTarget::Existing(idx),
            Screen::EditPodcastSelect,
        );
    }

    pub fn move_podcast_up(&mut self) -> Result<()> {
        let i = self.selected_podcast_index();
        if i > 0 && i < self.config.podcasts.len() {
            self.config.podcasts.swap(i, i - 1);
            if i < self.latest_episodes.len() {
                self.latest_episodes.swap(i, i - 1);
            }
            self.podcast_list_state.select(Some(i - 1));
            save_config(&self.config)?;
        }
        Ok(())
    }

    pub fn move_podcast_down(&mut self) -> Result<()> {
        let i = self.selected_podcast_index();
        if i + 1 < self.config.podcasts.len() {
            self.config.podcasts.swap(i, i + 1);
            if i + 1 < self.latest_episodes.len() {
                self.latest_episodes.swap(i, i + 1);
            }
            self.podcast_list_state.select(Some(i + 1));
            save_config(&self.config)?;
        }
        Ok(())
    }

    /// Delete the pending podcast, persisting before mutating in-memory state.
    ///
    /// The candidate config is saved first and adopted only on success, so a
    /// failed write leaves both the list and the pending marker untouched and
    /// the confirmation stays open to retry. Ordering matters: removing before
    /// saving would let a retry delete whichever podcast had shifted into that
    /// index.
    pub fn confirm_delete_podcast(&mut self) -> Result<()> {
        if let Some(i) = self.podcast_delete_pending
            && i < self.config.podcasts.len()
        {
            let mut next = self.config.clone();
            next.podcasts.remove(i);
            save_config(&next)?;
            self.config = next;
            let new_len = self.config.podcasts.len();
            self.latest_episodes = vec![None; new_len];
            let mut s = ListState::default();
            if new_len > 0 {
                s.select(Some(i.min(new_len - 1)));
            }
            self.podcast_list_state = s;
        }
        self.podcast_delete_pending = None;
        Ok(())
    }

    pub fn enter_podcast_editor(&mut self, target: PodcastEditorTarget, back_to: Screen) {
        let working_copy = match &target {
            PodcastEditorTarget::Existing(i) => {
                if *i < self.config.podcasts.len() {
                    self.config.podcasts[*i].clone()
                } else {
                    Podcast::default()
                }
            }
            PodcastEditorTarget::Template => self.config.default_podcast.clone(),
        };
        let first_field = match &target {
            PodcastEditorTarget::Template => 4,
            _ => 0,
        };
        let mut ls = ListState::default();
        ls.select(Some(first_field));
        self.podcast_editor = PodcastEditorState {
            target,
            back_to,
            working_copy,
            list_state: ls,
            edit_mode: ConfigEditMode::Navigate,
            text_buffer: String::new(),
            dirty: false,
            show_confirm_discard: false,
            quit_pending: false,
        };
        self.screen = Screen::EditPodcast;
    }

    pub fn enter_podcast_editor_existing(&mut self, idx: usize) {
        self.enter_podcast_editor(PodcastEditorTarget::Existing(idx), Screen::PodcastList);
    }

    pub fn enter_template_editor(&mut self) {
        self.enter_podcast_editor(PodcastEditorTarget::Template, Screen::Config);
    }

    pub fn podcast_editor_first_field(&self) -> usize {
        match self.podcast_editor.target {
            PodcastEditorTarget::Template => 4,
            _ => 0,
        }
    }

    pub fn podcast_editor_move_up(&mut self) {
        let i = self.podcast_editor.list_state.selected().unwrap_or(0);
        let min = self.podcast_editor_first_field();
        if i > min {
            self.podcast_editor.list_state.select(Some(i - 1));
        }
    }

    pub fn podcast_editor_move_down(&mut self) {
        let i = self.podcast_editor.list_state.selected().unwrap_or(0);
        if i < PODCAST_FIELD_COUNT.saturating_sub(1) {
            self.podcast_editor.list_state.select(Some(i + 1));
        }
    }

    /// Act on the highlighted field: toggle a bool, or start editing text.
    ///
    /// The numeric field has no activation — it changes with `+` and `-`.
    pub fn podcast_editor_activate_field(&mut self) {
        let field = self.podcast_editor.list_state.selected().unwrap_or(0);
        if (PODCAST_FIELD_BOOL_START..PODCAST_FIELD_USIZE).contains(&field) {
            self.podcast_editor_toggle_bool();
        } else if field != PODCAST_FIELD_USIZE {
            self.podcast_editor_enter_edit_mode();
        }
    }

    pub fn podcast_editor_enter_edit_mode(&mut self) {
        let fi = self.podcast_editor.list_state.selected().unwrap_or(0);
        let value = match fi {
            0 => self.podcast_editor.working_copy.feed_url.clone(),
            1 => self.podcast_editor.working_copy.name.clone(),
            2 => self.podcast_editor.working_copy.album_name.clone(),
            3 => self.podcast_editor.working_copy.artist.clone(),
            13 => self.podcast_editor.working_copy.user_agent.clone(),
            _ => return,
        };
        self.podcast_editor.text_buffer = value;
        self.podcast_editor.edit_mode = ConfigEditMode::EditingText;
    }

    pub fn podcast_editor_commit_edit(&mut self) {
        let fi = self.podcast_editor.list_state.selected().unwrap_or(0);
        let buf = self.podcast_editor.text_buffer.clone();
        match fi {
            0 => self.podcast_editor.working_copy.feed_url = buf,
            1 => self.podcast_editor.working_copy.name = buf,
            2 => self.podcast_editor.working_copy.album_name = buf,
            3 => self.podcast_editor.working_copy.artist = buf,
            13 => self.podcast_editor.working_copy.user_agent = buf,
            _ => {}
        }
        self.podcast_editor.dirty = true;
        self.podcast_editor.text_buffer.clear();
        self.podcast_editor.edit_mode = ConfigEditMode::Navigate;
    }

    pub fn podcast_editor_cancel_edit(&mut self) {
        self.podcast_editor.text_buffer.clear();
        self.podcast_editor.edit_mode = ConfigEditMode::Navigate;
    }

    pub fn podcast_editor_toggle_bool(&mut self) {
        let fi = self.podcast_editor.list_state.selected().unwrap_or(0);
        let p = &mut self.podcast_editor.working_copy;
        let changed = match fi {
            4 => {
                p.overwrite_tags = !p.overwrite_tags;
                true
            }
            5 => {
                p.remove_images = !p.remove_images;
                true
            }
            6 => {
                p.remove_existing_tags = !p.remove_existing_tags;
                true
            }
            7 => {
                p.overwrite_album_name = !p.overwrite_album_name;
                true
            }
            8 => {
                p.overwrite_artist = !p.overwrite_artist;
                true
            }
            9 => {
                p.overwrite_file_name = !p.overwrite_file_name;
                true
            }
            10 => {
                p.overwrite_title = !p.overwrite_title;
                true
            }
            11 => {
                p.append_number_to_title = !p.append_number_to_title;
                true
            }
            _ => false,
        };
        if changed {
            self.podcast_editor.dirty = true;
        }
    }

    pub fn podcast_editor_increment_usize(&mut self, delta: i64) {
        let fi = self.podcast_editor.list_state.selected().unwrap_or(0);
        if fi == PODCAST_FIELD_USIZE {
            let current = self.podcast_editor.working_copy.leading_zeros_amount as i64;
            let new_val = (current + delta).max(0) as usize;
            self.podcast_editor.working_copy.leading_zeros_amount = new_val;
            self.podcast_editor.dirty = true;
        }
    }

    pub fn podcast_editor_restore_default(&mut self) {
        let fi = self.podcast_editor.list_state.selected().unwrap_or(0);
        let def = Podcast::default();
        let p = &mut self.podcast_editor.working_copy;
        match fi {
            0 => p.feed_url = def.feed_url,
            1 => p.name = def.name,
            2 => p.album_name = def.album_name,
            3 => p.artist = def.artist,
            4 => p.overwrite_tags = def.overwrite_tags,
            5 => p.remove_images = def.remove_images,
            6 => p.remove_existing_tags = def.remove_existing_tags,
            7 => p.overwrite_album_name = def.overwrite_album_name,
            8 => p.overwrite_artist = def.overwrite_artist,
            9 => p.overwrite_file_name = def.overwrite_file_name,
            10 => p.overwrite_title = def.overwrite_title,
            11 => p.append_number_to_title = def.append_number_to_title,
            12 => p.leading_zeros_amount = def.leading_zeros_amount,
            13 => p.user_agent = def.user_agent,
            _ => return,
        }
        self.podcast_editor.dirty = true;
    }

    pub fn podcast_editor_reset_all_defaults(&mut self) {
        let def = Podcast::default();
        let p = &mut self.podcast_editor.working_copy;
        p.overwrite_tags = def.overwrite_tags;
        p.remove_images = def.remove_images;
        p.remove_existing_tags = def.remove_existing_tags;
        p.overwrite_album_name = def.overwrite_album_name;
        p.overwrite_artist = def.overwrite_artist;
        p.overwrite_file_name = def.overwrite_file_name;
        p.overwrite_title = def.overwrite_title;
        p.append_number_to_title = def.append_number_to_title;
        p.leading_zeros_amount = def.leading_zeros_amount;
        self.podcast_editor.dirty = true;
    }

    pub fn podcast_editor_save(&mut self) -> Result<()> {
        match &self.podcast_editor.target {
            PodcastEditorTarget::Existing(i) => {
                let i = *i;
                if i < self.config.podcasts.len() {
                    self.config.podcasts[i] = self.podcast_editor.working_copy.clone();
                }
            }
            PodcastEditorTarget::Template => {
                self.config.default_podcast = self.podcast_editor.working_copy.clone();
            }
        }
        save_config(&self.config)?;
        let count = self.config.podcasts.len();
        self.latest_episodes = vec![None; count];
        let mut ls = ListState::default();
        ls.select(Some(0));
        self.podcast_list_state = ls;
        self.podcast_editor_go_back();
        Ok(())
    }

    pub fn podcast_editor_discard(&mut self) {
        if self.podcast_editor.dirty {
            self.podcast_editor.show_confirm_discard = true;
        } else {
            self.podcast_editor_go_back();
        }
    }

    pub fn podcast_editor_go_back(&mut self) {
        match self.podcast_editor.back_to {
            Screen::Config => self.enter_config(),
            Screen::EditPodcastSelect => self.screen = Screen::EditPodcastSelect,
            _ => self.screen = Screen::PodcastList,
        }
    }
}
