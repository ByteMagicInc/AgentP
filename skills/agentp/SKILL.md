---
name: agentp
description: Drives the AgentP podcast CLI (`agentp`). Use when the user wants a podcast episode downloaded, found by topic, date or number, or a podcast added to AgentP.
---

`agentp <subcommand> --help` is the flag reference. `podcast add`, `podcast edit`, `podcast delete` and `download` change the user's real podcast list and download folder: run them where a step says to.

## 1. Find the CLI

Run `agentp skill --help`; every later step needs a release that has it.

- **Help prints:** go on.
- **`agentp` is not found:** offer the install for the user's platform.
- **`error: unrecognized subcommand 'skill'`:** the installed AgentP is older than this skill. Find how it was installed (`brew list agent-p`, `scoop list agent-p`, or `cargo install --list`; none of them lists it: an archive) and offer that method's upgrade.

Install or upgrade only once the user agrees:

| Method | Install | Upgrade |
|---|---|---|
| Homebrew (macOS, Linux) | `brew install ByteMagicInc/tap/agent-p` | `brew update`, then `brew upgrade agent-p` |
| Scoop (Windows) | `scoop bucket add bytemagicinc https://github.com/ByteMagicInc/scoop-bucket`, then `scoop install agent-p` | `scoop update`, then `scoop update agent-p` |
| Cargo | `cargo install agent-p --locked` | the same command |
| Archive | the platform's archive from <https://github.com/ByteMagicInc/AgentP/releases>, with `agentp` on `PATH` | a newer archive over the old one |

Then run `agentp skill --help` again. The same error after an upgrade means the release this skill needs is not published yet: tell the user that and stop.

Done when `agentp skill --help` prints help; a declined install or upgrade ends the skill.

## 2. Pick the route

If `podcasts.json` is missing from the folder `agentp config path` prints, the next command fills the list with demo podcasts; they count as saved, and the report names them. Run `agentp podcast list --no-fetch`; each line starts with the podcast's number. Match the request to one route:

- **Add only**: the user asks to add a podcast and names no episode.
- **Saved**: the podcast is on the list (a loose name match counts; confirm only if two entries fit).
- **New**: not on the list, and the user wants to keep it.
- **One-off**: not on the list, and the user wants it once ("just this one", "don't keep it", "remove it after").

Not on the list, with a download asked for but no signal about keeping it: ask New or One-off.

Keep the podcast's number and address it by that number everywhere: `add` accepts duplicates, and a name selector picks the first exact match. A podcast whose name is exactly that number (one named `7`) captures `--podcast 7`; if the list has one, tell the user and stop.

Every route but Saved needs the RSS feed URL:

- Use the URL the user gave; `add` rejects one that is not a feed.
- Otherwise look it up: `https://itunes.apple.com/search?media=podcast&entity=podcast&term=<URL-encoded name>` returns JSON. Take the `feedUrl` of the result whose `collectionName` and `artistName` fit the show, and ask when two fit. When it returns no results, or anything but JSON (a redirect or a web page means a filter blocked it), web-search for the show's RSS feed.

Then search `podcasts.json` for the feed's host and path, ignoring scheme, trailing slash and spaces. A match is on the list already, so the route is **Saved** and its number is on the list line with its `name`; it stays on the list even if the user said one-off, and the report says so. For Add only, a match means there is nothing to add: go to Report.

Done when you hold one route, plus a feed URL unless it is Saved, or a number when it is.

## 3. Add the podcast

Skip this step for Saved.

1. Run `agentp podcast add --feed-url "<url>" --from-feed`. It names the podcast after the feed's title and fills empty album and artist defaults from its title and author. When it fails, take its error back to the feed lookup in step 2.
2. The new podcast is the last line of `agentp podcast list --no-fetch`. Check that its name is the show the user asked for and keep its number. Another show: run `agentp podcast delete --podcast <number>` and return to the feed lookup.

Done when the podcast is on the list and you hold its number. Add only goes to Report.

## 4. Pick the episodes

- **Newest:** `--latest N`, with N = 1 by default; a download request that names no episode means this. `--latest` takes the first N items in feed order, so first run `agentp podcast episodes --podcast <number> --limit 2`: when the first date is older than the second, the feed lists oldest first, and the newest are `--oldest N`.
- **Oldest:** `--oldest N`, with the same check the other way round.
- **Every episode:** `--all`, only when the user says so.
- **One episode by topic, date or number:** run `agentp podcast episodes --podcast <number> --search "<query>"`, building the query from the terms `agentp podcast episodes --help` shows.
  - A relative date ("last week") becomes explicit `since:` and `until:` dates. Search three days wider on each side, and keep the dates the user meant for choosing.
  - Words also match episode descriptions.
  - A number the user gives is the show's own: search for it as a word (`500`) and read the titles. When no title carries it, offer `#500`, AgentP's number, which counts up from the feed's last item and can differ.

  Choose a match only when its date falls within the dates the user meant and its title names the topic; download it when exactly one does. Otherwise show the matches with their dates and ask. None: drop the date terms, run the search again with `--limit 10`, show those and ask; still none, tell the user.

Done when you hold one selector (`--latest N`, `--oldest N`, `--episodes A,B` or `--all`), or no episode is chosen: then go to Report, deleting a One-off first with `agentp podcast delete --podcast <number>`.

## 5. Download

1. Run `agentp podcast show --podcast <number>` and keep its `Download folder:` and `Convert to MP3:` lines.
2. **MP3s:** when the user asks for MP3 files and `Convert to MP3:` is `off`, run `agentp podcast edit --podcast <number> --convert-to-mp3 true`. When it warns that ffmpeg is missing, offer `brew install ffmpeg` or `scoop install ffmpeg` before downloading; declined, episodes keep the feed's own format.
3. Run `agentp download --podcast <number> <selector>` with a long timeout or in the background: it prints nothing while an episode transfers.
4. Saved, when step 2 turned MP3 on: run `agentp podcast edit --podcast <number> --convert-to-mp3 false`, so one request leaves its later downloads unchanged.
5. One-off: on exit 0, run `agentp podcast delete --podcast <number>`; the audio stays on disk. On a non-zero exit, leave the entry so a retry needs no re-add, and say so.

`download` exits non-zero when any episode fails to download, write or tag, even when its audio landed before the tag failed. A `blocked URL` line means a web page came back instead of audio, usually from a content filter or proxy, sometimes the host refusing the request. `Warning:` lines leave a usable episode on disk and still exit 0.

Done when `download` has exited, MP3 is back off for a Saved podcast that had it off, and a One-off entry is deleted (exit 0) or kept (failure).

## 6. Report

Done when the user has:

- for a download: the episode titles, the download folder (kept in step 5), and every failure and warning with its text;
- for Add only: the podcast's name, and whether it was added or already there;
- for no episode chosen: what the search found;
- for every route: whether the podcast is on the list now, and any demo podcasts this run added.
