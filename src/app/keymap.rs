//! Single source of truth for TUI key bindings.
//!
//! Every binding is declared once in this module: the keys that trigger it, the
//! label the hint bar shows for it, the [`Action`] it produces, and the guards
//! that decide when it applies. [`context_bindings`] then lists the bindings a
//! [`KeyContext`] offers, in the order the hint bar shows them — the bar
//! collapses left to right, so that order is part of the screen's design.
//!
//! `tui.rs` dispatches by resolving a key event to an `Action`, `ui/widgets.rs`
//! renders the hint bar from [`hints`], and `commands.rs` derives its palette
//! shortcut text from the same bindings. Rebinding a key means editing the
//! `keys` of one binding here.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::add_podcast_wizard::{WIZARD_STEPS, WizardStepKind};
use super::state::{
    App, ConfigDialog, ConfigEditMode, PODCAST_FIELD_BOOL_START, PODCAST_FIELD_USIZE,
    PodcastEditorTarget, Screen,
};

/// A key press: a key code plus the modifiers that must accompany it.
///
/// Matching ignores Shift for `Char` codes, because the shifted character is
/// already part of the code (`D` and `d` are different `Char`s), and demands an
/// exact match for every other code so that `Shift+↑` never resolves as `↑`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
    pub shift: bool,
}

impl Key {
    /// A key with no modifiers.
    pub const fn plain(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            shift: false,
        }
    }

    /// An unmodified character key.
    pub const fn ch(c: char) -> Self {
        Self::plain(KeyCode::Char(c))
    }

    /// A key held with Control.
    pub const fn ctrl(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: true,
            shift: false,
        }
    }

    /// A key held with Shift.
    pub const fn shift(code: KeyCode) -> Self {
        Self {
            code,
            ctrl: false,
            shift: true,
        }
    }

    /// Whether this key describes the given crossterm event.
    pub fn matches(&self, event: &KeyEvent) -> bool {
        if self.code != event.code {
            return false;
        }
        if event.modifiers.contains(KeyModifiers::CONTROL) != self.ctrl {
            return false;
        }
        if matches!(self.code, KeyCode::Char(_)) {
            return true;
        }
        event.modifiers.contains(KeyModifiers::SHIFT) == self.shift
    }
}

/// Render a key the way the hint bar and the command palette spell it.
pub fn key_display(key: Key) -> String {
    let base = match key.code {
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) if key.ctrl => c.to_ascii_uppercase().to_string(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "Shift+Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        other => format!("{:?}", other),
    };
    let mut out = String::new();
    if key.ctrl {
        out.push_str("Ctrl+");
    }
    if key.shift {
        out.push_str("Shift+");
    }
    out.push_str(&base);
    out
}

/// Everything a key press can ask the application to do.
///
/// The command palette runs the same variants the keys produce, so an entry and
/// its shortcut cannot drift apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    ForceQuit,
    Quit,
    TogglePalette,
    ToggleHintBar,

    GoToConfig,
    GoToPodcastList,
    RefreshFeeds,
    AddPodcast,
    OpenDownloadFolder,
    OpenPodcastFolder,
    MovePodcastUp,
    MovePodcastDown,

    PodcastUp,
    PodcastDown,
    OpenEpisodeList,
    EditSelectedPodcast,
    PromptDeletePodcast,
    ConfirmDeletePodcast,
    CancelDeletePodcast,

    EpisodeUp,
    EpisodeDown,
    ToggleEpisode,
    ToggleAllEpisodes,
    ToggleSortOrder,
    StartDownload,
    GoBack,

    ConfirmCreateFolder,
    CancelCreateFolder,

    ConfigMenuUp,
    ConfigMenuDown,
    ConfigActivateRow,
    ConfigRestoreDirDefault,
    ConfigCommitDirEdit,
    ConfigCancelDirEdit,
    ConfigConfirmDirChange,
    ConfigCancelDirChange,
    ConfigConfirmDirDefault,
    ConfigCancelDirDefault,

    EditSelectUp,
    EditSelectDown,
    OpenPodcastEditor,

    EditorUp,
    EditorDown,
    EditorActivateField,
    EditorToggleBool,
    EditorIncrement,
    EditorDecrement,
    EditorRestoreField,
    EditorResetAll,
    EditorSave,
    EditorBack,
    EditorCommitEdit,
    EditorCancelEdit,
    EditorConfirmSave,
    EditorConfirmDiscard,
    EditorConfirmCancel,

    WizardChooseManual,
    WizardPrepopulate,
    WizardCancelModeSelect,
    WizardCancelFetch,
    WizardTextNext,
    WizardTextBack,
    WizardNext,
    WizardPrev,
    WizardToggleBool,
    WizardIncrement,
    WizardDecrement,
    WizardCancel,

    Paste,
}

/// The label the hint bar shows for a binding.
pub enum HintLabel {
    Static(&'static str),
    Dynamic(fn(&App) -> &'static str),
}

impl HintLabel {
    fn resolve(&self, app: &App) -> &'static str {
        match self {
            HintLabel::Static(text) => text,
            HintLabel::Dynamic(f) => f(app),
        }
    }
}

/// One binding: its keys, its hint, its actions, and its guards.
///
/// A binding may carry several keys, and those keys may produce different
/// actions where the hint bar presents them as one entry — `Shift+↑/↓ Reorder`
/// is a single hint over two actions.
pub struct Binding {
    /// How the hint bar spells the keys. `None` derives the text from `keys`.
    pub display: Option<&'static str>,
    /// The hint text. `None` for bindings that dispatch but are never hinted.
    pub label: Option<HintLabel>,
    pub keys: &'static [(Key, Action)],
    /// When the keys dispatch. `None` means always.
    pub enabled: Option<fn(&App) -> bool>,
    /// When the hint is shown. `None` follows `enabled`.
    pub hint_when: Option<fn(&App) -> bool>,
}

impl Binding {
    /// Whether this binding's keys currently dispatch.
    pub fn is_enabled(&self, app: &App) -> bool {
        self.enabled.map(|f| f(app)).unwrap_or(true)
    }

    fn hint_visible(&self, app: &App) -> bool {
        match self.hint_when {
            Some(f) => f(app),
            None => self.is_enabled(app),
        }
    }

    /// Whether this binding's keys would act on the screen the palette covers.
    ///
    /// The palette is itself a text field, so the guards that keep `q` and
    /// `?` out of one answer "no" for as long as it is open. The palette asks
    /// this instead, of the screen behind it: is the binding wired there, and
    /// is that screen not a text field of its own?
    pub fn is_live_on_screen(&self, app: &App) -> bool {
        let context = app.key_context();
        !context.is_text_input()
            && GLOBALS
                .iter()
                .chain(context_bindings(context))
                .any(|binding| std::ptr::eq(*binding as *const Binding, self as *const Binding))
    }

    /// How the palette spells the keys of this binding that run `action`.
    pub fn shortcut_for(&self, action: Action) -> Option<String> {
        let text = self
            .keys
            .iter()
            .filter(|(_, bound)| *bound == action)
            .map(|(key, _)| key_display(*key))
            .collect::<Vec<_>>()
            .join(" or ");
        (!text.is_empty()).then_some(text)
    }

    fn hint(&self, app: &App) -> Option<Hint> {
        let label = self.label.as_ref()?;
        if !self.hint_visible(app) {
            return None;
        }
        let key = match self.display {
            Some(text) => text.to_string(),
            None => self
                .keys
                .iter()
                .map(|(key, _)| key_display(*key))
                .collect::<Vec<_>>()
                .join(" or "),
        };
        Some(Hint {
            key,
            label: label.resolve(app),
        })
    }
}

/// One rendered entry of the key-hint bar.
pub struct Hint {
    pub key: String,
    pub label: &'static str,
}

/// Which set of bindings is live, given the screen and the mode it is in.
///
/// Overlays — the delete prompt, the folder and directory confirmations — get
/// their own context so they can rebind keys, but [`App::hint_context`] maps
/// them back to the screen underneath, which keeps showing its own hint bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyContext {
    PodcastList,
    PodcastDeleteConfirm,
    EpisodeSelectLoading,
    EpisodeSelect,
    Downloading,
    CreateFolderConfirm,
    ConfigMenu,
    ConfigDirEdit,
    ConfigDirChangeConfirm,
    ConfigDirDefaultConfirm,
    EditPodcastSelect,
    EditorNavigate,
    EditorTextEdit,
    EditorDiscardConfirm,
    WizardLoading,
    WizardModeSelect,
    WizardText,
    WizardBool,
    WizardUsize,
}

/// Every context, for the invariant tests.
///
/// A new [`KeyContext`] belongs here as well as in [`context_bindings`].
#[cfg(test)]
pub const ALL_CONTEXTS: &[KeyContext] = &[
    KeyContext::PodcastList,
    KeyContext::PodcastDeleteConfirm,
    KeyContext::EpisodeSelectLoading,
    KeyContext::EpisodeSelect,
    KeyContext::Downloading,
    KeyContext::CreateFolderConfirm,
    KeyContext::ConfigMenu,
    KeyContext::ConfigDirEdit,
    KeyContext::ConfigDirChangeConfirm,
    KeyContext::ConfigDirDefaultConfirm,
    KeyContext::EditPodcastSelect,
    KeyContext::EditorNavigate,
    KeyContext::EditorTextEdit,
    KeyContext::EditorDiscardConfirm,
    KeyContext::WizardLoading,
    KeyContext::WizardModeSelect,
    KeyContext::WizardText,
    KeyContext::WizardBool,
    KeyContext::WizardUsize,
];

impl KeyContext {
    /// Whether unresolved character keys type into a text buffer here.
    pub fn is_text_input(self) -> bool {
        matches!(
            self,
            KeyContext::ConfigDirEdit | KeyContext::EditorTextEdit | KeyContext::WizardText
        )
    }
}

fn not_typing(app: &App) -> bool {
    !app.palette.open && !app.key_context().is_text_input()
}

fn always(_: &App) -> bool {
    true
}

fn download_finished(app: &App) -> bool {
    app.download_progress.finished
}

fn config_row_is_download_dir(app: &App) -> bool {
    app.config_editor.menu_state.selected() == Some(1)
}

fn editor_field_is_bool(app: &App) -> bool {
    let field = app.podcast_editor.list_state.selected().unwrap_or(0);
    (PODCAST_FIELD_BOOL_START..PODCAST_FIELD_USIZE).contains(&field)
}

fn editor_field_is_usize(app: &App) -> bool {
    app.podcast_editor.list_state.selected() == Some(PODCAST_FIELD_USIZE)
}

fn editor_edits_template(app: &App) -> bool {
    matches!(app.podcast_editor.target, PodcastEditorTarget::Template)
}

fn wizard_past_first_step(app: &App) -> bool {
    app.add_podcast.step > 0
}

fn sort_order_label(app: &App) -> &'static str {
    app.sort_order.label()
}

fn wizard_next_label(app: &App) -> &'static str {
    if app.add_podcast_on_last_visible_step() {
        "Save"
    } else {
        "Next"
    }
}

pub static FORCE_QUIT: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::ctrl(KeyCode::Char('c')), Action::ForceQuit)],
    enabled: None,
    hint_when: None,
};

pub static QUIT: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Quit")),
    keys: &[(Key::ch('q'), Action::Quit)],
    enabled: Some(not_typing),
    hint_when: Some(always),
};

pub static COMMAND_PALETTE: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Command Palette")),
    keys: &[(Key::ctrl(KeyCode::Char('k')), Action::TogglePalette)],
    enabled: None,
    hint_when: None,
};

pub static TOGGLE_HINT_BAR: Binding = Binding {
    display: None,
    label: None,
    keys: &[
        (Key::ch('?'), Action::ToggleHintBar),
        (Key::ch('.'), Action::ToggleHintBar),
    ],
    enabled: Some(not_typing),
    hint_when: None,
};

/// Bindings that resolve on every screen, before the current context.
pub static GLOBALS: &[&Binding] = &[&FORCE_QUIT, &COMMAND_PALETTE, &QUIT, &TOGGLE_HINT_BAR];

static PASTE: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Paste")),
    keys: &[(Key::ctrl(KeyCode::Char('v')), Action::Paste)],
    enabled: None,
    hint_when: None,
};

static TYPE_TO_EDIT: Binding = Binding {
    display: Some("Type"),
    label: Some(HintLabel::Static("Edit")),
    keys: &[],
    enabled: None,
    hint_when: Some(always),
};

static NAVIGATE_PODCASTS: Binding = Binding {
    display: Some("↑/↓"),
    label: Some(HintLabel::Static("Navigate")),
    keys: &[
        (Key::plain(KeyCode::Up), Action::PodcastUp),
        (Key::ch('k'), Action::PodcastUp),
        (Key::plain(KeyCode::Down), Action::PodcastDown),
        (Key::ch('j'), Action::PodcastDown),
    ],
    enabled: None,
    hint_when: None,
};

static OPEN_EPISODE_LIST: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Select")),
    keys: &[(Key::plain(KeyCode::Enter), Action::OpenEpisodeList)],
    enabled: None,
    hint_when: None,
};

pub static REFRESH_FEEDS: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Refresh")),
    keys: &[(Key::ch('r'), Action::RefreshFeeds)],
    enabled: None,
    hint_when: None,
};

pub static GO_TO_CONFIG: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Config")),
    keys: &[(Key::ch('c'), Action::GoToConfig)],
    enabled: None,
    hint_when: None,
};

pub static REORDER_PODCASTS: Binding = Binding {
    display: Some("Shift+↑/↓"),
    label: Some(HintLabel::Static("Reorder")),
    keys: &[
        (Key::shift(KeyCode::Up), Action::MovePodcastUp),
        (Key::shift(KeyCode::Down), Action::MovePodcastDown),
    ],
    enabled: None,
    hint_when: None,
};

pub static OPEN_PODCAST_FOLDER: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Open folder")),
    keys: &[(Key::ch('o'), Action::OpenPodcastFolder)],
    enabled: None,
    hint_when: None,
};

static EDIT_SELECTED_PODCAST: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Edit")),
    keys: &[(Key::ch('e'), Action::EditSelectedPodcast)],
    enabled: None,
    hint_when: None,
};

static DELETE_PODCAST: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Delete")),
    keys: &[(Key::ch('D'), Action::PromptDeletePodcast)],
    enabled: None,
    hint_when: None,
};

static CONFIRM_DELETE_PODCAST: Binding = Binding {
    display: None,
    label: None,
    keys: &[
        (Key::ch('y'), Action::ConfirmDeletePodcast),
        (Key::ch('D'), Action::ConfirmDeletePodcast),
    ],
    enabled: None,
    hint_when: None,
};

static CANCEL_DELETE_PODCAST: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::plain(KeyCode::Esc), Action::CancelDeletePodcast)],
    enabled: None,
    hint_when: None,
};

static NAVIGATE_EPISODES: Binding = Binding {
    display: Some("↑/↓"),
    label: Some(HintLabel::Static("Navigate")),
    keys: &[
        (Key::plain(KeyCode::Up), Action::EpisodeUp),
        (Key::ch('k'), Action::EpisodeUp),
        (Key::plain(KeyCode::Down), Action::EpisodeDown),
        (Key::ch('j'), Action::EpisodeDown),
    ],
    enabled: None,
    hint_when: None,
};

static TOGGLE_EPISODE: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Toggle")),
    keys: &[(Key::ch(' '), Action::ToggleEpisode)],
    enabled: None,
    hint_when: None,
};

static TOGGLE_ALL_EPISODES: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("All")),
    keys: &[(Key::ch('a'), Action::ToggleAllEpisodes)],
    enabled: None,
    hint_when: None,
};

static TOGGLE_SORT_ORDER: Binding = Binding {
    display: None,
    label: Some(HintLabel::Dynamic(sort_order_label)),
    keys: &[(Key::ch('s'), Action::ToggleSortOrder)],
    enabled: None,
    hint_when: None,
};

static START_DOWNLOAD: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Download")),
    keys: &[(Key::plain(KeyCode::Enter), Action::StartDownload)],
    enabled: None,
    hint_when: None,
};

static EPISODES_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Esc), Action::GoBack)],
    enabled: None,
    hint_when: None,
};

static DOWNLOAD_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back to list")),
    keys: &[(Key::plain(KeyCode::Esc), Action::GoBack)],
    enabled: Some(download_finished),
    hint_when: None,
};

static CONFIRM_CREATE_FOLDER: Binding = Binding {
    display: None,
    label: None,
    keys: &[
        (Key::ch('y'), Action::ConfirmCreateFolder),
        (Key::plain(KeyCode::Enter), Action::ConfirmCreateFolder),
    ],
    enabled: None,
    hint_when: None,
};

static CANCEL_CREATE_FOLDER: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::plain(KeyCode::Esc), Action::CancelCreateFolder)],
    enabled: None,
    hint_when: None,
};

static NAVIGATE_CONFIG_MENU: Binding = Binding {
    display: Some("↑/↓"),
    label: Some(HintLabel::Static("Navigate")),
    keys: &[
        (Key::plain(KeyCode::Up), Action::ConfigMenuUp),
        (Key::ch('k'), Action::ConfigMenuUp),
        (Key::plain(KeyCode::Down), Action::ConfigMenuDown),
        (Key::ch('j'), Action::ConfigMenuDown),
    ],
    enabled: None,
    hint_when: None,
};

static CONFIG_ACTIVATE_ROW: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Select")),
    keys: &[(Key::plain(KeyCode::Enter), Action::ConfigActivateRow)],
    enabled: None,
    hint_when: None,
};

pub static CONFIG_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Esc), Action::GoToPodcastList)],
    enabled: None,
    hint_when: None,
};

static CONFIG_RESTORE_DIR: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::ch('r'), Action::ConfigRestoreDirDefault)],
    enabled: Some(config_row_is_download_dir),
    hint_when: None,
};

static CONFIG_COMMIT_DIR: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Confirm")),
    keys: &[(Key::plain(KeyCode::Enter), Action::ConfigCommitDirEdit)],
    enabled: None,
    hint_when: None,
};

static CONFIG_CANCEL_DIR_EDIT: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Cancel")),
    keys: &[(Key::plain(KeyCode::Esc), Action::ConfigCancelDirEdit)],
    enabled: None,
    hint_when: None,
};

static CONFIRM_DIR_CHANGE: Binding = Binding {
    display: None,
    label: None,
    keys: &[
        (Key::ch('y'), Action::ConfigConfirmDirChange),
        (Key::plain(KeyCode::Enter), Action::ConfigConfirmDirChange),
    ],
    enabled: None,
    hint_when: None,
};

static CANCEL_DIR_CHANGE: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::plain(KeyCode::Esc), Action::ConfigCancelDirChange)],
    enabled: None,
    hint_when: None,
};

static CONFIRM_DIR_DEFAULT: Binding = Binding {
    display: None,
    label: None,
    keys: &[
        (Key::ch('y'), Action::ConfigConfirmDirDefault),
        (Key::plain(KeyCode::Enter), Action::ConfigConfirmDirDefault),
    ],
    enabled: None,
    hint_when: None,
};

static CANCEL_DIR_DEFAULT: Binding = Binding {
    display: None,
    label: None,
    keys: &[(Key::plain(KeyCode::Esc), Action::ConfigCancelDirDefault)],
    enabled: None,
    hint_when: None,
};

static NAVIGATE_EDIT_SELECT: Binding = Binding {
    display: Some("↑/↓"),
    label: Some(HintLabel::Static("Navigate")),
    keys: &[
        (Key::plain(KeyCode::Up), Action::EditSelectUp),
        (Key::ch('k'), Action::EditSelectUp),
        (Key::plain(KeyCode::Down), Action::EditSelectDown),
        (Key::ch('j'), Action::EditSelectDown),
    ],
    enabled: None,
    hint_when: None,
};

static OPEN_PODCAST_EDITOR: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Edit")),
    keys: &[(Key::plain(KeyCode::Enter), Action::OpenPodcastEditor)],
    enabled: None,
    hint_when: None,
};

static EDIT_SELECT_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Esc), Action::GoToConfig)],
    enabled: None,
    hint_when: None,
};

static NAVIGATE_EDITOR: Binding = Binding {
    display: Some("↑/↓"),
    label: Some(HintLabel::Static("Navigate")),
    keys: &[
        (Key::plain(KeyCode::Up), Action::EditorUp),
        (Key::ch('k'), Action::EditorUp),
        (Key::plain(KeyCode::Down), Action::EditorDown),
        (Key::ch('j'), Action::EditorDown),
    ],
    enabled: None,
    hint_when: None,
};

static EDITOR_ACTIVATE_FIELD: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Edit")),
    keys: &[(Key::plain(KeyCode::Enter), Action::EditorActivateField)],
    enabled: None,
    hint_when: None,
};

static EDITOR_TOGGLE_BOOL: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Toggle")),
    keys: &[(Key::ch(' '), Action::EditorToggleBool)],
    enabled: Some(editor_field_is_bool),
    hint_when: Some(always),
};

static EDITOR_ADJUST: Binding = Binding {
    display: Some("+/-"),
    label: Some(HintLabel::Static("Adjust")),
    keys: &[
        (Key::plain(KeyCode::Left), Action::EditorDecrement),
        (Key::ch('-'), Action::EditorDecrement),
        (Key::plain(KeyCode::Right), Action::EditorIncrement),
        (Key::ch('+'), Action::EditorIncrement),
    ],
    enabled: Some(editor_field_is_usize),
    hint_when: None,
};

static EDITOR_RESTORE_FIELD: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Reset field")),
    keys: &[(Key::ch('r'), Action::EditorRestoreField)],
    enabled: None,
    hint_when: None,
};

static EDITOR_RESET_ALL: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Reset all")),
    keys: &[(Key::ch('R'), Action::EditorResetAll)],
    enabled: Some(editor_edits_template),
    hint_when: None,
};

static EDITOR_SAVE: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Save")),
    keys: &[(Key::ch('s'), Action::EditorSave)],
    enabled: None,
    hint_when: None,
};

static EDITOR_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Esc), Action::EditorBack)],
    enabled: None,
    hint_when: None,
};

static EDITOR_COMMIT_EDIT: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Confirm")),
    keys: &[(Key::plain(KeyCode::Enter), Action::EditorCommitEdit)],
    enabled: None,
    hint_when: None,
};

static EDITOR_CANCEL_EDIT: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Cancel")),
    keys: &[(Key::plain(KeyCode::Esc), Action::EditorCancelEdit)],
    enabled: None,
    hint_when: None,
};

static EDITOR_CONFIRM_SAVE: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Save")),
    keys: &[(Key::ch('s'), Action::EditorConfirmSave)],
    enabled: None,
    hint_when: None,
};

static EDITOR_CONFIRM_DISCARD: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Discard")),
    keys: &[(Key::ch('d'), Action::EditorConfirmDiscard)],
    enabled: None,
    hint_when: None,
};

static EDITOR_CONFIRM_CANCEL: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Cancel")),
    keys: &[(Key::plain(KeyCode::Esc), Action::EditorConfirmCancel)],
    enabled: None,
    hint_when: None,
};

static WIZARD_CANCEL_FETCH: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Cancel")),
    keys: &[(Key::plain(KeyCode::Esc), Action::WizardCancelFetch)],
    enabled: None,
    hint_when: None,
};

static WIZARD_MANUAL: Binding = Binding {
    display: Some("1/m"),
    label: Some(HintLabel::Static("Manual")),
    keys: &[
        (Key::ch('1'), Action::WizardChooseManual),
        (Key::ch('m'), Action::WizardChooseManual),
    ],
    enabled: None,
    hint_when: None,
};

static WIZARD_PREPOPULATE: Binding = Binding {
    display: Some("2/p"),
    label: Some(HintLabel::Static("Prepopulate from feed")),
    keys: &[
        (Key::ch('2'), Action::WizardPrepopulate),
        (Key::ch('p'), Action::WizardPrepopulate),
        (Key::plain(KeyCode::Enter), Action::WizardPrepopulate),
    ],
    enabled: None,
    hint_when: None,
};

static WIZARD_MODE_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Esc), Action::WizardCancelModeSelect)],
    enabled: None,
    hint_when: None,
};

static WIZARD_TEXT_NEXT: Binding = Binding {
    display: None,
    label: Some(HintLabel::Dynamic(wizard_next_label)),
    keys: &[(Key::plain(KeyCode::Enter), Action::WizardTextNext)],
    enabled: None,
    hint_when: None,
};

static WIZARD_TEXT_BACK: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Back")),
    keys: &[(Key::plain(KeyCode::Backspace), Action::WizardTextBack)],
    enabled: None,
    hint_when: Some(wizard_past_first_step),
};

static WIZARD_TOGGLE_BOOL: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Toggle")),
    keys: &[(Key::ch(' '), Action::WizardToggleBool)],
    enabled: None,
    hint_when: None,
};

static WIZARD_ADJUST: Binding = Binding {
    display: Some("←/→"),
    label: Some(HintLabel::Static("Adjust")),
    keys: &[
        (Key::plain(KeyCode::Left), Action::WizardDecrement),
        (Key::ch('-'), Action::WizardDecrement),
        (Key::plain(KeyCode::Right), Action::WizardIncrement),
        (Key::ch('+'), Action::WizardIncrement),
    ],
    enabled: None,
    hint_when: None,
};

static WIZARD_NEXT: Binding = Binding {
    display: Some("Enter"),
    label: Some(HintLabel::Dynamic(wizard_next_label)),
    keys: &[
        (Key::plain(KeyCode::Enter), Action::WizardNext),
        (Key::plain(KeyCode::Down), Action::WizardNext),
        (Key::ch('j'), Action::WizardNext),
        (Key::plain(KeyCode::Tab), Action::WizardNext),
    ],
    enabled: None,
    hint_when: None,
};

static WIZARD_PREV: Binding = Binding {
    display: Some("Backspace"),
    label: Some(HintLabel::Static("Back")),
    keys: &[
        (Key::plain(KeyCode::Backspace), Action::WizardPrev),
        (Key::plain(KeyCode::Up), Action::WizardPrev),
        (Key::ch('k'), Action::WizardPrev),
    ],
    enabled: None,
    hint_when: Some(wizard_past_first_step),
};

static WIZARD_CANCEL: Binding = Binding {
    display: None,
    label: Some(HintLabel::Static("Cancel")),
    keys: &[(Key::plain(KeyCode::Esc), Action::WizardCancel)],
    enabled: None,
    hint_when: None,
};

static PODCAST_LIST_BINDINGS: &[&Binding] = &[
    &NAVIGATE_PODCASTS,
    &OPEN_EPISODE_LIST,
    &REFRESH_FEEDS,
    &COMMAND_PALETTE,
    &GO_TO_CONFIG,
    &QUIT,
    &REORDER_PODCASTS,
    &OPEN_PODCAST_FOLDER,
    &EDIT_SELECTED_PODCAST,
    &DELETE_PODCAST,
];

static PODCAST_DELETE_CONFIRM_BINDINGS: &[&Binding] =
    &[&CONFIRM_DELETE_PODCAST, &CANCEL_DELETE_PODCAST];

static EPISODE_SELECT_LOADING_BINDINGS: &[&Binding] = &[
    &EPISODES_BACK,
    &QUIT,
    &COMMAND_PALETTE,
    &OPEN_PODCAST_FOLDER,
];

static EPISODE_SELECT_BINDINGS: &[&Binding] = &[
    &NAVIGATE_EPISODES,
    &TOGGLE_EPISODE,
    &TOGGLE_ALL_EPISODES,
    &TOGGLE_SORT_ORDER,
    &START_DOWNLOAD,
    &COMMAND_PALETTE,
    &EPISODES_BACK,
    &QUIT,
    &OPEN_PODCAST_FOLDER,
];

static DOWNLOADING_BINDINGS: &[&Binding] = &[&DOWNLOAD_BACK];

static CREATE_FOLDER_CONFIRM_BINDINGS: &[&Binding] =
    &[&CONFIRM_CREATE_FOLDER, &CANCEL_CREATE_FOLDER];

static CONFIG_MENU_BINDINGS: &[&Binding] = &[
    &NAVIGATE_CONFIG_MENU,
    &CONFIG_ACTIVATE_ROW,
    &CONFIG_BACK,
    &COMMAND_PALETTE,
    &CONFIG_RESTORE_DIR,
];

static CONFIG_DIR_EDIT_BINDINGS: &[&Binding] = &[
    &TYPE_TO_EDIT,
    &PASTE,
    &CONFIG_COMMIT_DIR,
    &CONFIG_CANCEL_DIR_EDIT,
];

static CONFIG_DIR_CHANGE_BINDINGS: &[&Binding] = &[&CONFIRM_DIR_CHANGE, &CANCEL_DIR_CHANGE];

static CONFIG_DIR_DEFAULT_BINDINGS: &[&Binding] = &[&CONFIRM_DIR_DEFAULT, &CANCEL_DIR_DEFAULT];

static EDIT_PODCAST_SELECT_BINDINGS: &[&Binding] = &[
    &NAVIGATE_EDIT_SELECT,
    &OPEN_PODCAST_EDITOR,
    &EDIT_SELECT_BACK,
    &COMMAND_PALETTE,
];

static EDITOR_NAVIGATE_BINDINGS: &[&Binding] = &[
    &NAVIGATE_EDITOR,
    &EDITOR_ACTIVATE_FIELD,
    &EDITOR_TOGGLE_BOOL,
    &EDITOR_ADJUST,
    &EDITOR_RESTORE_FIELD,
    &EDITOR_RESET_ALL,
    &EDITOR_SAVE,
    &EDITOR_BACK,
    &COMMAND_PALETTE,
];

static EDITOR_TEXT_EDIT_BINDINGS: &[&Binding] = &[
    &TYPE_TO_EDIT,
    &PASTE,
    &EDITOR_COMMIT_EDIT,
    &EDITOR_CANCEL_EDIT,
];

static EDITOR_DISCARD_CONFIRM_BINDINGS: &[&Binding] = &[
    &EDITOR_CONFIRM_SAVE,
    &EDITOR_CONFIRM_DISCARD,
    &EDITOR_CONFIRM_CANCEL,
];

static WIZARD_LOADING_BINDINGS: &[&Binding] = &[&WIZARD_CANCEL_FETCH];

static WIZARD_MODE_SELECT_BINDINGS: &[&Binding] =
    &[&WIZARD_MANUAL, &WIZARD_PREPOPULATE, &WIZARD_MODE_BACK];

static WIZARD_TEXT_BINDINGS: &[&Binding] = &[
    &TYPE_TO_EDIT,
    &PASTE,
    &WIZARD_TEXT_NEXT,
    &WIZARD_TEXT_BACK,
    &WIZARD_CANCEL,
];

static WIZARD_BOOL_BINDINGS: &[&Binding] = &[
    &WIZARD_TOGGLE_BOOL,
    &WIZARD_NEXT,
    &WIZARD_PREV,
    &WIZARD_CANCEL,
];

static WIZARD_USIZE_BINDINGS: &[&Binding] =
    &[&WIZARD_ADJUST, &WIZARD_NEXT, &WIZARD_PREV, &WIZARD_CANCEL];

/// The bindings a context offers, in the order the hint bar shows them.
pub fn context_bindings(context: KeyContext) -> &'static [&'static Binding] {
    match context {
        KeyContext::PodcastList => PODCAST_LIST_BINDINGS,
        KeyContext::PodcastDeleteConfirm => PODCAST_DELETE_CONFIRM_BINDINGS,
        KeyContext::EpisodeSelectLoading => EPISODE_SELECT_LOADING_BINDINGS,
        KeyContext::EpisodeSelect => EPISODE_SELECT_BINDINGS,
        KeyContext::Downloading => DOWNLOADING_BINDINGS,
        KeyContext::CreateFolderConfirm => CREATE_FOLDER_CONFIRM_BINDINGS,
        KeyContext::ConfigMenu => CONFIG_MENU_BINDINGS,
        KeyContext::ConfigDirEdit => CONFIG_DIR_EDIT_BINDINGS,
        KeyContext::ConfigDirChangeConfirm => CONFIG_DIR_CHANGE_BINDINGS,
        KeyContext::ConfigDirDefaultConfirm => CONFIG_DIR_DEFAULT_BINDINGS,
        KeyContext::EditPodcastSelect => EDIT_PODCAST_SELECT_BINDINGS,
        KeyContext::EditorNavigate => EDITOR_NAVIGATE_BINDINGS,
        KeyContext::EditorTextEdit => EDITOR_TEXT_EDIT_BINDINGS,
        KeyContext::EditorDiscardConfirm => EDITOR_DISCARD_CONFIRM_BINDINGS,
        KeyContext::WizardLoading => WIZARD_LOADING_BINDINGS,
        KeyContext::WizardModeSelect => WIZARD_MODE_SELECT_BINDINGS,
        KeyContext::WizardText => WIZARD_TEXT_BINDINGS,
        KeyContext::WizardBool => WIZARD_BOOL_BINDINGS,
        KeyContext::WizardUsize => WIZARD_USIZE_BINDINGS,
    }
}

fn resolve_in(bindings: &[&Binding], app: &App, event: &KeyEvent) -> Option<Action> {
    bindings.iter().find_map(|binding| {
        if !binding.is_enabled(app) {
            return None;
        }
        binding
            .keys
            .iter()
            .find(|(key, _)| key.matches(event))
            .map(|(_, action)| *action)
    })
}

/// Resolve a key against the bindings that apply on every screen.
pub fn resolve_global(app: &App, event: &KeyEvent) -> Option<Action> {
    resolve_in(GLOBALS, app, event)
}

/// Resolve a key against the current context's bindings.
pub fn resolve_context(app: &App, event: &KeyEvent) -> Option<Action> {
    resolve_in(context_bindings(app.key_context()), app, event)
}

/// The hint-bar entries for the screen on show, in collapse order.
pub fn hints(app: &App) -> Vec<Hint> {
    context_bindings(app.hint_context())
        .iter()
        .filter_map(|binding| binding.hint(app))
        .collect()
}

/// The key the hint bar prints on its own expand and collapse indicator.
pub fn hint_bar_toggle_key() -> String {
    key_display(TOGGLE_HINT_BAR.keys[0].0)
}

impl App {
    /// The bindings that are live right now, including any open overlay.
    pub fn key_context(&self) -> KeyContext {
        if self.folder_prompt_open() {
            return KeyContext::CreateFolderConfirm;
        }
        match self.screen {
            Screen::PodcastList if self.podcast_delete_pending.is_some() => {
                KeyContext::PodcastDeleteConfirm
            }
            Screen::Config => match self.config_editor.dialog {
                ConfigDialog::ConfirmDirChange => KeyContext::ConfigDirChangeConfirm,
                ConfigDialog::ConfirmDirDefault => KeyContext::ConfigDirDefaultConfirm,
                ConfigDialog::ConfirmCreateDir | ConfigDialog::Closed => self.hint_context(),
            },
            _ => self.hint_context(),
        }
    }

    /// Whether the create-folder prompt is on screen.
    ///
    /// Every screen but the config menu draws that prompt from
    /// `pending_open_folder` alone. The config menu draws it as one of its own
    /// dialogs, so a pending folder carried in from another screen — the
    /// palette can navigate here while one is set — shows nothing there and
    /// must not take the config menu's keys.
    fn folder_prompt_open(&self) -> bool {
        self.pending_open_folder.is_some()
            && (self.screen != Screen::Config
                || self.config_editor.dialog == ConfigDialog::ConfirmCreateDir)
    }

    /// The context whose hint bar the screen shows.
    ///
    /// Overlays that leave the screen behind them visible — the delete prompt,
    /// the folder and directory confirmations — keep the underlying hint bar.
    pub fn hint_context(&self) -> KeyContext {
        match self.screen {
            Screen::PodcastList => KeyContext::PodcastList,
            Screen::EpisodeSelect => {
                if self.loading_episodes || self.episode_load_error.is_some() {
                    KeyContext::EpisodeSelectLoading
                } else {
                    KeyContext::EpisodeSelect
                }
            }
            Screen::Downloading => KeyContext::Downloading,
            Screen::Config => match self.config_editor.edit_mode {
                ConfigEditMode::EditingText => KeyContext::ConfigDirEdit,
                ConfigEditMode::Navigate => KeyContext::ConfigMenu,
            },
            Screen::EditPodcastSelect => KeyContext::EditPodcastSelect,
            Screen::EditPodcast => {
                if self.podcast_editor.show_confirm_discard {
                    KeyContext::EditorDiscardConfirm
                } else {
                    match self.podcast_editor.edit_mode {
                        ConfigEditMode::EditingText => KeyContext::EditorTextEdit,
                        ConfigEditMode::Navigate => KeyContext::EditorNavigate,
                    }
                }
            }
            Screen::AddPodcast => {
                if self.add_podcast.loading_feed_info {
                    KeyContext::WizardLoading
                } else if self.add_podcast.mode_select {
                    KeyContext::WizardModeSelect
                } else {
                    match WIZARD_STEPS[self.add_podcast.step].kind {
                        WizardStepKind::Text => KeyContext::WizardText,
                        WizardStepKind::Bool => KeyContext::WizardBool,
                        WizardStepKind::Usize => KeyContext::WizardUsize,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app_one_podcast as test_app;
    use super::*;
    use crate::app::PODCAST_FIELD_COUNT;

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn press_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn shown(app: &App) -> Vec<(String, &'static str)> {
        hints(app).into_iter().map(|h| (h.key, h.label)).collect()
    }

    fn assert_hints(app: &App, expected: &[(&str, &str)]) {
        let shown = shown(app);
        let actual: Vec<(&str, &str)> = shown.iter().map(|(k, l)| (k.as_str(), *l)).collect();
        assert_eq!(
            actual,
            expected,
            "the hint bar for {:?} changed — its order is the order it collapses in",
            app.hint_context()
        );
    }

    fn editor_app(target: PodcastEditorTarget) -> App {
        let mut app = test_app();
        match target {
            PodcastEditorTarget::Template => app.enter_template_editor(),
            PodcastEditorTarget::Existing(i) => app.enter_podcast_editor_existing(i),
        }
        app
    }

    fn wizard_app(step: usize) -> App {
        let mut app = test_app();
        app.enter_add_podcast();
        app.add_podcast.podcast.overwrite_tags = true;
        app.add_podcast.podcast.overwrite_title = true;
        app.add_podcast.podcast.append_number_to_title = true;
        app.add_podcast.step = step;
        app
    }

    #[test]
    fn podcast_list_hints() {
        assert_hints(
            &test_app(),
            &[
                ("↑/↓", "Navigate"),
                ("Enter", "Select"),
                ("r", "Refresh"),
                ("Ctrl+K", "Command Palette"),
                ("c", "Config"),
                ("q", "Quit"),
                ("Shift+↑/↓", "Reorder"),
                ("o", "Open folder"),
                ("e", "Edit"),
                ("D", "Delete"),
            ],
        );
    }

    #[test]
    fn episode_select_hints() {
        let mut app = test_app();
        app.screen = Screen::EpisodeSelect;
        app.loading_episodes = true;
        assert_hints(
            &app,
            &[
                ("Esc", "Back"),
                ("q", "Quit"),
                ("Ctrl+K", "Command Palette"),
                ("o", "Open folder"),
            ],
        );
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Char('o'))),
            Some(Action::OpenPodcastFolder),
            "a slow or failed feed must not take the open-folder key away"
        );

        app.episode_load_error = Some("boom".to_string());
        assert_eq!(app.hint_context(), KeyContext::EpisodeSelectLoading);
        app.episode_load_error = None;

        app.loading_episodes = false;
        assert_hints(
            &app,
            &[
                ("↑/↓", "Navigate"),
                ("Space", "Toggle"),
                ("a", "All"),
                ("s", "Newest first"),
                ("Enter", "Download"),
                ("Ctrl+K", "Command Palette"),
                ("Esc", "Back"),
                ("q", "Quit"),
                ("o", "Open folder"),
            ],
        );
    }

    #[test]
    fn the_sort_hint_follows_the_sort_order() {
        let mut app = test_app();
        app.screen = Screen::EpisodeSelect;
        app.toggle_sort_order();
        assert!(shown(&app).contains(&("s".to_string(), "Oldest first")));
    }

    #[test]
    fn downloading_hints_only_once_the_run_has_finished() {
        let mut app = test_app();
        app.start_download();
        assert_hints(&app, &[]);
        app.download_progress.finished = true;
        assert_hints(&app, &[("Esc", "Back to list")]);
    }

    #[test]
    fn config_hints() {
        let mut app = test_app();
        app.enter_config();
        assert_hints(
            &app,
            &[
                ("↑/↓", "Navigate"),
                ("Enter", "Select"),
                ("Esc", "Back"),
                ("Ctrl+K", "Command Palette"),
            ],
        );

        app.config_enter_dir_edit();
        assert_hints(
            &app,
            &[
                ("Type", "Edit"),
                ("Ctrl+V", "Paste"),
                ("Enter", "Confirm"),
                ("Esc", "Cancel"),
            ],
        );
    }

    #[test]
    fn an_open_dialog_keeps_the_hint_bar_of_the_screen_behind_it() {
        let mut app = test_app();
        app.podcast_delete_pending = Some(0);
        assert_eq!(app.hint_context(), KeyContext::PodcastList);
        assert_eq!(app.key_context(), KeyContext::PodcastDeleteConfirm);

        let mut app = test_app();
        app.enter_config();
        app.config_restore_dir_to_default();
        assert_eq!(app.hint_context(), KeyContext::ConfigMenu);
        assert_eq!(app.key_context(), KeyContext::ConfigDirDefaultConfirm);
    }

    #[test]
    fn a_folder_prompt_left_behind_does_not_take_the_config_menu_keys() {
        let mut app = test_app();
        app.pending_open_folder = Some("/tmp/nope".to_string());
        assert_eq!(app.key_context(), KeyContext::CreateFolderConfirm);

        app.enter_config();
        app.pending_open_folder = Some("/tmp/nope".to_string());
        assert_eq!(
            app.key_context(),
            KeyContext::ConfigMenu,
            "the config menu draws no prompt for a pending folder from another screen"
        );
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Enter)),
            Some(Action::ConfigActivateRow)
        );

        app.config_editor.dialog = ConfigDialog::ConfirmCreateDir;
        assert_eq!(app.key_context(), KeyContext::CreateFolderConfirm);
    }

    #[test]
    fn edit_podcast_select_hints() {
        let mut app = test_app();
        app.enter_edit_podcast_select();
        assert_hints(
            &app,
            &[
                ("↑/↓", "Navigate"),
                ("Enter", "Edit"),
                ("Esc", "Back"),
                ("Ctrl+K", "Command Palette"),
            ],
        );
    }

    #[test]
    fn editor_hints() {
        let mut app = editor_app(PodcastEditorTarget::Existing(0));
        assert_hints(
            &app,
            &[
                ("↑/↓", "Navigate"),
                ("Enter", "Edit"),
                ("Space", "Toggle"),
                ("r", "Reset field"),
                ("s", "Save"),
                ("Esc", "Back"),
                ("Ctrl+K", "Command Palette"),
            ],
        );

        app.podcast_editor
            .list_state
            .select(Some(PODCAST_FIELD_USIZE));
        assert!(shown(&app).contains(&("+/-".to_string(), "Adjust")));

        app.podcast_editor.list_state.select(Some(0));
        app.podcast_editor_enter_edit_mode();
        assert_hints(
            &app,
            &[
                ("Type", "Edit"),
                ("Ctrl+V", "Paste"),
                ("Enter", "Confirm"),
                ("Esc", "Cancel"),
            ],
        );

        app.podcast_editor.show_confirm_discard = true;
        assert_hints(&app, &[("s", "Save"), ("d", "Discard"), ("Esc", "Cancel")]);
    }

    #[test]
    fn reset_all_is_offered_on_the_template_only() {
        let template = editor_app(PodcastEditorTarget::Template);
        assert!(shown(&template).contains(&("R".to_string(), "Reset all")));

        let existing = editor_app(PodcastEditorTarget::Existing(0));
        assert!(!shown(&existing).contains(&("R".to_string(), "Reset all")));
    }

    #[test]
    fn wizard_hints() {
        let mut app = wizard_app(0);
        assert_hints(
            &app,
            &[
                ("Type", "Edit"),
                ("Ctrl+V", "Paste"),
                ("Enter", "Next"),
                ("Esc", "Cancel"),
            ],
        );

        app.add_podcast.mode_select = true;
        assert_hints(
            &app,
            &[
                ("1/m", "Manual"),
                ("2/p", "Prepopulate from feed"),
                ("Esc", "Back"),
            ],
        );

        app.add_podcast.mode_select = false;
        app.add_podcast.loading_feed_info = true;
        assert_hints(&app, &[("Esc", "Cancel")]);

        assert_hints(
            &wizard_app(PODCAST_FIELD_BOOL_START),
            &[
                ("Space", "Toggle"),
                ("Enter", "Next"),
                ("Backspace", "Back"),
                ("Esc", "Cancel"),
            ],
        );

        assert_hints(
            &wizard_app(PODCAST_FIELD_USIZE),
            &[
                ("←/→", "Adjust"),
                ("Enter", "Next"),
                ("Backspace", "Back"),
                ("Esc", "Cancel"),
            ],
        );

        assert_hints(
            &wizard_app(PODCAST_FIELD_COUNT - 1),
            &[
                ("Type", "Edit"),
                ("Ctrl+V", "Paste"),
                ("Enter", "Save"),
                ("Backspace", "Back"),
                ("Esc", "Cancel"),
            ],
        );
    }

    #[test]
    fn no_context_binds_the_same_key_twice() {
        for &context in ALL_CONTEXTS {
            let mut seen: Vec<(Key, *const Binding)> = Vec::new();
            for binding in GLOBALS.iter().chain(context_bindings(context)) {
                let owner = *binding as *const Binding;
                for (key, _) in binding.keys {
                    if let Some((_, other)) = seen.iter().find(|(other_key, _)| other_key == key) {
                        assert!(
                            std::ptr::eq(*other, owner),
                            "{:?} binds {} in two different bindings",
                            context,
                            key_display(*key)
                        );
                    }
                    seen.push((*key, owner));
                }
            }
        }
    }

    #[test]
    fn every_context_offers_bindings() {
        for &context in ALL_CONTEXTS {
            assert!(
                !context_bindings(context).is_empty(),
                "{:?} has no bindings",
                context
            );
        }
    }

    #[test]
    fn shift_arrows_reorder_where_plain_arrows_navigate() {
        let app = test_app();
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Up)),
            Some(Action::PodcastUp)
        );
        assert_eq!(
            resolve_context(&app, &press_with(KeyCode::Up, KeyModifiers::SHIFT)),
            Some(Action::MovePodcastUp)
        );
    }

    #[test]
    fn the_delete_prompt_takes_y_and_d_but_never_enter() {
        let mut app = test_app();
        app.podcast_delete_pending = Some(0);
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Char('y'))),
            Some(Action::ConfirmDeletePodcast)
        );
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Char('D'))),
            Some(Action::ConfirmDeletePodcast)
        );
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Enter)),
            None,
            "Enter must not confirm a deletion — it opens a podcast on this screen"
        );
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Esc)),
            Some(Action::CancelDeletePodcast)
        );
    }

    #[test]
    fn the_palette_opens_from_inside_a_text_field_where_q_types() {
        let mut app = test_app();
        app.enter_config();
        app.config_enter_dir_edit();
        assert!(app.key_context().is_text_input());
        assert_eq!(
            resolve_global(&app, &press_with(KeyCode::Char('k'), KeyModifiers::CONTROL)),
            Some(Action::TogglePalette)
        );
        assert_eq!(resolve_global(&app, &press(KeyCode::Char('q'))), None);
        assert_eq!(resolve_global(&app, &press(KeyCode::Char('?'))), None);
    }

    #[test]
    fn q_does_not_quit_while_the_palette_is_open() {
        let mut app = test_app();
        app.palette_open();
        assert_eq!(resolve_global(&app, &press(KeyCode::Char('q'))), None);
        assert_eq!(
            resolve_global(&app, &press_with(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::ForceQuit)
        );
    }

    #[test]
    fn escape_leaves_a_finished_download_but_not_a_running_one() {
        let mut app = test_app();
        app.start_download();
        assert_eq!(resolve_context(&app, &press(KeyCode::Esc)), None);
        app.download_progress.finished = true;
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Esc)),
            Some(Action::GoBack)
        );
    }

    #[test]
    fn the_editor_adjusts_numbers_only_on_the_numeric_field() {
        let mut app = editor_app(PodcastEditorTarget::Existing(0));
        app.podcast_editor.list_state.select(Some(0));
        assert_eq!(resolve_context(&app, &press(KeyCode::Char('+'))), None);
        app.podcast_editor
            .list_state
            .select(Some(PODCAST_FIELD_USIZE));
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Char('+'))),
            Some(Action::EditorIncrement)
        );
    }

    #[test]
    fn the_wizard_backspaces_on_the_first_step_without_hinting_it() {
        let app = wizard_app(0);
        assert_eq!(
            resolve_context(&app, &press(KeyCode::Backspace)),
            Some(Action::WizardTextBack),
            "Backspace must still delete characters on the first step"
        );
        assert!(!shown(&app).iter().any(|(_, label)| *label == "Back"));
    }

    #[test]
    fn key_display_spells_modifiers_and_arrows() {
        assert_eq!(key_display(Key::ctrl(KeyCode::Char('k'))), "Ctrl+K");
        assert_eq!(key_display(Key::shift(KeyCode::Up)), "Shift+↑");
        assert_eq!(key_display(Key::ch(' ')), "Space");
        assert_eq!(key_display(Key::plain(KeyCode::Esc)), "Esc");
    }
}
