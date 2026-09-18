//! Command palette: actions, entries, filtering, and navigation.

use ratatui::widgets::ListState;

use super::keymap::{
    Action, Binding, CONFIG_BACK, GO_TO_CONFIG, OPEN_PODCAST_FOLDER, QUIT, REFRESH_FEEDS,
    REORDER_PODCASTS, TOGGLE_HINT_BAR,
};
use super::state::{App, Screen};

/// A single entry in the command palette with display info and visibility rules.
pub struct CommandEntry {
    pub category: &'static str,
    pub description: &'static str,
    pub description_podcast_list: Option<&'static str>,
    /// The binding whose keys the palette prints beside this entry, if any.
    ///
    /// The text comes from the binding's own keys, so rebinding a key updates
    /// the palette with it, and it is printed only where that key is wired —
    /// `c` opens the config editor from the podcast list, so the palette
    /// offers the entry everywhere but names the key only there.
    pub shortcut: Option<&'static Binding>,
    pub action: Action,
    pub hide_on: &'static [Screen],
}

impl CommandEntry {
    /// How the palette spells this entry's shortcut on the current screen.
    ///
    /// `None` where the entry has no shortcut, or where its key does nothing
    /// on the screen the palette is covering — every entry runs from the
    /// palette, but most of the keys are bound on one screen only.
    pub fn shortcut_text(&self, app: &App) -> Option<String> {
        let binding = self.shortcut?;
        if !binding.is_live_on_screen(app) {
            return None;
        }
        binding.shortcut_for(self.action)
    }
}

const SCREENS_EXCEPT_PODCAST_LIST: &[Screen] = &[
    Screen::EpisodeSelect,
    Screen::Downloading,
    Screen::Config,
    Screen::EditPodcast,
    Screen::EditPodcastSelect,
    Screen::AddPodcast,
];

/// All available command palette entries.
pub static COMMANDS: &[CommandEntry] = &[
    CommandEntry {
        category: "podcast",
        description: "refresh feeds",
        description_podcast_list: None,
        shortcut: Some(&REFRESH_FEEDS),
        action: Action::RefreshFeeds,
        hide_on: &[],
    },
    CommandEntry {
        category: "config",
        description: "open config editor",
        description_podcast_list: None,
        shortcut: Some(&GO_TO_CONFIG),
        action: Action::GoToConfig,
        hide_on: &[Screen::Config, Screen::Downloading],
    },
    CommandEntry {
        category: "podcast",
        description: "go to podcast list",
        description_podcast_list: None,
        shortcut: Some(&CONFIG_BACK),
        action: Action::GoToPodcastList,
        hide_on: &[Screen::PodcastList, Screen::Downloading],
    },
    CommandEntry {
        category: "podcast",
        description: "add new podcast",
        description_podcast_list: None,
        shortcut: None,
        action: Action::AddPodcast,
        hide_on: &[Screen::AddPodcast, Screen::Downloading],
    },
    CommandEntry {
        category: "folder",
        description: "open download folder",
        description_podcast_list: None,
        shortcut: None,
        action: Action::OpenDownloadFolder,
        hide_on: &[],
    },
    CommandEntry {
        category: "folder",
        description: "open podcast folder",
        description_podcast_list: Some("open selected podcast folder"),
        shortcut: Some(&OPEN_PODCAST_FOLDER),
        action: Action::OpenPodcastFolder,
        hide_on: &[Screen::AddPodcast, Screen::Config, Screen::EditPodcast],
    },
    CommandEntry {
        category: "podcast",
        description: "move podcast up",
        description_podcast_list: None,
        shortcut: Some(&REORDER_PODCASTS),
        action: Action::MovePodcastUp,
        hide_on: SCREENS_EXCEPT_PODCAST_LIST,
    },
    CommandEntry {
        category: "podcast",
        description: "move podcast down",
        description_podcast_list: None,
        shortcut: Some(&REORDER_PODCASTS),
        action: Action::MovePodcastDown,
        hide_on: SCREENS_EXCEPT_PODCAST_LIST,
    },
    CommandEntry {
        category: "app",
        description: "toggle hint bar",
        description_podcast_list: None,
        shortcut: Some(&TOGGLE_HINT_BAR),
        action: Action::ToggleHintBar,
        hide_on: &[],
    },
    CommandEntry {
        category: "app",
        description: "quit",
        description_podcast_list: None,
        shortcut: Some(&QUIT),
        action: Action::Quit,
        hide_on: &[],
    },
];

/// UI state for the command palette overlay.
pub struct CommandPaletteState {
    pub open: bool,
    pub query: String,
    pub list_state: ListState,
    pub matches: Vec<usize>,
}

/// Check whether a command should be visible on the given screen.
pub fn palette_command_visible(screen: Screen, cmd: &CommandEntry) -> bool {
    !cmd.hide_on.contains(&screen)
}

impl App {
    /// Open the command palette and reset its filter.
    pub fn palette_open(&mut self) {
        self.palette.open = true;
        self.palette.query.clear();
        let screen = self.screen;
        self.palette.matches = (0..COMMANDS.len())
            .filter(|&i| palette_command_visible(screen, &COMMANDS[i]))
            .collect();
        let mut state = ListState::default();
        if !self.palette.matches.is_empty() {
            state.select(Some(0));
        }
        self.palette.list_state = state;
    }

    /// Close the command palette.
    pub fn palette_close(&mut self) {
        self.palette.open = false;
    }

    fn palette_filter(&mut self) {
        let q = self.palette.query.to_lowercase();
        let screen = self.screen;
        self.palette.matches = COMMANDS
            .iter()
            .enumerate()
            .filter(|(_, cmd)| {
                if !palette_command_visible(screen, cmd) {
                    return false;
                }
                let mut haystack = format!("{} {}", cmd.category, cmd.description).to_lowercase();
                if let Some(alt) = cmd.description_podcast_list {
                    haystack.push(' ');
                    haystack.push_str(&alt.to_lowercase());
                }
                haystack.contains(&q)
            })
            .map(|(i, _)| i)
            .collect();
        if !self.palette.matches.is_empty() {
            self.palette.list_state.select(Some(0));
        } else {
            self.palette.list_state.select(None);
        }
    }

    /// Append a character to the palette filter query.
    pub fn palette_type(&mut self, c: char) {
        self.palette.query.push(c);
        self.palette_filter();
    }

    /// Remove the last character from the palette filter query.
    pub fn palette_backspace(&mut self) {
        self.palette.query.pop();
        self.palette_filter();
    }

    /// Move the palette selection cursor up.
    pub fn palette_move_up(&mut self) {
        if let Some(i) = self.palette.list_state.selected()
            && i > 0
        {
            self.palette.list_state.select(Some(i - 1));
        }
    }

    /// Move the palette selection cursor down.
    pub fn palette_move_down(&mut self) {
        if let Some(i) = self.palette.list_state.selected()
            && i + 1 < self.palette.matches.len()
        {
            self.palette.list_state.select(Some(i + 1));
        }
    }

    /// Return the action for the currently selected palette command, if any.
    pub fn palette_execute(&self) -> Option<Action> {
        let i = self.palette.list_state.selected()?;
        let cmd_idx = *self.palette.matches.get(i)?;
        Some(COMMANDS[cmd_idx].action)
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app_one_podcast as test_app;
    use super::*;

    #[test]
    fn go_to_podcast_list_hidden_on_podcast_list() {
        let cmd = command_for(Action::GoToPodcastList);
        assert!(!palette_command_visible(Screen::PodcastList, cmd));
        assert!(palette_command_visible(Screen::Config, cmd));
    }

    #[test]
    fn open_config_editor_hidden_on_config() {
        let cmd = command_for(Action::GoToConfig);
        assert!(!palette_command_visible(Screen::Config, cmd));
        assert!(palette_command_visible(Screen::PodcastList, cmd));
    }

    fn command_for(action: Action) -> &'static CommandEntry {
        COMMANDS.iter().find(|c| c.action == action).unwrap()
    }

    #[test]
    fn every_palette_shortcut_comes_from_a_binding_that_runs_that_entry() {
        for cmd in COMMANDS {
            if let Some(binding) = cmd.shortcut {
                assert!(
                    binding.shortcut_for(cmd.action).is_some(),
                    "the shortcut binding for \"{}\" does not run its action",
                    cmd.description
                );
            }
        }
    }

    #[test]
    fn palette_shortcuts_read_as_the_keys_themselves() {
        let app = test_app();
        let shortcut = |action: Action| command_for(action).shortcut_text(&app);
        assert_eq!(shortcut(Action::Quit).as_deref(), Some("q"));
        assert_eq!(shortcut(Action::RefreshFeeds).as_deref(), Some("r"));
        assert_eq!(shortcut(Action::ToggleHintBar).as_deref(), Some("? or ."));
        assert_eq!(shortcut(Action::MovePodcastUp).as_deref(), Some("Shift+↑"));
        assert_eq!(
            shortcut(Action::MovePodcastDown).as_deref(),
            Some("Shift+↓")
        );
        assert_eq!(shortcut(Action::AddPodcast), None);
    }

    #[test]
    fn a_shortcut_is_named_only_on_the_screen_that_binds_it() {
        let mut app = test_app();
        let config = command_for(Action::GoToConfig);
        let refresh = command_for(Action::RefreshFeeds);
        assert_eq!(config.shortcut_text(&app).as_deref(), Some("c"));
        assert_eq!(refresh.shortcut_text(&app).as_deref(), Some("r"));

        app.screen = Screen::EpisodeSelect;
        assert_eq!(
            config.shortcut_text(&app),
            None,
            "c opens the config editor from the podcast list only"
        );
        assert_eq!(refresh.shortcut_text(&app), None);
        assert_eq!(
            command_for(Action::OpenPodcastFolder)
                .shortcut_text(&app)
                .as_deref(),
            Some("o"),
            "o is bound here too, so it keeps its shortcut"
        );

        let mut app = test_app();
        app.enter_config();
        assert_eq!(
            command_for(Action::GoToPodcastList)
                .shortcut_text(&app)
                .as_deref(),
            Some("Esc")
        );
    }

    #[test]
    fn the_palette_names_no_shortcut_over_a_text_field() {
        let mut app = test_app();
        app.enter_config();
        app.config_enter_dir_edit();
        for cmd in COMMANDS {
            assert_eq!(
                cmd.shortcut_text(&app),
                None,
                "\"{}\" must not name a key that would type into the field behind the palette",
                cmd.description
            );
        }
    }

    #[test]
    fn an_open_palette_does_not_hide_the_quit_shortcut() {
        let mut app = test_app();
        app.palette_open();
        assert_eq!(
            command_for(Action::Quit).shortcut_text(&app).as_deref(),
            Some("q")
        );
        assert_eq!(
            command_for(Action::ToggleHintBar)
                .shortcut_text(&app)
                .as_deref(),
            Some("? or .")
        );
    }

    #[test]
    fn palette_type_and_backspace() {
        let mut app = test_app();
        app.palette_open();
        let initial_count = app.palette.matches.len();
        app.palette_type('q');
        assert_eq!(app.palette.query, "q");
        assert!(app.palette.matches.len() <= initial_count);
        app.palette_backspace();
        assert_eq!(app.palette.query, "");
        assert_eq!(app.palette.matches.len(), initial_count);
    }

    #[test]
    fn palette_execute_returns_action() {
        let mut app = test_app();
        app.palette_open();
        let action = app.palette_execute();
        assert!(action.is_some());
    }
}
