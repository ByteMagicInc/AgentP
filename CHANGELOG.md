# Changelog

All notable changes to this project will be documented in this file.

<!-- next-header -->

## [Unreleased]

### Added

- About screen with version and build info (git SHA, platform, build profile) and links to the repository and issue tracker; reachable from the config menu and the command palette.
- Report issues and star the repository from inside the app: palette entries on every screen, a Report Issue row on the config menu, and issue pages prefilled with build information.
- Copy the version and build information to the clipboard from the About screen.

### Changed

- Podcast and episode lists render only the rows that fit on screen, cutting redraw cost by about 65% and keeping large feeds light on CPU while idle.
- CI now runs as part of tagged releases (or on demand) and gates every release build.

## [0.1.0] - 2026-09-18

Initial public release of AgentP, a podcast downloader with an interactive terminal interface and a scriptable CLI.

### Added

- Browse RSS podcast feeds, preview the latest episode, and select episodes for download in a Dracula-themed terminal interface.
- Add podcasts manually or prepopulate their name, album, and artist from feed metadata; edit, reorder, and delete podcasts from the library.
- Download selected episodes, the latest or oldest episodes, or an entire feed through the CLI, using podcast names or 1-based indices.
- Customize ID3 title, album, artist, and album-artist tags; remove existing tags or artwork; rename files and add zero-padded episode-number prefixes.
- Organize downloads into per-podcast folders with a configurable download location and reusable defaults for new podcasts.
- Override the audio-download User-Agent per podcast for hosts that reject the default client.
- Search actions through the command palette, navigate lists with keyboard shortcuts, and paste into text fields from the clipboard.
- Choose TUI or CLI as the default mode and select a joined or ASCII banner for terminal compatibility.
- Save configuration as JSON, seed new installations with demo podcasts, and accept legacy podcast option names.
- Provide release archives for Linux (x86_64 GNU, x86_64 musl, and ARM64), macOS (Intel and Apple Silicon), and Windows (x86_64), with Cargo, Homebrew, and Scoop distribution workflows.
