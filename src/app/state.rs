//! Core application state, screen enum, and episode/download logic.

use std::time::Instant;

use crossterm::event::KeyCode;
use ratatui::widgets::ListState;

use crate::podcast::{Config, DownloadEvent, EpisodeInfo, Podcast};

use super::add_podcast_wizard::AddPodcastState;
use super::commands::{COMMANDS, CommandPaletteState, palette_command_visible};

/// Available screens in the application.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Screen {
    PodcastList,
    EpisodeSelect,
    Downloading,
    Config,
    EditPodcast,
    EditPodcastSelect,
    AddPodcast,
}

/// Episode list sort direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SortOrder {
    Desc,
    Asc,
}

impl SortOrder {
    /// Return the opposite sort order.
    pub fn toggle(&self) -> Self {
        match self {
            SortOrder::Desc => SortOrder::Asc,
            SortOrder::Asc => SortOrder::Desc,
        }
    }

    /// Human-readable label for the current sort order.
    pub fn label(&self) -> &'static str {
        match self {
            SortOrder::Desc => "Newest first",
            SortOrder::Asc => "Oldest first",
        }
    }
}

/// Tracks progress of an in-flight download batch.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub current_episode: String,
    pub current_index: usize,
    pub total: usize,
    pub completed: Vec<String>,
    pub tagged: Vec<String>,
    pub finished: bool,
    pub errors: Vec<String>,
}

impl DownloadProgress {
    /// Create an empty download progress tracker.
    pub fn new() -> Self {
        Self {
            current_episode: String::new(),
            current_index: 0,
            total: 0,
            completed: Vec::new(),
            tagged: Vec::new(),
            finished: false,
            errors: Vec::new(),
        }
    }

    /// True until the batch reports `Finished`.
    ///
    /// Per-episode errors do not end a batch: `download_selected_episodes`
    /// records the failure and carries on with the remaining selections, so
    /// only `Finished` marks the run complete.
    pub fn is_active(&self) -> bool {
        !self.finished
    }
}

/// Editing mode for config and podcast editor screens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigEditMode {
    Navigate,
    EditingText,
}

/// Which confirmation dialog is open on the config screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConfigDialog {
    Closed,
    ConfirmDirChange,
    ConfirmDirDefault,
    ConfirmCreateDir,
}

/// Number of rows Page Up and Page Down travel in a list.
pub const LIST_PAGE_STEP: usize = 10;

/// Apply a jump or page key to a list selection.
///
/// Handles `Home`/`g`, `End`/`G`, `PageUp` and `PageDown`, clamping the result
/// to `floor..len`. Returns true when the key was consumed.
pub fn apply_list_jump(state: &mut ListState, len: usize, floor: usize, code: KeyCode) -> bool {
    if len == 0 || floor >= len {
        return false;
    }
    let last = len - 1;
    let current = state.selected().unwrap_or(floor).clamp(floor, last);
    let target = match code {
        KeyCode::Home | KeyCode::Char('g') => floor,
        KeyCode::End | KeyCode::Char('G') => last,
        KeyCode::PageUp => current.saturating_sub(LIST_PAGE_STEP).max(floor),
        KeyCode::PageDown => current.saturating_add(LIST_PAGE_STEP).min(last),
        _ => return false,
    };
    state.select(Some(target));
    true
}

/// Number of items in the config menu.
pub const CONFIG_MENU_ITEM_COUNT: usize = 9;
/// Total number of editable fields in the podcast editor.
pub const PODCAST_FIELD_COUNT: usize = 14;
/// Index of the first boolean field in the podcast editor.
pub const PODCAST_FIELD_BOOL_START: usize = 4;
/// Index of the leading-zeros usize field in the podcast editor.
pub const PODCAST_FIELD_USIZE: usize = 12;

/// UI state for the config menu screen.
pub struct ConfigEditorState {
    pub menu_state: ListState,
    pub edit_mode: ConfigEditMode,
    pub text_buffer: String,
    pub pending_dir_change: Option<String>,
    pub dialog: ConfigDialog,
}

/// Whether the podcast editor is editing an existing podcast or the default template.
#[derive(Debug)]
pub enum PodcastEditorTarget {
    Existing(usize),
    Template,
}

/// UI state for the single-podcast editor screen.
pub struct PodcastEditorState {
    pub target: PodcastEditorTarget,
    pub back_to: Screen,
    pub working_copy: Podcast,
    pub list_state: ListState,
    pub edit_mode: ConfigEditMode,
    pub text_buffer: String,
    pub dirty: bool,
    pub show_confirm_discard: bool,
    pub quit_pending: bool,
}

/// Application state for the TUI podcast manager.
pub struct App {
    pub screen: Screen,
    pub config: Config,
    pub podcast_list_state: ListState,
    pub latest_episodes: Vec<Option<(String, Option<String>)>>,
    pub download_progress: DownloadProgress,
    pub should_quit: bool,

    pub episodes: Vec<EpisodeInfo>,
    pub episode_selected: Vec<bool>,
    pub episode_list_state: ListState,
    pub loading_episodes: bool,
    pub episode_load_error: Option<String>,
    pub sort_order: SortOrder,
    pub config_notice: Option<String>,

    pub config_editor: ConfigEditorState,
    pub palette: CommandPaletteState,
    pub podcast_delete_pending: Option<usize>,
    pub podcast_editor: PodcastEditorState,
    pub edit_podcast_select_state: ListState,
    pub add_podcast: AddPodcastState,
    pub pending_open_folder: Option<String>,
    pub hint_bar_expanded: bool,
    pub banner_started: Instant,
}

impl App {
    /// Initialize the app with the given config and an optional startup notice.
    pub fn new(config: Config, config_notice: Option<String>) -> Self {
        let podcast_count = config.podcasts.len();
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let mut menu_state = ListState::default();
        menu_state.select(Some(0));

        let palette_matches: Vec<usize> = (0..COMMANDS.len())
            .filter(|&i| palette_command_visible(Screen::PodcastList, &COMMANDS[i]))
            .collect();
        let mut palette_list_state = ListState::default();
        if !palette_matches.is_empty() {
            palette_list_state.select(Some(0));
        }

        let mut ped_list_state = ListState::default();
        ped_list_state.select(Some(0));

        Self {
            screen: Screen::PodcastList,
            config,
            podcast_list_state: list_state,
            latest_episodes: vec![None; podcast_count],
            download_progress: DownloadProgress::new(),
            should_quit: false,
            episodes: Vec::new(),
            episode_selected: Vec::new(),
            episode_list_state: ListState::default(),
            loading_episodes: false,
            episode_load_error: None,
            sort_order: SortOrder::Desc,
            config_notice,
            config_editor: ConfigEditorState {
                menu_state,
                edit_mode: ConfigEditMode::Navigate,
                text_buffer: String::new(),
                pending_dir_change: None,
                dialog: ConfigDialog::Closed,
            },
            palette: CommandPaletteState {
                open: false,
                query: String::new(),
                list_state: palette_list_state,
                matches: palette_matches,
            },
            podcast_delete_pending: None,
            podcast_editor: PodcastEditorState {
                target: PodcastEditorTarget::Existing(0),
                back_to: Screen::PodcastList,
                working_copy: Podcast::default(),
                list_state: ped_list_state,
                edit_mode: ConfigEditMode::Navigate,
                text_buffer: String::new(),
                dirty: false,
                show_confirm_discard: false,
                quit_pending: false,
            },
            edit_podcast_select_state: ListState::default(),
            add_podcast: AddPodcastState {
                step: 0,
                podcast: Podcast::default(),
                text_buffer: String::new(),
                loading_feed_info: false,
                mode_select: false,
            },
            pending_open_folder: None,
            hint_bar_expanded: false,
            banner_started: Instant::now(),
        }
    }

    pub fn toggle_hint_bar(&mut self) {
        self.hint_bar_expanded = !self.hint_bar_expanded;
    }

    /// True while a download batch is still running.
    pub fn download_in_progress(&self) -> bool {
        self.screen == Screen::Downloading && self.download_progress.is_active()
    }

    /// Quit, unless a download is running or the editor has unsaved changes.
    ///
    /// The one path both `q` and the palette's quit entry take, so the guards
    /// cannot drift apart. Unsaved editor changes open the save/discard
    /// confirmation instead of quitting.
    pub fn request_quit(&mut self) {
        if self.download_in_progress() {
            return;
        }
        self.request_force_quit();
    }

    /// Quit even mid-download, still asking about unsaved editor changes.
    pub fn request_force_quit(&mut self) {
        self.palette_close();
        if self.screen == Screen::EditPodcast && self.podcast_editor.dirty {
            self.podcast_editor.quit_pending = true;
            self.podcast_editor.show_confirm_discard = true;
        } else {
            self.should_quit = true;
        }
    }

    /// Index of the currently highlighted podcast in the list.
    pub fn selected_podcast_index(&self) -> usize {
        self.podcast_list_state.selected().unwrap_or(0)
    }

    /// Name of the currently highlighted podcast.
    pub fn selected_podcast_name(&self) -> &str {
        &self.config.podcasts[self.selected_podcast_index()].name
    }

    /// Navigate one podcast up in the list.
    pub fn move_up(&mut self) {
        let i = self.selected_podcast_index();
        if i > 0 {
            self.podcast_list_state.select(Some(i - 1));
        }
    }

    /// Navigate one podcast down in the list.
    pub fn move_down(&mut self) {
        let i = self.selected_podcast_index();
        if i < self.config.podcasts.len().saturating_sub(1) {
            self.podcast_list_state.select(Some(i + 1));
        }
    }

    /// Switch to the episode selection screen for the current podcast.
    pub fn enter_episode_select(&mut self) {
        self.podcast_delete_pending = None;
        self.screen = Screen::EpisodeSelect;
        self.episodes.clear();
        self.episode_selected.clear();
        self.episode_list_state = ListState::default();
        self.loading_episodes = true;
        self.episode_load_error = None;
    }

    /// Store a fetched episode list and reset selection state.
    pub fn set_episodes(&mut self, episodes: Vec<EpisodeInfo>) {
        let count = episodes.len();
        self.episodes = episodes;
        self.episode_selected = vec![false; count];
        if count > 0 {
            self.episode_list_state.select(Some(0));
        }
        self.loading_episodes = false;
    }

    /// Reverse the episode list sort order.
    pub fn toggle_sort_order(&mut self) {
        self.sort_order = self.sort_order.toggle();
        self.episodes.reverse();
        self.episode_selected.reverse();
        if !self.episodes.is_empty() {
            self.episode_list_state.select(Some(0));
        }
    }

    /// Record an error that occurred while loading episodes.
    pub fn set_episode_load_error(&mut self, err: String) {
        self.episode_load_error = Some(err);
        self.loading_episodes = false;
    }

    /// Move the episode cursor up by one.
    pub fn episode_move_up(&mut self) {
        if let Some(i) = self.episode_list_state.selected()
            && i > 0
        {
            self.episode_list_state.select(Some(i - 1));
        }
    }

    /// Move the episode cursor down by one.
    pub fn episode_move_down(&mut self) {
        if let Some(i) = self.episode_list_state.selected()
            && i < self.episodes.len().saturating_sub(1)
        {
            self.episode_list_state.select(Some(i + 1));
        }
    }

    /// Toggle the download checkbox on the current episode.
    pub fn toggle_current_episode(&mut self) {
        if let Some(i) = self.episode_list_state.selected()
            && i < self.episode_selected.len()
        {
            self.episode_selected[i] = !self.episode_selected[i];
        }
    }

    /// Toggle all episode checkboxes on or off.
    pub fn select_all_episodes(&mut self) {
        let all_selected = self.episode_selected.iter().all(|&s| s);
        self.episode_selected.fill(!all_selected);
    }

    /// Number of currently selected episodes.
    pub fn selected_count(&self) -> usize {
        self.episode_selected.iter().filter(|&&s| s).count()
    }

    /// Feed indices of all selected episodes for download.
    pub fn selected_feed_indices(&self) -> Vec<usize> {
        self.episodes
            .iter()
            .zip(self.episode_selected.iter())
            .filter(|&(_, &sel)| sel)
            .map(|(ep, _)| ep.feed_index)
            .collect()
    }

    /// Navigate back from the current screen.
    pub fn go_back(&mut self) {
        match self.screen {
            Screen::EpisodeSelect => self.screen = Screen::PodcastList,
            Screen::Downloading if !self.download_progress.is_active() => {
                self.screen = Screen::PodcastList;
                self.download_progress = DownloadProgress::new();
            }
            _ => {}
        }
    }

    /// Switch to the downloading screen and reset progress.
    pub fn start_download(&mut self) {
        self.screen = Screen::Downloading;
        self.download_progress = DownloadProgress::new();
    }

    /// Process a download progress event from the background task.
    pub fn handle_download_event(&mut self, event: DownloadEvent) {
        match event {
            DownloadEvent::Started {
                episode_name,
                index,
                total,
            } => {
                self.download_progress.current_episode = episode_name;
                self.download_progress.current_index = index;
                self.download_progress.total = total;
            }
            DownloadEvent::Completed { episode_name } => {
                self.download_progress.completed.push(episode_name);
            }
            DownloadEvent::Tagged { episode_name } => {
                self.download_progress.tagged.push(episode_name);
            }
            DownloadEvent::Error { message } => {
                self.download_progress.errors.push(message);
            }
            DownloadEvent::Finished => {
                self.download_progress.finished = true;
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn test_app_one_podcast() -> App {
    use crate::podcast::{Config, Podcast};
    let config = Config {
        download_dir_location: "/tmp/test".to_string(),
        podcasts: vec![Podcast {
            name: "Pod1".into(),
            feed_url: "http://example.com/1".into(),
            ..Podcast::default()
        }],
        default_podcast: Podcast::default(),
        default_mode: crate::podcast::DefaultMode::default(),
        banner_style: crate::podcast::BannerStyle::default(),
    };
    App::new(config, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::podcast::{Config, Podcast};

    fn test_app() -> App {
        let config = Config {
            download_dir_location: "/tmp/test".to_string(),
            podcasts: vec![
                Podcast {
                    name: "Pod1".into(),
                    feed_url: "http://example.com/1".into(),
                    ..Podcast::default()
                },
                Podcast {
                    name: "Pod2".into(),
                    feed_url: "http://example.com/2".into(),
                    ..Podcast::default()
                },
            ],
            default_podcast: Podcast::default(),
            default_mode: crate::podcast::DefaultMode::default(),
            banner_style: crate::podcast::BannerStyle::default(),
        };
        App::new(config, None)
    }

    #[test]
    fn sort_order_toggle_desc_to_asc() {
        assert_eq!(SortOrder::Desc.toggle(), SortOrder::Asc);
    }

    #[test]
    fn sort_order_toggle_asc_to_desc() {
        assert_eq!(SortOrder::Asc.toggle(), SortOrder::Desc);
    }

    #[test]
    fn sort_order_desc_label() {
        assert_eq!(SortOrder::Desc.label(), "Newest first");
    }

    #[test]
    fn sort_order_asc_label() {
        assert_eq!(SortOrder::Asc.label(), "Oldest first");
    }

    #[test]
    fn download_progress_new_defaults() {
        let dp = DownloadProgress::new();
        assert!(!dp.finished);
        assert_eq!(dp.total, 0);
        assert_eq!(dp.current_index, 0);
        assert!(dp.current_episode.is_empty());
        assert!(dp.completed.is_empty());
        assert!(dp.errors.is_empty());
    }

    #[test]
    fn list_jump_home_and_end_reach_the_bounds() {
        let mut st = ListState::default();
        st.select(Some(3));
        assert!(apply_list_jump(&mut st, 8, 0, KeyCode::End));
        assert_eq!(st.selected(), Some(7));
        assert!(apply_list_jump(&mut st, 8, 0, KeyCode::Char('g')));
        assert_eq!(st.selected(), Some(0));
    }

    #[test]
    fn list_jump_pages_clamp_at_both_ends() {
        let mut st = ListState::default();
        st.select(Some(2));
        assert!(apply_list_jump(&mut st, 30, 0, KeyCode::PageUp));
        assert_eq!(st.selected(), Some(0));
        assert!(apply_list_jump(&mut st, 30, 0, KeyCode::PageDown));
        assert_eq!(st.selected(), Some(LIST_PAGE_STEP));
        st.select(Some(25));
        assert!(apply_list_jump(&mut st, 30, 0, KeyCode::PageDown));
        assert_eq!(st.selected(), Some(29));
    }

    #[test]
    fn list_jump_respects_the_floor() {
        let mut st = ListState::default();
        st.select(Some(9));
        assert!(apply_list_jump(&mut st, 14, 4, KeyCode::Home));
        assert_eq!(st.selected(), Some(4));
        assert!(apply_list_jump(&mut st, 14, 4, KeyCode::PageUp));
        assert_eq!(st.selected(), Some(4));
    }

    #[test]
    fn list_jump_handles_no_selection_and_empty_lists() {
        let mut st = ListState::default();
        assert!(apply_list_jump(&mut st, 5, 0, KeyCode::End));
        assert_eq!(st.selected(), Some(4));

        let mut empty = ListState::default();
        assert!(!apply_list_jump(&mut empty, 0, 0, KeyCode::End));
        assert_eq!(empty.selected(), None);

        let mut other = ListState::default();
        other.select(Some(1));
        assert!(!apply_list_jump(&mut other, 5, 0, KeyCode::Char('j')));
        assert_eq!(other.selected(), Some(1));
    }

    #[test]
    fn move_up_at_zero_stays() {
        let mut app = test_app();
        app.podcast_list_state.select(Some(0));
        app.move_up();
        assert_eq!(app.selected_podcast_index(), 0);
    }

    #[test]
    fn move_down_advances_index() {
        let mut app = test_app();
        app.podcast_list_state.select(Some(0));
        app.move_down();
        assert_eq!(app.selected_podcast_index(), 1);
    }

    #[test]
    fn toggle_sort_order_reverses_episodes() {
        let mut app = test_app();
        app.set_episodes(vec![
            EpisodeInfo {
                title: "Ep1".into(),
                feed_index: 1,
                pub_date: None,
            },
            EpisodeInfo {
                title: "Ep2".into(),
                feed_index: 2,
                pub_date: None,
            },
        ]);
        assert_eq!(app.sort_order, SortOrder::Desc);
        app.toggle_sort_order();
        assert_eq!(app.sort_order, SortOrder::Asc);
        assert_eq!(app.episodes[0].title, "Ep2");
        assert_eq!(app.episodes[1].title, "Ep1");
    }

    #[test]
    fn select_all_then_deselect_all() {
        let mut app = test_app();
        app.set_episodes(vec![
            EpisodeInfo {
                title: "Ep1".into(),
                feed_index: 1,
                pub_date: None,
            },
            EpisodeInfo {
                title: "Ep2".into(),
                feed_index: 2,
                pub_date: None,
            },
        ]);
        app.select_all_episodes();
        assert!(app.episode_selected.iter().all(|&s| s));
        app.select_all_episodes();
        assert!(app.episode_selected.iter().all(|&s| !s));
    }

    #[test]
    fn selected_feed_indices_returns_correct() {
        let mut app = test_app();
        app.set_episodes(vec![
            EpisodeInfo {
                title: "Ep1".into(),
                feed_index: 10,
                pub_date: None,
            },
            EpisodeInfo {
                title: "Ep2".into(),
                feed_index: 20,
                pub_date: None,
            },
            EpisodeInfo {
                title: "Ep3".into(),
                feed_index: 30,
                pub_date: None,
            },
        ]);
        app.episode_selected[0] = true;
        app.episode_selected[2] = true;
        assert_eq!(app.selected_feed_indices(), vec![10, 30]);
    }

    #[test]
    fn go_back_from_episode_select_to_podcast_list() {
        let mut app = test_app();
        app.screen = Screen::EpisodeSelect;
        app.go_back();
        assert_eq!(app.screen, Screen::PodcastList);
    }

    #[test]
    fn handle_download_event_started() {
        let mut app = test_app();
        app.handle_download_event(DownloadEvent::Started {
            episode_name: "Test Ep".into(),
            index: 1,
            total: 5,
        });
        assert_eq!(app.download_progress.current_episode, "Test Ep");
        assert_eq!(app.download_progress.current_index, 1);
        assert_eq!(app.download_progress.total, 5);
    }

    #[test]
    fn handle_download_event_completed() {
        let mut app = test_app();
        app.handle_download_event(DownloadEvent::Completed {
            episode_name: "Done Ep".into(),
        });
        assert_eq!(app.download_progress.completed, vec!["Done Ep"]);
    }

    #[test]
    fn an_episode_error_does_not_end_the_batch() {
        let mut app = test_app();
        app.start_download();
        app.handle_download_event(DownloadEvent::Started {
            episode_name: "Ep 1".into(),
            index: 1,
            total: 3,
        });
        app.handle_download_event(DownloadEvent::Error {
            message: "Episode index 9 is out of range — skipping".into(),
        });
        assert!(
            app.download_progress.is_active(),
            "a skipped episode must not end a batch that is still downloading"
        );
        app.handle_download_event(DownloadEvent::Finished);
        assert!(!app.download_progress.is_active());
    }

    #[test]
    fn handle_download_event_finished() {
        let mut app = test_app();
        assert!(!app.download_progress.finished);
        app.handle_download_event(DownloadEvent::Finished);
        assert!(app.download_progress.finished);
    }
}
