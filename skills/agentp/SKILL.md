---
name: agentp
description: Downloads podcast episodes as audio files with the `agentp` CLI. Use when the user wants podcast episodes downloaded (the newest, or one found by topic, date or number, from a show AgentP has or any other) or wants a podcast added to AgentP.
---

The `agentp` CLI does the work; `agentp <subcommand> --help` is the flag reference. Its subcommands run headless whatever `default_mode` says, and they change the user's real podcast list and download folder.

## 1. Find the CLI

Run `agentp --version`. When `agentp` is missing, offer the user the install for their platform and run it only once they agree:

- macOS or Linux: `brew install ByteMagicInc/tap/agent-p`
- Windows: `scoop bucket add bytemagicinc https://github.com/ByteMagicInc/scoop-bucket`, then `scoop install agent-p`
- With Rust: `cargo install agent-p --locked`

Done when `agentp --version` prints a version; a declined install ends the skill.

## 2. Pick the route

Run `agentp podcast list --no-fetch`. Each line starts with the podcast's number, which every later command uses. Match the request to one route:

- **Saved**: the podcast is on the list (a loose name match counts; confirm only if two entries fit). Keep its number.
- **New**: not on the list, and the user wants to keep it.
- **One-off**: not on the list, and the user wants it once ("just this one", "don't keep it", "remove it after").
- **Add only**: the user asks to add a podcast and names no episode to download.

Not on the list, with a download asked for but no signal about keeping it: ask New or One-off. Every other case proceeds without asking.

New, One-off and Add only need the RSS feed URL:

- Use the URL the user gave when it is a feed.
- Otherwise look it up: `https://itunes.apple.com/search?media=podcast&entity=podcast&term=<name>` returns JSON whose results carry a `feedUrl`. Failing that, web-search for the show's RSS feed.
- A page that is not RSS, including a content filter's block page, is the user's to resolve: report what came back and ask for the URL.

Then search `podcasts.json`, in the folder `agentp config path` prints, for that URL. A feed already there makes the route **Saved** (or, for Add only, means there is nothing to add), and that entry stays on the list even if the user said one-off; say so in the report.

Done when you hold one route and, unless it is Saved, a feed URL.

## 3. Pick the episodes

Skip this step for Add only.

- Newest, or a download request that names no episode: `--latest N`, with N = 1 by default.
- Oldest: `--oldest N`.
- Every episode: `--all`, only when the user says so.
- One episode by topic, date or number: when `agentp podcast episodes --help` lists `--search`, run `agentp podcast episodes --podcast <number> --search "<query>"`, building the query from the terms that help text shows. A release without `--search` lists every episode with `agentp podcast episodes --podcast <number>`; match the titles yourself. Pass the number from the first column to `--episodes`. More than one plausible match: show them and ask.

Numbers count up from the feed's last item, so in the usual newest-first feed the **highest** is the newest. Numbers in episode titles are the show's own and can differ from AgentP's.

Done when you hold one selector: `--latest N`, `--oldest N`, `--episodes A,B` or `--all`.

## 4. Add and download

Address every podcast by its number, never its name: `add` accepts duplicates, and a name selector picks the first match, so a name can reach a different podcast.

**Saved:** `agentp download --podcast <number> <selector>`.

**New, One-off and Add only:**

1. Run `agentp podcast add --feed-url "<url>" --from-feed`. It reads the feed, fills in name, album and artist, and prints `Added podcast "<name>".` When it fails, nothing was saved, and its error says what the URL returned: go back to the feed lookup in step 2 with that text.
2. The new podcast is the last line of `agentp podcast list --no-fetch`; confirm its name and keep its number N. Add only ends here.
3. One-off only: keep the `Download folder` line of `agentp podcast show --podcast N` for the report.
4. Run `agentp download --podcast N <selector>`.
5. One-off only: on exit 0, run `agentp podcast delete --podcast N`; the audio stays on disk. On a non-zero exit, leave the entry so a retry needs no re-add, and say so.

Give `download` a long timeout or run it in the background; it prints nothing while an episode transfers.

**MP3s:** when the user asks for MP3 files, pass `--convert-to-mp3 true` to `add`, or to `agentp podcast edit --podcast <number>` for a saved podcast, which keeps the setting for later downloads; tell the user. Conversion needs `ffmpeg` on `PATH`, and a release whose `--help` lacks the flag cannot convert.

**Exit status:** `download` exits non-zero when any episode fails to download, write or tag, even when its audio landed before the tag failed. A `blocked URL` line means a content filter or proxy stopped the file. `Warning:` lines leave a usable episode on disk and still exit 0; pass their text on.

Done when `download` has exited and a One-off entry is deleted (exit 0) or kept (failure); for Add only, when the podcast is on the list.

## 5. Report

Done when the user has the episode titles, the download folder (from `agentp podcast show`), every failure and warning with its text, and whether the podcast is on the list now.
