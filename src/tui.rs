use std::io::stdout;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    ExecutableCommand,
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::Terminal;
use ratatui::prelude::CrosstermBackend;
use tokio::sync::mpsc;

use crate::app::{self, Action, App, KeyContext, Screen};
use crate::podcast::{
    self, download_selected_episodes, fetch_episode_list, fetch_feed_metadata,
    get_last_podcast_name,
};
use crate::ui;

type LatestRx = mpsc::UnboundedReceiver<(String, (String, Option<String>))>;
type EpisodesRx = mpsc::UnboundedReceiver<Result<Vec<podcast::EpisodeInfo>, String>>;
type DownloadRx = mpsc::UnboundedReceiver<podcast::DownloadEvent>;
type FeedInfoRx = mpsc::UnboundedReceiver<Result<podcast::FeedMetadata, String>>;

/// Receivers for the background work the TUI starts.
///
/// Each is replaced wholesale when a new task supersedes the last, which drops
/// any results still queued from the previous one.
struct Channels {
    latest: LatestRx,
    episodes: EpisodesRx,
    downloads: DownloadRx,
    feed_info: FeedInfoRx,
}

fn paste_from_clipboard(target: &mut String) {
    if let Ok(mut cb) = arboard::Clipboard::new()
        && let Ok(text) = cb.get_text()
    {
        for c in text.chars().filter(|c| *c != '\n' && *c != '\r') {
            target.push(c);
        }
    }
}

fn spawn_latest_fetches(app: &App) -> LatestRx {
    let (tx, rx) = mpsc::unbounded_channel();
    for i in 0..app.config.podcasts.len() {
        let tx = tx.clone();
        let feed_url = app.config.podcasts[i].feed_url.clone();
        let name = app.config.podcasts[i].name.clone();
        if feed_url.is_empty() || name.is_empty() {
            continue;
        }
        let key = feed_url.clone();
        tokio::spawn(async move {
            let p = podcast::Podcast {
                name,
                feed_url,
                ..podcast::Podcast::default()
            };
            if let Ok(title) = get_last_podcast_name(&p).await {
                let _ = tx.send((key, title));
            }
        });
    }
    rx
}

/// Apply `Home`/`End`/`PageUp`/`PageDown` to whichever list the context owns.
///
/// Runs before the keymap so the jump keys reach every list without each
/// context repeating them. Contexts with no list of their own, the overlays
/// among them, return false and leave the key to the keymap.
fn apply_list_jump(app: &mut App, context: KeyContext, code: KeyCode) -> bool {
    let podcast_count = app.config.podcasts.len();
    match context {
        KeyContext::PodcastList => {
            app::apply_list_jump(&mut app.podcast_list_state, podcast_count, 0, code)
        }
        KeyContext::EpisodeSelect | KeyContext::EpisodeSelectLoading => {
            let episode_count = app.episodes.len();
            app::apply_list_jump(&mut app.episode_list_state, episode_count, 0, code)
        }
        KeyContext::ConfigMenu => app::apply_list_jump(
            &mut app.config_editor.menu_state,
            app::CONFIG_MENU_ITEM_COUNT,
            0,
            code,
        ),
        KeyContext::EditPodcastSelect => {
            app::apply_list_jump(&mut app.edit_podcast_select_state, podcast_count, 0, code)
        }
        KeyContext::EditorNavigate => {
            let floor = app.podcast_editor_first_field();
            app::apply_list_jump(
                &mut app.podcast_editor.list_state,
                app::PODCAST_FIELD_COUNT,
                floor,
                code,
            )
        }
        _ => false,
    }
}

/// The text buffer the context is editing, if it is editing one.
fn text_buffer(app: &mut App, context: KeyContext) -> Option<&mut String> {
    match context {
        KeyContext::ConfigDirEdit => Some(&mut app.config_editor.text_buffer),
        KeyContext::EditorTextEdit => Some(&mut app.podcast_editor.text_buffer),
        KeyContext::WizardText => Some(&mut app.add_podcast.text_buffer),
        _ => None,
    }
}

/// Type a key the keymap did not claim into the context's text buffer.
fn type_into_buffer(app: &mut App, context: KeyContext, code: KeyCode) {
    let Some(buffer) = text_buffer(app, context) else {
        return;
    };
    match code {
        KeyCode::Char(c) => buffer.push(c),
        KeyCode::Backspace => {
            buffer.pop();
        }
        _ => {}
    }
}

fn apply_action(app: &mut App, action: Action, channels: &mut Channels) {
    match action {
        Action::ForceQuit => app.request_force_quit(),
        Action::Quit => app.request_quit(),
        Action::TogglePalette => {
            if app.palette.open {
                app.palette_close();
            } else {
                app.palette_open();
            }
        }
        Action::ToggleHintBar => app.toggle_hint_bar(),

        Action::GoToConfig => {
            if !app.download_in_progress() {
                app.enter_config();
            }
        }
        Action::GoToPodcastList => {
            if !app.download_in_progress() {
                app.screen = Screen::PodcastList;
            }
        }
        Action::RefreshFeeds => {
            app.latest_episodes = vec![None; app.config.podcasts.len()];
            channels.latest = spawn_latest_fetches(app);
        }
        Action::AddPodcast => {
            if !app.download_in_progress() {
                app.enter_add_podcast();
            }
        }
        Action::OpenDownloadFolder => app.open_download_folder(),
        Action::OpenPodcastFolder => app.open_podcast_folder(),
        Action::MovePodcastUp => {
            if app.screen == Screen::PodcastList {
                let _ = app.move_podcast_up();
            }
        }
        Action::MovePodcastDown => {
            if app.screen == Screen::PodcastList {
                let _ = app.move_podcast_down();
            }
        }

        Action::PodcastUp => app.move_up(),
        Action::PodcastDown => app.move_down(),
        Action::OpenEpisodeList => {
            let idx = app.selected_podcast_index();
            if idx < app.config.podcasts.len() {
                let feed_url = app.config.podcasts[idx].feed_url.clone();
                app.enter_episode_select();

                let (tx, rx) = mpsc::unbounded_channel();
                channels.episodes = rx;
                tokio::spawn(async move {
                    match fetch_episode_list(&feed_url).await {
                        Ok(eps) => {
                            let _ = tx.send(Ok(eps));
                        }
                        Err(e) => {
                            let _ = tx.send(Err(e.to_string()));
                        }
                    }
                });
            }
        }
        Action::EditSelectedPodcast => {
            let idx = app.selected_podcast_index();
            if idx < app.config.podcasts.len() {
                app.enter_podcast_editor_existing(idx);
            }
        }
        Action::PromptDeletePodcast => {
            let idx = app.selected_podcast_index();
            if idx < app.config.podcasts.len() {
                app.podcast_delete_pending = Some(idx);
            }
        }
        Action::ConfirmDeletePodcast => {
            if app.confirm_delete_podcast().is_ok() {
                channels.latest = spawn_latest_fetches(app);
            }
        }
        Action::CancelDeletePodcast => app.podcast_delete_pending = None,

        Action::EpisodeUp => app.episode_move_up(),
        Action::EpisodeDown => app.episode_move_down(),
        Action::ToggleEpisode => app.toggle_current_episode(),
        Action::ToggleAllEpisodes => app.select_all_episodes(),
        Action::ToggleSortOrder => app.toggle_sort_order(),
        Action::StartDownload => {
            let indices = app.selected_feed_indices();
            if !indices.is_empty() {
                let dir = app.config.download_dir_location.clone();
                let podcast_idx = app.selected_podcast_index();
                let p = app.config.podcasts[podcast_idx].clone();
                app.start_download();

                let (tx, rx) = mpsc::unbounded_channel();
                channels.downloads = rx;
                tokio::spawn(async move {
                    if let Err(e) = download_selected_episodes(&dir, &p, indices, tx.clone()).await
                    {
                        let _ = tx.send(podcast::DownloadEvent::Error {
                            message: e.to_string(),
                        });
                        let _ = tx.send(podcast::DownloadEvent::Finished);
                    }
                });
            }
        }
        Action::GoBack => app.go_back(),

        Action::ConfirmCreateFolder => app.confirm_create_and_open_folder(),
        Action::CancelCreateFolder => app.cancel_open_folder(),

        Action::ConfigMenuUp => app.config_menu_move_up(),
        Action::ConfigMenuDown => app.config_menu_move_down(),
        Action::ConfigActivateRow => app.config_activate_row(),
        Action::ConfigRestoreDirDefault => app.config_restore_dir_to_default(),
        Action::ConfigCommitDirEdit => app.config_commit_dir_edit(),
        Action::ConfigCancelDirEdit => app.config_cancel_edit(),
        Action::ConfigConfirmDirChange => {
            let _ = app.config_confirm_dir_change();
        }
        Action::ConfigCancelDirChange => {
            app.config_editor.pending_dir_change = None;
            app.config_editor.dialog = app::ConfigDialog::Closed;
        }
        Action::ConfigConfirmDirDefault => {
            let _ = app.config_confirm_dir_default();
        }
        Action::ConfigCancelDirDefault => {
            app.config_editor.dialog = app::ConfigDialog::Closed;
        }

        Action::EditSelectUp => app.edit_select_move_up(),
        Action::EditSelectDown => app.edit_select_move_down(),
        Action::OpenPodcastEditor => {
            let idx = app.edit_podcast_select_state.selected().unwrap_or(0);
            if idx < app.config.podcasts.len() {
                app.enter_podcast_editor_from_edit_select(idx);
            }
        }

        Action::EditorUp => app.podcast_editor_move_up(),
        Action::EditorDown => app.podcast_editor_move_down(),
        Action::EditorActivateField => app.podcast_editor_activate_field(),
        Action::EditorToggleBool => app.podcast_editor_toggle_bool(),
        Action::EditorIncrement => app.podcast_editor_increment_usize(1),
        Action::EditorDecrement => app.podcast_editor_increment_usize(-1),
        Action::EditorRestoreField => app.podcast_editor_restore_default(),
        Action::EditorResetAll => app.podcast_editor_reset_all_defaults(),
        Action::EditorSave => {
            if app.podcast_editor_save().is_ok() {
                channels.latest = spawn_latest_fetches(app);
            }
        }
        Action::EditorBack => app.podcast_editor_discard(),
        Action::EditorCommitEdit => app.podcast_editor_commit_edit(),
        Action::EditorCancelEdit => app.podcast_editor_cancel_edit(),
        Action::EditorConfirmSave => {
            if app.podcast_editor_save().is_ok() {
                if app.podcast_editor.quit_pending {
                    app.should_quit = true;
                } else {
                    channels.latest = spawn_latest_fetches(app);
                }
            }
        }
        Action::EditorConfirmDiscard => {
            if app.podcast_editor.quit_pending {
                app.should_quit = true;
            } else {
                app.podcast_editor_go_back();
            }
        }
        Action::EditorConfirmCancel => {
            app.podcast_editor.quit_pending = false;
            app.podcast_editor.show_confirm_discard = false;
        }

        Action::WizardChooseManual => app.add_podcast_choose_manual(),
        Action::WizardPrepopulate => {
            app.add_podcast.mode_select = false;
            app.add_podcast.loading_feed_info = true;
            let url = app.add_podcast.podcast.feed_url.clone();

            let (tx, rx) = mpsc::unbounded_channel();
            channels.feed_info = rx;
            tokio::spawn(async move {
                let result = fetch_feed_metadata(&url).await.map_err(|e| e.to_string());
                let _ = tx.send(result);
            });
        }
        Action::WizardCancelModeSelect => app.add_podcast_cancel_mode_select(),
        Action::WizardCancelFetch => app.add_podcast_cancel_fetch(),
        Action::WizardTextNext => {
            if app.add_podcast.step == 0 {
                app.add_podcast_enter_mode_select();
            } else if app.add_podcast_next() && app.add_podcast_save().is_ok() {
                channels.latest = spawn_latest_fetches(app);
            }
        }
        Action::WizardTextBack => {
            if app.add_podcast.text_buffer.is_empty() {
                app.add_podcast_prev();
            } else {
                app.add_podcast.text_buffer.pop();
            }
        }
        Action::WizardNext => {
            if app.add_podcast_next() && app.add_podcast_save().is_ok() {
                channels.latest = spawn_latest_fetches(app);
            }
        }
        Action::WizardPrev => app.add_podcast_prev(),
        Action::WizardToggleBool => app.add_podcast_toggle_bool(),
        Action::WizardIncrement => app.add_podcast_adjust_usize(1),
        Action::WizardDecrement => app.add_podcast_adjust_usize(-1),
        Action::WizardCancel => app.screen = Screen::Config,

        Action::Paste => {
            let context = app.key_context();
            if let Some(buffer) = text_buffer(app, context) {
                let mut pasted = String::new();
                paste_from_clipboard(&mut pasted);
                buffer.push_str(&pasted);
            }
        }
    }
}

pub async fn run(config: podcast::Config, config_notice: Option<String>) -> Result<()> {
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        original_hook(panic_info);
    }));

    let mut app = App::new(config, config_notice);

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    let (_ep_tx, ep_rx) = mpsc::unbounded_channel();
    let (_dl_tx, dl_rx) = mpsc::unbounded_channel();
    let (_feed_info_tx, feed_info_rx) = mpsc::unbounded_channel();

    let mut channels = Channels {
        latest: spawn_latest_fetches(&app),
        episodes: ep_rx,
        downloads: dl_rx,
        feed_info: feed_info_rx,
    };

    let result: Result<()> = (|| {
        loop {
            terminal.draw(|f| ui::draw(f, &mut app))?;

            while let Ok((feed_url, latest_episode)) = channels.latest.try_recv() {
                if let Some(index) = app
                    .config
                    .podcasts
                    .iter()
                    .position(|podcast| podcast.feed_url == feed_url)
                    && index < app.latest_episodes.len()
                {
                    app.latest_episodes[index] = Some(latest_episode);
                }
            }

            while let Ok(episode_result) = channels.episodes.try_recv() {
                match episode_result {
                    Ok(episodes) => app.set_episodes(episodes),
                    Err(error) => app.set_episode_load_error(error),
                }
            }

            while let Ok(download_event) = channels.downloads.try_recv() {
                app.handle_download_event(download_event);
            }

            while let Ok(feed_result) = channels.feed_info.try_recv() {
                if app.screen == Screen::AddPodcast && app.add_podcast.loading_feed_info {
                    match feed_result {
                        Ok(feed) => app.add_podcast_feed_loaded(feed.title, feed.author),
                        Err(_) => app.add_podcast_feed_failed(),
                    }
                }
            }

            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if let Some(action) = app::resolve_global(&app, &key) {
                    apply_action(&mut app, action, &mut channels);
                    continue;
                }

                if app.palette.open {
                    if key.modifiers.contains(KeyModifiers::CONTROL)
                        && key.code == KeyCode::Char('v')
                    {
                        let mut pasted = String::new();
                        paste_from_clipboard(&mut pasted);
                        for c in pasted.chars() {
                            app.palette_type(c);
                        }
                        continue;
                    }
                    match key.code {
                        KeyCode::Esc => app.palette_close(),
                        KeyCode::Up | KeyCode::BackTab => app.palette_move_up(),
                        KeyCode::Down | KeyCode::Tab => app.palette_move_down(),
                        KeyCode::Backspace => app.palette_backspace(),
                        KeyCode::Enter => {
                            if let Some(action) = app.palette_execute() {
                                app.palette_close();
                                apply_action(&mut app, action, &mut channels);
                            }
                        }
                        KeyCode::Char(c) => app.palette_type(c),
                        _ => {}
                    }
                    continue;
                }

                if app.config_notice.take().is_some() {
                    if app.screen != Screen::PodcastList {
                        continue;
                    }
                    if let Ok((new_config, _)) = podcast::load_config() {
                        app.config = new_config;
                        app.latest_episodes = vec![None; app.config.podcasts.len()];
                        app.podcast_list_state =
                            ratatui::widgets::ListState::default().with_selected(Some(0));
                        channels.latest = spawn_latest_fetches(&app);
                    }
                    if key.code != KeyCode::Char('c') {
                        continue;
                    }
                }

                let context = app.key_context();
                if apply_list_jump(&mut app, context, key.code) {
                    continue;
                }
                match app::resolve_context(&app, &key) {
                    Some(action) => apply_action(&mut app, action, &mut channels),
                    None => type_into_buffer(&mut app, context, key.code),
                }
            }

            if app.should_quit {
                break;
            }
        }

        Ok(())
    })();

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    result
}
