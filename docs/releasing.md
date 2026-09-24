# Releasing

Install the tools with `cargo install cargo-release git-cliff --locked`.

1. Update **Unreleased** in `CHANGELOG.md`. Optionally draft notes with `git-cliff --unreleased --strip header`; review and copy the relevant bullets without replacing the file or its `<!-- next-header -->` marker.
2. Commit the notes and start from a clean, up-to-date `master`. CI (fmt, clippy, tests) runs only as part of a release, where it gates the build; to check before tagging, run it from **Actions → CI → Run workflow**, or locally with `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test --all --locked`.
3. Preview `cargo release patch` (use `minor` or `major` as appropriate). Review the version and changelog changes.
4. Run `cargo release patch --execute` with the same version level when ready to publish.

`cargo release` runs tests, updates the version and changelog, creates a fresh **Unreleased** section, then commits, tags, and pushes. The tag triggers GitHub Actions to build archives, publish the reviewed changelog entry as release notes, publish to crates.io, and update Homebrew and Scoop for stable releases. Check that all release jobs succeed.
