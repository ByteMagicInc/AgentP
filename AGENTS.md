# AGENTS.md

This file is the source of truth for AI coding assistants working with code in this repository. `CLAUDE.md` holds a verbatim copy of everything below, for tools that do not follow file imports. Edit here first, then mirror the change into `CLAUDE.md` so the two never disagree.

## Commands

- `cargo build` — compile
- `cargo run` — launch the default mode (TUI unless `default_mode` is `cli`)
- `cargo run -- <subcommand>` — run a CLI subcommand (e.g. `podcast list`, `download`)
- `cargo check` — fast type-check without full compilation
- `cargo test` — run unit tests
- `cargo clippy -- -D warnings` — lint with zero warnings
- `cargo build --release` — optimized build

## Architecture

AgentP is a single-binary Rust application for downloading and tagging podcast episodes. It has two modes: an interactive **TUI** built with `ratatui`/`crossterm`, and a headless **CLI** built with `clap`. The default mode is configurable via `default_mode` in `config.json`.

**Stack:** Tokio for async HTTP and tasks; `tokio::sync::mpsc` carries results from background work into the TUI loop. The TUI main loop calls `terminal.draw` every iteration and blocks on `event::poll(Duration::from_millis(50))` before reading input, so redraws are driven by that poll interval when idle (not a fixed 60 FPS).

**Source modules:**

- `main.rs` — entry point, CLI argument parsing via `clap`, dispatches to TUI or CLI mode based on subcommand or `default_mode` config
- `tui.rs` — TUI event loop: terminal setup, key dispatch against the keymap, spawns background tasks, command-palette handling
- `cli/` — headless CLI mode, split by concern:
  - `args.rs` — `clap` derive structs: `Cli`, `Command`, `PodcastCommand`, `DownloadArgs`, `ConfigCommand`, `OpenCommand`, and all args types
  - `handlers.rs` — command execution: podcast CRUD, download with `--episodes`/`--latest`/`--oldest`/`--all`, config show/set/path, open folders
  - `mod.rs` — re-exports `Cli` and `run`
- `app/` — application state and screen logic, split by concern:
  - `state.rs` — `App` struct, `Screen` enum, core state, navigation, episode/download handling
  - `commands.rs` — command palette types (`CommandEntry`, `COMMANDS`), filtering, and navigation
  - `keymap.rs` — every key binding: `Key`, `Action`, `Binding`, `KeyContext`, `context_bindings`, `resolve_global` / `resolve_context`, `hints`
  - `config_screen.rs` — config menu screen logic, directory editing, folder operations
  - `podcast_editor.rs` — edit-podcast screen logic (field navigation, save/discard, delete)
  - `add_podcast_wizard.rs` — multi-step wizard (`WizardStepKind`, `WIZARD_STEPS`), step navigation
- `podcast/` — data types and I/O, split by domain:
  - `data.rs` — `Podcast`, `Config`, `DefaultMode`, `EpisodeInfo`, `DownloadEvent` structs
  - `config_files.rs` — `load_config` / `save_config`, path helpers (`config_dir`, `config_path`)
  - `rss_feed.rs` — `fetch_episode_list`, `get_last_podcast_name`, `fetch_feed_metadata` / `FeedMetadata` (async RSS fetching)
  - `download.rs` — `download_selected_episodes` (async download + ID3 tagging); `sanitize_filename` shared `pub(crate)` helper
- `ui/` — rendering, split per screen:
  - `theme.rs` — Dracula color palette
  - `banner.rs` — the podcast-list banner: mascot art, both wordmarks, and `draw_banner`, which left-aligns them in a centered box and animates a one-shot typewriter reveal plus a periodic blink, both pure functions of `App::banner_started.elapsed()`. `banner_style` picks `WORDMARK_JOINED` (box-drawing strokes that meet across cell edges) or `WORDMARK_ASCII`; both place the letters in the same columns.
  - `widgets.rs` — shared helpers (`styled_block`, `key_hint`, `hint_bar`, `render_centered_dialog`, `format_date`)
  - `podcast_list.rs` — render podcast list screen
  - `episode_select.rs` — render episode selection with checkboxes
  - `download.rs` — render download progress screen
  - `config.rs` — render config menu and confirmation dialogs
  - `edit_podcast.rs` — render podcast editor and podcast selection screens
  - `add_podcast.rs` — render add-podcast wizard
  - `command_palette.rs` — render command palette overlay

**CLI surface:** full command list is in the README and `agentp --help`. Two non-obvious things to know when writing or changing CLI code:

- `--podcast <selector>` accepts either a 1-based index or a case-insensitive name match — see how handlers resolve this in `cli/handlers.rs`.
- `download` is mutually exclusive across `--episodes`/`--latest`/`--oldest`/`--all`; `clap` enforces this via the `DownloadArgs` group in `cli/args.rs`.

**TUI Screens (`Screen` enum):**

1. **PodcastList** — podcast list with latest-episode preview; `j/k` or arrows move, `Enter` opens episodes, `r` refresh feeds, `c` config, `e` edit selected podcast, `D` delete prompt (confirm with `y`/`D`, cancel `Esc`; `Enter` is excluded here because it opens a podcast on this screen and deletion is irreversible); `Home`/`g`, `End`/`G`, `PageUp`/`PageDown` jump and page
2. **EpisodeSelect** — checkboxes per episode, `Space` toggle, `a` all, `s` sort, `o` open podcast folder, `Enter` download (if any selected), `Esc` back
3. **Downloading** — progress gauge and log; `Esc` returns when finished or on error
4. **Config** — menu: add podcast, download folder, default mode, banner style, new-podcast defaults, edit podcasts list, open `config.json`, open `podcasts.json`, open download folder; dialog and directory text-edit modes; `Esc` back to podcast list in navigate mode; `r` on download-folder row restores default path
5. **EditPodcastSelect** — pick a podcast; `Enter` opens editor, `Esc` to config
6. **EditPodcast** — field list for one podcast or template; text / bool / usize editing; `s` save, `r` restore field default, `R` reset all on the template, `Esc` discard in navigate mode; `Ctrl+C` exit with discard confirmation when dirty; `Ctrl+V` paste while editing text
7. **AddPodcast** — multi-step wizard (`WIZARD_STEPS` in `add_podcast_wizard.rs`); first step is Feed URL; pressing Enter on it shows a **mode-select overlay** (`1`/`m` Manual, `2`/`p` Prepopulate from feed); Prepopulate fetches channel metadata and pre-fills Name, Album Name (sanitized), and Artist; `Esc` cancels/returns at each stage; `Ctrl+V` on text steps

**Primary flows:** **PodcastList → EpisodeSelect → Downloading** for downloads. **Config** (from `c` on the podcast list or the palette) reaches **EditPodcastSelect → EditPodcast** and the add-podcast wizard.

**Quitting:** `App::request_quit` is the one path for both `q` and the palette **Quit**: it refuses while `download_in_progress`, opens the save/discard confirmation on **EditPodcast** with unsaved changes (`dirty`), and otherwise sets `should_quit`. `Ctrl+C` calls `request_force_quit`, which skips only the download guard. Nothing breaks out of the loop directly — the loop exits on `should_quit`.

**Keymap — the single source of truth for keys.** Every TUI binding is declared once in `app/keymap.rs`. A `Binding` carries its keys, the hint-bar `display` and `label`, the `Action` each key produces, and its guards; `context_bindings(KeyContext)` lists the bindings a screen-and-mode offers **in hint-bar order**, which is the order the bar collapses left to right. `tui.rs` resolves a `KeyEvent` to an `Action` (`resolve_global`, then `resolve_context`) and applies it in one `match`; `ui/widgets.rs` renders the bar from `keymap::hints`; `commands.rs` points each palette entry at a `Binding`. Rebinding a key is one edit to that binding's `keys`.

- **`display: None` derives the hint text from the keys** through `key_display`, so a rebind needs no second edit. Set it only for grouped hints (`↑/↓`, `Shift+↑/↓`, `+/-`, `1/m`, `Type`), where one hint covers several keys or even two actions.
- **Two guards, not one.** `enabled` decides whether the keys dispatch; `hint_when` decides whether the hint shows, and defaults to `enabled`. They diverge on purpose: the editor's `Space Toggle` is hinted always but dispatches only on a bool field, and the wizard's `Backspace` dispatches always — deleting a character, or stepping back when the buffer is already empty — but is hinted only past the first step. Gating both on one predicate breaks one of them.
- **Contexts and hint contexts differ.** `App::key_context` includes the overlays — delete prompt, folder and directory confirmations — so they can rebind keys; `App::hint_context` maps them back to the screen underneath, which keeps showing its own hint bar. Only the editor's discard confirmation replaces the bar.
- **`GLOBALS` resolve before the context:** `Ctrl+C` and `Ctrl+K` unconditionally, `q` and `?`/`.` only when `not_typing` (no palette, no text field) — which is why `Ctrl+K` opens the palette from inside a text field. Contexts still list `&QUIT` and `&COMMAND_PALETTE` to place those hints.
- Character keys and `Backspace` that no binding claims fall through to the context's text buffer (`KeyContext::is_text_input`). Palette navigation stays in `tui.rs`, because its list is driven by the filter.
- Invariant tests guard all of it: a golden `(key, label)` sequence per hint context — the collapse order — plus no context binding one key twice, and `commands.rs` checks every palette shortcut names a binding that runs that entry's action.

**List navigation:** `apply_list_jump` in `state.rs` handles `Home`/`g`, `End`/`G`, `PageUp`/`PageDown` against any `ListState`, clamped to `floor..len`; the floor is non-zero on the template editor, where `podcast_editor_first_field` hides the identity fields. The `apply_list_jump` wrapper in `tui.rs` picks the list for the current `KeyContext` and runs before keymap resolution, so the jump keys reach every list without each context repeating them. Contexts with no list — the overlays among them — return false. The add-podcast wizard is a stepper and is deliberately excluded, as is the command palette: its list is driven by the filter, where `Char` keys must reach `palette_type` rather than jump the cursor.

**Command palette:** `Ctrl+K` toggles. Type to filter; arrows move; `Enter` runs the selected `Action`; `Esc` closes; `Ctrl+V` pastes into the filter. Entries live in `commands.rs` as `COMMANDS`, and each one's `shortcut` is a `&'static Binding` rather than a string — the palette prints the keys that binding maps to the entry's own action, so `Shift+↑/↓ Reorder` still shows as `Shift+↑` on **move podcast up**. It names those keys only where they are wired (`Binding::is_live_on_screen`): every entry runs from the palette, but most of the keys are bound on one screen, so **open config editor** shows `c` on the podcast list and nothing elsewhere, and no entry names a key over a text field. That question is asked of the screen behind the palette rather than through `enabled`, because the palette is a text field itself and `not_typing` reads false for as long as it is open. On **PodcastList**, **go to podcast list** is hidden; the open-folder line reads **open selected podcast folder**. On **Config**, **open config editor** is hidden.

**Data flow:**

1. `load_config()` reads from `<home>/.config/AgentP/` (via `dirs`); on Windows the same relative path is typically `%USERPROFILE%\.config\AgentP\`. `config.json` holds `download_dir_location`, `default_podcast`, `default_mode`, and `banner_style`; `podcasts.json` holds the podcast list. Missing files are created (demo podcasts from `example.podcasts.json` when appropriate). `example.podcasts.json` in the repo is the template for the podcast list shape.
2. On TUI startup, `spawn_latest_fetches` runs `get_last_podcast_name` per podcast on `tokio::spawn`; results arrive on an unbounded `mpsc` channel consumed with `try_recv` in the main loop.
3. Choosing a podcast spawns `fetch_episode_list`; results arrive on a separate unbounded `mpsc` channel as `Ok(episodes)` / `Err`.
4. `download_selected_episodes()` writes under `<download_dir>/<effective_album_name>/` with optional ID3 tags; `effective_album_name()` falls back to podcast name when album_name is empty. Progress uses `DownloadEvent` on `mpsc`.
5. Config changes from the TUI call synchronous `save_config()` in `config_files.rs`, which rewrites both JSON files.

**Async architecture:** Episode fetch, latest-title fetch, and downloads use `tokio::spawn` and `mpsc`; the TUI main loop drains channels with `try_recv()` each frame. CLI commands call the same async functions directly.

**Per-podcast options** in `podcasts.json` use JSON keys `override_tags`, `remove_existing_tags`, `remove_images`, `override_album_name`, `override_artist`, `override_title`, `override_file_name`, `leading_zeros_to_title`, and `user_agent` (see `serde` aliases on `Podcast` in `data.rs`; Rust fields are named `overwrite_*` plus `album_name`, `artist`, etc.). Most control ID3 and filename behavior; `user_agent` overrides the built-in `AgentP/<version>` `User-Agent` for that podcast's audio downloads (blank = default), applied as a per-request header in `download.rs`. The shared `reqwest` client in `podcast/http.rs` carries the default `User-Agent`; RSS feed fetches always use that default.

**Podcast editor/wizard field indices are positional.** The TUI podcast editor (`podcast_editor.rs`) and add-podcast wizard (`add_podcast_wizard.rs`) share one `WIZARD_STEPS` list and address fields by index: identity text fields `0..PODCAST_FIELD_BOOL_START`, bool `PODCAST_FIELD_BOOL_START..PODCAST_FIELD_USIZE`, usize `PODCAST_FIELD_USIZE`, then `user_agent` as the final field — a Text field kept last in the UI (constants in `state.rs`). Because text fields are no longer contiguous, the wizard commits text by `WizardStepKind::Text`, not by an index threshold. `field_text`/`field_bool` in `data.rs` and the editor's `enter_edit_mode`/`commit_edit`/`toggle_bool`/`restore_default` match on the same indices. Inserting or reordering a field means updating all of these together — the catch-all match arms make a wrong index fail silently, so the invariant test in `add_podcast_wizard.rs` guards it.

**Key crates (directly used in `src/`):** `ratatui`, `crossterm`, `tokio`, `reqwest`, `rss`, `id3`, `serde`/`serde_json`, `anyhow`, `dirs`, `chrono`, `open`, `arboard`, `sanitize-filename`, `clap`, `unicode-width`.

`main.rs` and helpers use `anyhow::Result`. Async network/file streaming for downloads lives under Tokio in `podcast/download.rs`.

## Gotchas

- **Legacy JSON keys load but are never written.** `example.podcasts.json` and every config `save_config()` writes use the Rust names (`overwrite_*`, `leading_zeros_amount`). The older `override_*`, `leading_zeros_to_title`, `create_new_tags` and `remove_image` spellings survive only as `serde` aliases on `Podcast` in `data.rs`, and a config using them is rewritten canonically on the next save. Dropping an alias silently breaks configs that predate the rename.
- **`example.podcasts.json` is the canonical shape.** If you add or rename a field on `Podcast`, update it too — it's what seeds new installs. Fields marked `skip_serializing_if` are optional and deliberately absent from it, `user_agent` among them.
- **Episode index 1 is newest.** `--latest`, `--oldest`, `--episodes`, and the ID3 title-number prefix all share this convention; don't flip it in one place.
- **`save_config()` rewrites both JSON files** every call — TUI-side edits don't need a separate save for `podcasts.json` vs `config.json`.
- **TUI redraws are driven by the 50ms `event::poll`**, not a timer. If you add background work that should update the UI, send on an `mpsc` — don't rely on a repaint loop. The banner animation is the one thing that does ride the poll: it reads elapsed time at draw time and needs no task or channel.

## Code Style

- **Rust:** 2024 edition; use normal `rustfmt` formatting.
- **No comments:** Do not add any comments to the code — no `//` or `/* */` comments. Doc comments (`///` and `//!`) are allowed.
- **Errors:** use `anyhow::Result` for propagation.
- **Naming:** `snake_case` fields, `PascalCase` types; derive `Debug` / `Serialize` / `Deserialize` as needed.
- **Imports:** `std`, then external crates, then `crate::` modules.
