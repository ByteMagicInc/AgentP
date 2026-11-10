# CLAUDE.md

This file is a verbatim copy of `AGENTS.md`, kept for tools that do not follow file imports. `AGENTS.md` is the source of truth: make edits there and mirror them here so the two never disagree.

## Done

CI runs automatically only when a release is tagged (`release.yml` calls `ci.yml`), never on a pull request, so a change is checked before it merges only if you check it. A change is done when:

- **Green:** all three pass, exactly as CI runs them:
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --locked -- -D warnings`
  - `cargo test --all --locked`
- **Documented:** a change to keys, CLI flags or `Podcast` fields updates the matching README table (Keyboard reference, CLI reference, Per-podcast options). A change to anything `skills/agentp/SKILL.md` relies on updates that skill too: a command or flag, an output line it reads (`podcast list` numbering, the `podcast show` labels, `Warning:` and `blocked URL` lines), the `podcasts.json` layout, episode numbers, or `--latest` order. `npx skills add` installs the skill from `master` at once, so merge a skill change that needs an unreleased command together with that release. When the skill starts using a command or flag newer than `agentp skill`, move its step 1 check to it: the `agentp skill --help` probe, its error line and the Done line.
- **Pictured:** a change to a screen the README shows is re-recorded with `docs/capture_media.py`. It runs on Linux only (its docstring says how); elsewhere, say in the hand-off that the media needs re-recording.

## Commits and releases

Start commit subjects with `Fix`, `Add`, `Refactor` or `Update`: `cliff.toml` groups the changelog by that first word. Releasing is in `docs/releasing.md`.

## Running the app

- `cargo run -- <subcommand>` runs the headless CLI (`podcast list`, `podcast episodes --podcast 1 --search …`, `download …`); `cargo run -- --help` lists the surface. `cargo run -- --tui` opens the TUI, which needs a real terminal; plain `cargo run` follows `default_mode` in `config.json`.
- **Downloading episodes for the user**, rather than changing the code: follow `skills/agentp/SKILL.md`, running `cargo run -q --` wherever it says `agentp`.
- **Both modes read and write the real config** in `<home>/.config/AgentP/` (`config.json`, `podcasts.json`). To experiment on Linux or macOS, point `HOME` at a scratch folder. On Windows `dirs::home_dir` ignores environment variables, so back up both files first and restore them after.

## Architecture

AgentP is one Rust binary with two modes: an interactive **TUI** (`ratatui`/`crossterm`) and a headless **CLI** (`clap`). Every source file opens with a `//!` doc naming what it owns: `app/` holds state and screen logic, `ui/` renders, `podcast/` holds data types, config I/O, RSS and downloads, and `cli/` the headless mode.

- **Async and redraws:** episode, latest-title and feed-metadata fetches and downloads run on `tokio::spawn` and report over unbounded `mpsc` channels, sending errors as `String`. The TUI loop draws, drains the channels with `try_recv`, then blocks on a 50 ms `event::poll`; there is no repaint timer, so background work that should change the screen sends on a channel. Two things are read at draw time instead: the banner animation's elapsed time and the `ffmpeg` check in `podcast/ffmpeg.rs`. The CLI awaits the same functions directly.
- **Tests never run `ffmpeg`.** App-state code leaves `ffmpeg::refresh_in_background` to `tui.rs`, which calls it at launch in `run` and in `apply_action` whenever an action turns Convert To MP3 on.
- **HTTP** goes through `podcast::http::client()`, which carries the `User-Agent` some podcast hosts require; a podcast's `user_agent` overrides it on audio downloads only.

## TUI

Screens are the `Screen` enum in `app/state.rs`. Downloads run **PodcastList → EpisodeSelect → Downloading**; **Config** reaches **EditPodcastSelect → EditPodcast**, the **AddPodcast** wizard and **About**.

### Keymap

`app/keymap.rs` is the single source of truth for key bindings; read its module doc and the `Binding` and `KeyContext` docs before touching one. What they leave out:

- **Two guards diverge on purpose.** `enabled` decides whether keys dispatch; `hint_when` decides whether the hint shows. The editor's `Space Toggle` is hinted always but dispatches only on a bool field; the wizard's `Backspace` always dispatches (deleting a character, or stepping back on an empty buffer) but is hinted only past the first step. One predicate for both breaks one of them.
- **Overlays keep the bar underneath.** `App::key_context` includes the delete prompt and the folder and directory confirmations so they can rebind keys; `App::hint_context` maps them back to the screen underneath. Only the editor's discard confirmation and the wizard's Manual/Prepopulate overlay replace the bar.
- **`GLOBALS` resolve first:** `Ctrl+C` and `Ctrl+K` always, `q` and `?`/`.` only when `not_typing`, which is why `Ctrl+K` opens the palette from inside a text field. Contexts still list `&QUIT` and `&COMMAND_PALETTE` to place their hints.
- **Some keys never reach the keymap.** `tui.rs` handles them first: the list jumps (`Home`/`g`, `End`/`G`, `PageUp`/`PageDown`, through `apply_list_jump`), a digit on the episode list, and the command palette's own keys, because its list is driven by the filter. The duplicate-key test cannot see these, so check them before binding a key.
- Golden tests pin each hint context's `(key, label)` sequence. Update the golden when a hint change is intended.

### Command palette

Entries are `COMMANDS` in `app/commands.rs`. A `shortcut` is an optional `&'static Binding`, never key text: the palette prints that binding's keys for the entry's own action, and only where `Binding::is_live_on_screen` holds for the screen behind the palette.

### Quitting

Every quit path calls `App::request_quit` (`q`, the palette's **Quit**) or `request_force_quit` (`Ctrl+C`), which hold the download and unsaved-editor guards. The loop exits on `should_quit` alone.

### Episode search and jump

`App::episodes` and `episode_selected` always hold the whole feed, so downloads and sorting never see a filter; select-all (`a`) acts on the episodes the search leaves showing. Four invariants keep the filtered view honest:

- **Cursor:** `episode_visible` holds indices into `episodes`, and `episode_list_state` points into `episode_visible`. Read the cursor through `current_episode_index()` and render by iterating `episode_visible`; indexing `episode_selected` by list position picks the wrong episode.
- **In step:** `episode_search_index` holds one `SearchEntry` per episode, in `episodes` order. `reset_episode_view` rebuilds it and `toggle_sort_order` reverses it together with `episodes` and `episode_selected`; any other code that replaces or reorders `episodes` keeps all three in step.
- **Go to by number:** the go-to number is a `feed_index` (see **Episode numbers** below), not a list position, so it finds the episode under either sort order. A digit typed on the list opens the prompt already holding that digit: an `Action` carries no payload, so `tui.rs` seeds it before keymap resolution, and `GO_TO_EPISODE` binds `#` for the palette and the hint bar.
- **Typing and pasting:** `EpisodeSearch` and `EpisodeJump` are text-input contexts, so `q`, `?`, `j` and `Space` type instead of acting. Paste goes through `insert_pasted` in `tui.rs`, which lets each prompt decide what a paste means: the search turns line breaks into spaces, and the go-to prompt takes only a plain number, optionally led by `#`.

The query grammar is `podcast::episode_search`, shared with `podcast episodes --search`; its module doc is the reference for every term.

### Podcast editor and wizard

The editor (`podcast_editor.rs`) and the add-podcast wizard (`add_podcast_wizard.rs`) share `WIZARD_STEPS` and address fields **by position**: identity text first, then the bools from `PODCAST_FIELD_BOOL_START`, the usize field at `PODCAST_FIELD_USIZE`, and `user_agent` last. The editor also edits the **template**, `config.default_podcast` (Config → New-podcast defaults), which starts at `PODCAST_FIELD_BOOL_START`. A field change is done when every site listed on `PODCAST_FIELD_COUNT` in `app/state.rs` agrees: their catch-all arms fail silently, and the tests check only the step layout.

## Data and config

- **Adding a `Podcast` field:** mark it `#[serde(default)]`, because existing `podcasts.json` files and the `default_podcast` in `config.json` lack it, and give it a load test like `convert_to_mp3_defaults_to_false_when_missing_from_saved_json`. Add it to `default.config.json`, which `Podcast::default()` parses, and to each podcast in `example.podcasts.json`, which seeds new installs; empty strings such as `user_agent` stay out.
- **Feeds are not all MP3.** `download.rs` sniffs each file's container from its bytes and tags MP4 with `mp4ameta`, never ID3. With `convert_to_mp3` on, `ffmpeg` runs only on a positively sniffed container, with its demuxer forced and only local files allowed: the file is untrusted, and a probed `Unknown` file could make `ffmpeg` fetch URLs or read local files. A missing or failed conversion, an unrecognised format and an original that could not be deleted are each a `DownloadEvent::Warning`, not an `Error`: the episode is usable, so `agentp download` still exits 0. Download, write and tag failures stay errors. The docs on `AudioContainer`, `convert_to_mp3` and `run_ffmpeg` hold the details. The CLI mirrors every option through `PodcastFieldArgs` in `cli/args.rs`, `apply_podcast_fields` and the `podcast show` and `config show` printers in `cli/handlers.rs`.
- **Episode numbers:** index 1 is the oldest episode and the highest index the newest. `feed_index` is `length - rss_idx` in `rss_feed.rs` and `download.rs` inverts it, so a newest-first feed numbers its latest episode as the episode count. `--episodes`, `podcast episodes`, go-to and the ID3 title-number prefix share this; `--latest` and `--oldest` select by feed order instead. Change it everywhere or nowhere.
- **`save_config()` rewrites both JSON files** on every call, so a TUI edit needs one save whichever file it touches.
- **Legacy JSON keys load but are never written.** `Podcast` in `data.rs` reads `override_*`, `leading_zeros_to_title`, `create_new_tags` and `remove_image` through `serde` aliases and writes the Rust names (`overwrite_*`, `leading_zeros_amount`). Keep every alias: dropping one silently breaks configs that predate the rename.
- **`effective_album_name()`** names a podcast's download folder and falls back to the podcast name when `album_name` is empty.

## Code style

- Write every explanation as a doc comment: `///` on items, `//!` on modules. Inside function bodies too, the code carries no `//` or `/* */` comments; say it with a clearer name or in the item's doc instead.
- Propagate errors with `anyhow::Result`.
- Order imports `std`, then external crates, then `crate::`.
