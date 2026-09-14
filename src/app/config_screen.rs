//! Config screen methods: download directory editing, dialogs, and file/folder opening.

use anyhow::Result;
use ratatui::widgets::ListState;

use crate::podcast::{DefaultMode, save_config};

use super::state::*;

impl App {
    pub fn enter_config(&mut self) {
        self.podcast_delete_pending = None;
        let mut menu_state = ListState::default();
        menu_state.select(Some(0));
        self.config_editor.menu_state = menu_state;
        self.config_editor.edit_mode = ConfigEditMode::Navigate;
        self.config_editor.text_buffer = String::new();
        self.config_editor.pending_dir_change = None;
        self.config_editor.dialog = ConfigDialog::Closed;
        self.screen = Screen::Config;
    }

    pub fn config_menu_move_up(&mut self) {
        let i = self.config_editor.menu_state.selected().unwrap_or(0);
        if i > 0 {
            self.config_editor.menu_state.select(Some(i - 1));
        }
    }

    pub fn config_menu_move_down(&mut self) {
        let i = self.config_editor.menu_state.selected().unwrap_or(0);
        if i < CONFIG_MENU_ITEM_COUNT - 1 {
            self.config_editor.menu_state.select(Some(i + 1));
        }
    }

    /// Activate the highlighted config menu row.
    ///
    /// The rows are positional and match the order `ui::config` renders them,
    /// so inserting one means updating both.
    pub fn config_activate_row(&mut self) {
        match self.config_editor.menu_state.selected().unwrap_or(0) {
            0 => self.enter_add_podcast(),
            1 => self.config_enter_dir_edit(),
            2 => {
                let _ = self.config_toggle_default_mode();
            }
            3 => self.enter_template_editor(),
            4 => self.enter_edit_podcast_select(),
            5 => {
                let _ = self.config_open_file();
            }
            6 => {
                let _ = self.config_open_podcasts_file();
            }
            7 => self.open_download_folder(),
            _ => {}
        }
    }

    pub fn config_enter_dir_edit(&mut self) {
        self.config_editor.text_buffer = self.config.download_dir_location.clone();
        self.config_editor.edit_mode = ConfigEditMode::EditingText;
    }

    pub fn config_commit_dir_edit(&mut self) {
        let buf = self.config_editor.text_buffer.clone();
        self.config_editor.pending_dir_change = Some(buf);
        self.config_editor.dialog = ConfigDialog::ConfirmDirChange;
        self.config_editor.text_buffer.clear();
        self.config_editor.edit_mode = ConfigEditMode::Navigate;
    }

    pub fn config_cancel_edit(&mut self) {
        self.config_editor.text_buffer.clear();
        self.config_editor.edit_mode = ConfigEditMode::Navigate;
    }

    pub fn config_restore_dir_to_default(&mut self) {
        self.config_editor.dialog = ConfigDialog::ConfirmDirDefault;
    }

    pub fn config_confirm_dir_default(&mut self) -> Result<()> {
        self.config.download_dir_location = crate::podcast::default_download_dir_location();
        save_config(&self.config)?;
        self.config_editor.dialog = ConfigDialog::Closed;
        Ok(())
    }

    pub fn config_toggle_default_mode(&mut self) -> Result<()> {
        self.config.default_mode = match self.config.default_mode {
            DefaultMode::Tui => DefaultMode::Cli,
            DefaultMode::Cli => DefaultMode::Tui,
        };
        save_config(&self.config)?;
        Ok(())
    }

    pub fn config_confirm_dir_change(&mut self) -> Result<()> {
        if let Some(new_dir) = self.config_editor.pending_dir_change.take() {
            self.config.download_dir_location = new_dir;
            save_config(&self.config)?;
        }
        self.config_editor.dialog = ConfigDialog::Closed;
        Ok(())
    }

    pub fn config_open_file(&self) -> Result<()> {
        let path = crate::podcast::config_path()?;
        open::that(path)?;
        Ok(())
    }

    pub fn config_open_podcasts_file(&self) -> Result<()> {
        let path = crate::podcast::config_dir()?.join("podcasts.json");
        open::that(path)?;
        Ok(())
    }

    pub fn open_download_folder(&mut self) {
        let path = &self.config.download_dir_location;
        if std::path::Path::new(path).exists() {
            let _ = open::that(path);
        } else {
            self.pending_open_folder = Some(path.to_string());
            if self.screen == Screen::Config {
                self.config_editor.dialog = ConfigDialog::ConfirmCreateDir;
            }
        }
    }

    pub fn open_podcast_folder(&mut self) {
        let idx = self.selected_podcast_index();
        if idx >= self.config.podcasts.len() {
            return;
        }
        let podcast = &self.config.podcasts[idx];
        let album = podcast.effective_album_name();
        let path = std::path::Path::new(&self.config.download_dir_location)
            .join(album)
            .to_string_lossy()
            .to_string();
        if std::path::Path::new(&path).exists() {
            let _ = open::that(&path);
        } else {
            self.pending_open_folder = Some(path);
        }
    }

    pub fn confirm_create_and_open_folder(&mut self) {
        if let Some(path) = self.pending_open_folder.take() {
            let _ = std::fs::create_dir_all(&path);
            let _ = open::that(&path);
        }
        if self.screen == Screen::Config {
            self.config_editor.dialog = ConfigDialog::Closed;
        }
    }

    pub fn cancel_open_folder(&mut self) {
        self.pending_open_folder = None;
        if self.screen == Screen::Config {
            self.config_editor.dialog = ConfigDialog::Closed;
        }
    }
}
