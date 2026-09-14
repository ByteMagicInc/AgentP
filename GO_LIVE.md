# How to Ship AgentP v0.1.0

The release pipeline is already built. Pushing a `v0.1.0` tag runs everything: build,
GitHub Release, crates.io, Homebrew, and Scoop. You just need to do the setup below first,
then push the tag.

Do these in order.

## 1. Merge your code

- [ ] Wait for the release-candidate PR's CI and review checks to pass.
- [ ] Merge it into `master`.

## 2. Set up accounts and secrets (one time only)

- [ ] Get a crates.io API token (log in, Account Settings, New Token).
- [ ] Add two GitHub repo secrets (Settings, Secrets and variables, Actions):
  - `CARGO_REGISTRY_TOKEN` = the crates.io token
  - `TAP_GITHUB_TOKEN` = a token with write access to the repos below
- [ ] Create `ByteMagicInc/homebrew-tap` with a default branch and an initial commit (for example, a README).
- [ ] Create `ByteMagicInc/scoop-bucket` with a default branch and an initial commit (for example, a README).
- [ ] Make `ByteMagicInc/AgentP` public.

## 3. Update the changelog

`CHANGELOG.md` already has a `[0.1.0]` section plus a separate `[Unreleased]` block.
Do not add a second `[0.1.0]` header.

- [ ] Move the `[Unreleased]` notes into the existing `[0.1.0]` section.
- [ ] Set that section's date to the ship day.
- [ ] Leave an empty `[Unreleased]` heading under `<!-- next-header -->`.
- [ ] Commit and push to `master`.

## 4. Push the tag (this ships it)

```sh
git checkout master && git pull
git tag v0.1.0
git push origin v0.1.0
```

Then open the **Actions** tab and watch the Release workflow finish.

## 5. Check it worked

- [ ] `cargo install agent-p`
- [ ] `brew install ByteMagicInc/tap/agent-p`
- [ ] `scoop bucket add bytemagicinc https://github.com/ByteMagicInc/scoop-bucket`
- [ ] `scoop install agent-p`
- [ ] Download a file from the Releases page and run `agentp --help`.

---

### Good to know

- **The tag must match `Cargo.toml`.** The version there is `0.1.0`, so the tag is `v0.1.0`.
  If they don't match, the workflow stops immediately.
- **Prerelease tags skip Homebrew and Scoop**, not crates.io. A tag with a hyphen
  (`v0.1.0-rc1`) is treated as a prerelease: GitHub marks the release as prerelease
  and does not update the tap or bucket. The crate is still published. To test that
  path you must also set `Cargo.toml` to `0.1.0-rc1` so the tag check passes. Yank
  the crate afterward; crates.io versions cannot be deleted.
- **You can't reuse a version.** Once `0.1.0` is on crates.io it's permanent. To fix a
  mistake, bump to `0.1.1`.
