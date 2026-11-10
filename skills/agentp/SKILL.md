---
name: agentp
description: Downloads podcast episodes and adds podcasts with the AgentP CLI (`agentp`). Use when the user wants a podcast episode downloaded or a podcast added to AgentP.
---

`agentp <subcommand> --help` is the flag reference. `podcast add`, `podcast edit`, `podcast delete` and `download` change the user's real podcast list and download folder: run them where a step says to.

## 1. Find the CLI

This skill is written for the AgentP release that ships `agentp skill`, and every later step relies on its commands. Run `agentp skill --help`:

- **Help prints:** the release is current enough; go on.
- **`agentp` is not found:** offer the user the install for their platform.
- **An unrecognised-subcommand error:** the installed AgentP is older than this skill. Offer the upgrade: `brew upgrade agent-p`, `scoop update agent-p`, `cargo install agent-p --locked`, or a newer archive.

Install or upgrade only once the user agrees:

- macOS or Linux: `brew install ByteMagicInc/tap/agent-p`
- Windows: `scoop bucket add bytemagicinc https://github.com/ByteMagicInc/scoop-bucket`, then `scoop install agent-p`
- With Rust: `cargo install agent-p --locked`
- Otherwise: an archive from <https://github.com/ByteMagicInc/AgentP/releases>, with `agentp` placed on `PATH`

Done when `agentp skill --help` prints help; a declined install or upgrade ends the skill.

## 2. Pick the route

Run `agentp podcast list --no-fetch`; each line starts with the podcast's number. A fresh install fills the list with demo podcasts on its first run; they count as saved. Match the request to one route:

- **Add only**: the user asks to add a podcast and names no episode. Already on the list: there is nothing to add, so go to Report.
- **Saved**: the podcast is on the list (a loose name match counts; confirm only if two entries fit). Keep its number.
- **New**: not on the list, and the user wants to keep it.
- **One-off**: not on the list, and the user wants it once ("just this one", "don't keep it", "remove it after").

Not on the list, with a download asked for but no signal about keeping it: ask New or One-off.

Every route but Saved needs the RSS feed URL:

- Use the URL the user gave; `add` rejects one that is not a feed.
- Otherwise look it up: `https://itunes.apple.com/search?media=podcast&entity=podcast&term=<URL-encoded name>` returns JSON. Take the `feedUrl` of the result whose `collectionName` and `artistName` fit the show, and ask when two fit. When the lookup finds nothing or is blocked, web-search for the show's RSS feed.

Then search `podcasts.json`, in the folder `agentp config path` prints, for the feed's host and path, ignoring scheme, trailing slash and spaces. A match is on the list already: its number is its position in the `podcasts` array, counting from 1. It makes the route **Saved** (for Add only, there is nothing to add), and it stays on the list even if the user said one-off; say so in the report.

Done when you hold one route, and a feed URL unless it is Saved.

## 3. Add the podcast

Skip this step for Saved.

**MP3s:** when the user asks for MP3 files, pass `--convert-to-mp3 true` to the `add` below. Conversion needs `ffmpeg` on `PATH`; `add` prints a warning when it is missing, and then episodes keep the feed's own format.

1. Run `agentp podcast add --feed-url "<url>" --from-feed`; it reads the feed's name, album and artist. When it fails, take its error back to the feed lookup in step 2, and check the list before retrying: the podcast may already be on it.
2. The new podcast is the last line of `agentp podcast list --no-fetch`. Check that its name is the show the user asked for and keep its number. Another show: run `agentp podcast delete --podcast <number>` and return to the feed lookup.

Done when the podcast is on the list and you hold its number. Add only goes to Report.

## 4. Pick the episodes

Address every podcast by its number: `add` accepts duplicates, and a name selector picks the first match.

- **Newest:** `--latest N`, with N = 1 by default; a download request that names no episode means this. `--latest` takes the first N items in feed order, so first run `agentp podcast episodes --podcast <number> --limit 2`: when the first date is older than the second, the feed lists oldest first, and the newest are `--oldest N`.
- **Oldest:** `--oldest N`, with the same check the other way round.
- **Every episode:** `--all`, only when the user says so.
- **One episode by topic, date or number:** run `agentp podcast episodes --podcast <number> --search "<query>"`, building the query from the terms `agentp podcast episodes --help` shows.
  - A relative date ("last week") becomes explicit `since:` and `until:` dates, a few days wider than the request.
  - Words also match episode descriptions, so a title that names the topic beats one that only mentions it.
  - A number the user gives is the show's own: search for it as a word (`500`) and read the titles. `#500` is AgentP's number, which counts the feed's items and can differ.

  One match: pass the number in its first column to `--episodes`. Several: show them with their dates and ask. None: search again without any date term and show what that finds before downloading anything; still none, tell the user.

Numbers count up from the feed's last item, so in the usual newest-first feed the highest is the newest.

Done when you hold one selector (`--latest N`, `--oldest N`, `--episodes A,B` or `--all`), or the user chose none. For One-off with none chosen, run `agentp podcast delete --podcast <number>` and go to Report.

## 5. Download

1. Saved, when the user asks for MP3 files: run `agentp podcast edit --podcast <number> --convert-to-mp3 true` and tell the user it stays on for later downloads. Conversion needs `ffmpeg`, as in step 3.
2. One-off only: keep the `Download folder` line of `agentp podcast show --podcast <number>` for the report.
3. Run `agentp download --podcast <number> <selector>` with a long timeout or in the background: it prints nothing while an episode transfers.
4. One-off only: on exit 0, run `agentp podcast delete --podcast <number>`; the audio stays on disk. On a non-zero exit, leave the entry so a retry needs no re-add, and say so.

`download` exits non-zero when any episode fails to download, write or tag, even when its audio landed before the tag failed. A `blocked URL` line means a web page came back instead of audio, usually from a content filter or proxy, sometimes the host refusing the request. `Warning:` lines leave a usable episode on disk and still exit 0.

Done when `download` has exited and a One-off entry is deleted (exit 0) or kept (failure).

## 6. Report

Done when the user has:

- for a download: the episode titles, the download folder (from `agentp podcast show`, or kept in step 5 for a One-off), and every failure and warning with its text;
- for Add only: the podcast's name, and whether it was added or already there;
- for every route: whether the podcast is on the list now, and any demo podcasts a fresh install added.
