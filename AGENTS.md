# AGENTS.md

This file is the source of truth for AI coding assistants working with code in this repository. `CLAUDE.md` holds a verbatim copy of everything below, for tools that do not follow file imports. Edit here first, then mirror the change into `CLAUDE.md` so the two never disagree.

## Done means green

CI runs only when a release is cut, so these three are the only check a change gets before it merges. A change is done when all three pass, exactly as CI runs them:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --locked -- -D warnings`
- `cargo test --all --locked`

Start commit subjects with `Fix`, `Add`, `Refactor` or `Update`: `cliff.toml` groups the changelog by that first word. Releasing is in `docs/releasing.md`.

## Running the app

- `cargo run -- <subcommand>` runs the headless CLI (`podcast list`, `podcast episodes --podcast 1 --search …`, `download …`); `agentp --help` lists the surface. Plain `cargo run` opens the TUI, which needs a real terminal.
- **Both modes read and write the real config** in `<home>/.config/AgentP/` (`config.json`, `podcasts.json`). To experiment on Linux or macOS, point `HOME` at a scratch folder. On Windows `dirs::home_dir` ignores environment variables, so back up both files first and restore them after.
- README screenshots and GIFs come from `docs/capture_media.py`, which drives the real TUI against live feeds; its docstring says how to run it. Re-record them when a change alters a screen they show.

## Architecture

AgentP is one Rust binary with two modes: an interactive **TUI** (`ratatui`/`crossterm`) and a headless **CLI** (`clap`). `default_mode` in `config.json` picks which one runs without a subcommand; `--tui`/`--cli` override it.

- `app/` holds state and screen logic, `ui/` renders one file per screen, `podcast/` holds data types, config I/O, RSS and downloads, and `cli/` the headless mode. Every source file opens with a `//!` doc naming what it owns.
- **Async:** episode fetches, latest-title fetches and downloads run on `tokio::spawn` and report over unbounded `mpsc` channels, which the TUI loop drains with `try_recv` every frame. The CLI awaits the same functions directly.
- **Redraws ride the input poll.** The loop draws, then blocks on a 50 ms `event::poll`; there is no repaint timer. Background work that should change the screen sends on an `mpsc`. Only the banner animation reads elapsed time at draw time and needs no channel.
- `example.podcasts.json` is the canonical `podcasts.json` shape and seeds new installs. Add or rename a `Podcast` field there too; fields marked `skip_serializing_if` (`user_agent` among them) stay out of it on purpose.

## TUI

Screens are the `Screen` enum in `app/state.rs`. Downloads run **PodcastList → EpisodeSelect → Downloading**; **Config** reaches **EditPodcastSelect → EditPodcast**, the **AddPodcast** wizard and **About**.

### Keymap

`app/keymap.rs` is the single source of truth for keys: rebinding is one edit to a binding's `keys`, and the hint bar, the palette's shortcut text and dispatch all follow. `context_bindings(KeyContext)` lists a context's bindings in hint-bar order, which is the order the bar collapses, so placement is part of the screen's design.

- **`display: None`** derives the hint from the keys. Set `display` only for a hint that covers several keys or actions (`↑/↓`, `Shift+↑/↓`, `+/-`, `1/m`, `Type`).
- **Two guards diverge on purpose.** `enabled` decides whether keys dispatch; `hint_when` decides whether the hint shows. The editor's `Space Toggle` is hinted always but dispatches only on a bool field; the wizard's `Backspace` always dispatches (deleting a character, or stepping back on an empty buffer) but is hinted only past the first step. One predicate for both breaks one of them.
- **Overlays have a key context but not a hint context.** `App::key_context` includes the delete prompt and the folder and directory confirmations so they can rebind keys; `App::hint_context` maps them back to the screen underneath, whose bar stays visible. Only the editor's discard confirmation replaces the bar.
- **`GLOBALS` resolve first:** `Ctrl+C` and `Ctrl+K` always, `q` and `?`/`.` only when `not_typing`, which is why `Ctrl+K` opens the palette from inside a text field. Contexts still list `&QUIT` and `&COMMAND_PALETTE` to place their hints.
- **Text input:** in a context where `KeyContext::is_text_input` is true, unclaimed character keys and `Backspace` type into its buffer.
- **List jumps run before the keymap.** `apply_list_jump` (`Home`/`g`, `End`/`G`, `PageUp`/`PageDown`) clamps to `floor..len`, where the floor is non-zero on the template editor (`podcast_editor_first_field`). Its wrapper in `tui.rs` picks the list for the current context. The wizard and the palette have none on purpose: the wizard is a stepper, and the palette's `Char` keys must reach `palette_type`.
- Golden tests pin each hint context's `(key, label)` sequence and forbid a context binding one key twice. Update the golden when a hint change is intended.

### Command palette

`Ctrl+K` toggles it. Entries are `COMMANDS` in `commands.rs`, each pointing at a `&'static Binding`, and the palette prints only the keys that map to that entry's own action, so `Shift+↑/↓ Reorder` shows as `Shift+↑` on **move podcast up**. It shows keys only where `Binding::is_live_on_screen` holds for the screen behind the palette: every entry runs from the palette, but most keys are bound on one screen. A test checks that every shortcut names a binding that runs its entry's action.

### Quitting

`App::request_quit` is the one path for `q` and the palette's **Quit**: it refuses during a download, asks to save or discard on a dirty **EditPodcast** (returning there first from **About**), and otherwise sets `should_quit`. `Ctrl+C` calls `request_force_quit`, which skips only the download guard. The loop exits on `should_quit` alone.

### Episode search and jump

`App::episodes` and `episode_selected` always hold the whole feed, so downloads and sorting never see a filter. Four invariants keep the filtered view honest:

- **Cursor:** `episode_visible` holds indices into `episodes`, and `episode_list_state` points into `episode_visible`. Read the cursor through `current_episode_index()` and render by iterating `episode_visible`; indexing `episode_selected` by list position picks the wrong episode.
- **In step:** `episode_search_index` holds one `SearchEntry` per episode, in `episodes` order. `reset_episode_view` rebuilds it and `toggle_sort_order` reverses it together with `episodes` and `episode_selected`; any other code that replaces or reorders `episodes` keeps all three in step.
- **Go to by number:** the go-to number is a `feed_index` (see **Episode numbers** below), not a list position, so it finds the episode under either sort order. A digit typed on the list opens the prompt already holding that digit: an `Action` carries no payload, so `tui.rs` seeds it before keymap resolution, as it does for `apply_list_jump`, and `GO_TO_EPISODE` binds `#` for the palette and the hint bar.
- **Typing and pasting:** `EpisodeSearch` and `EpisodeJump` are text-input contexts, so `q`, `?`, `j` and `Space` type instead of acting. Paste goes through `insert_pasted` in `tui.rs`, which lets each prompt decide what a paste means: the search turns line breaks into spaces, and the go-to prompt takes only a plain number, optionally led by `#`.

The query grammar lives in `podcast::episode_search`, shared with `podcast episodes --search`; its module doc is the reference for every term. A date term matches by calendar date and also as literal text, so `9/11` still finds a title that says it: keep both paths.

### Podcast editor and wizard

The editor (`podcast_editor.rs`) and the add-podcast wizard (`add_podcast_wizard.rs`) share `WIZARD_STEPS` and address fields **by position**: identity text `0..PODCAST_FIELD_BOOL_START`, bools up to `PODCAST_FIELD_USIZE`, the usize field, then `user_agent`, a text field kept last. `field_text`/`field_bool` in `data.rs` and the editor's `enter_edit_mode`, `commit_edit`, `toggle_bool` and `restore_default` match on the same numbers, and their catch-all arms make a wrong index fail silently. Adding or reordering a field means updating all of them and the constants in `state.rs`; `wizard_steps_match_field_index_constants` guards it. Since text fields are not contiguous, the wizard commits text by `WizardStepKind::Text`, never by an index threshold.

## Data and config

- **Episode numbers:** index 1 is the oldest episode and the highest index the newest. `feed_index` is `length - rss_idx` in `rss_feed.rs` and `download.rs` inverts it, so a newest-first feed numbers its latest episode as the episode count. `--episodes`, `podcast episodes`, go-to and the ID3 title-number prefix share this; `--latest` and `--oldest` select by feed order instead. Change it everywhere or nowhere.
- **`save_config()` rewrites both JSON files** on every call, so a TUI edit needs one save whichever file it touches.
- **Legacy JSON keys load but are never written.** `Podcast` in `data.rs` reads `override_*`, `leading_zeros_to_title`, `create_new_tags` and `remove_image` through `serde` aliases and writes the Rust names (`overwrite_*`, `leading_zeros_amount`). Keep every alias: dropping one silently breaks configs that predate the rename.
- **`effective_album_name()`** names a podcast's download folder and falls back to the podcast name when `album_name` is empty.
- **`--podcast <selector>`** takes a 1-based index or a case-insensitive name, resolved in `cli/handlers.rs`.

## Code style

- Rust 2024 edition, formatted by `rustfmt`.
- Explain code through names and doc comments (`///`, `//!`). Those are the only comments the code carries: leave out `//` and `/* */`.
- Propagate errors with `anyhow::Result`.
- Order imports `std`, then external crates, then `crate::`.
