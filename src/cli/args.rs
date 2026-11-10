//! `clap` definitions for every subcommand and flag.

use clap::{ArgGroup, Args, Parser, Subcommand};

use crate::podcast::{BannerStyle, DefaultMode};

#[derive(Parser)]
#[command(
    name = "agentp",
    version,
    about = "AgentP — a fast podcast downloader with customizable ID3 and MP4 tagging",
    long_about = "AgentP is a fast podcast downloader with customizable ID3 and MP4 tagging.\n\n\
        Run without a subcommand to launch the default mode (TUI by default).\n\
        Use subcommands like 'podcast', 'download', 'config', and 'open' for\n\
        headless CLI operation. Set default_mode in config to change the\n\
        default behavior."
)]
pub struct Cli {
    #[arg(
        long,
        visible_alias = "user-mode",
        conflicts_with = "cli",
        global = true,
        help = "Force TUI mode, overriding default_mode in config"
    )]
    pub tui: bool,
    #[arg(
        long,
        visible_alias = "agent-mode",
        global = true,
        help = "Force CLI mode, overriding default_mode in config"
    )]
    pub cli: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Subcommand)]
pub enum Command {
    #[command(about = "Manage podcasts (list, show, add, edit, delete, episodes)")]
    Podcast {
        #[command(subcommand)]
        command: PodcastCommand,
    },
    #[command(about = "Download episodes from a podcast")]
    Download(DownloadArgs),
    #[command(about = "Show or update application configuration")]
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    #[command(about = "Open download or podcast folders")]
    Open {
        #[command(subcommand)]
        command: OpenCommand,
    },
    #[command(
        about = "Print the agentp agent skill (SKILL.md) that matches this version",
        long_about = "Print the agentp agent skill (SKILL.md) that matches this version.\n\n\
            Coding agents such as Claude Code and Codex follow it to download episodes \
            with this CLI. Save it as SKILL.md in an agent's skill folder, for example:\n\n  \
            mkdir -p ~/.claude/skills/agentp && agentp skill > ~/.claude/skills/agentp/SKILL.md\n\n\
            In Windows PowerShell 5.1, whose > writes UTF-16, go through cmd:\n\n  \
            cmd /c 'mkdir \"%USERPROFILE%\\.claude\\skills\\agentp\" 2>nul & \
            agentp skill > \"%USERPROFILE%\\.claude\\skills\\agentp\\SKILL.md\"'\n\n\
            `npx skills add ByteMagicInc/AgentP --skill agentp -g` installs the copy on the \
            repository's master branch instead. It can be ahead of the latest release: it \
            offers to upgrade an older AgentP, and stops when no newer release is out yet."
    )]
    Skill,
}

#[derive(Subcommand)]
pub enum PodcastCommand {
    #[command(about = "List all configured podcasts with latest episode info")]
    List(ListPodcastArgs),
    #[command(about = "Show details for a podcast")]
    Show(ShowPodcastArgs),
    #[command(about = "List episodes for a podcast")]
    Episodes(EpisodesArgs),
    #[command(about = "Add a new podcast")]
    Add(AddPodcastArgs),
    #[command(about = "Edit an existing podcast")]
    Edit(EditPodcastArgs),
    #[command(about = "Delete a podcast")]
    Delete(DeletePodcastArgs),
}

#[derive(Args)]
pub struct ListPodcastArgs {
    #[arg(long, help = "Skip fetching latest episode info (faster)")]
    pub no_fetch: bool,
}

#[derive(Args)]
pub struct ShowPodcastArgs {
    #[arg(long, help = "Podcast name or 1-based index")]
    pub podcast: String,
}

#[derive(Args)]
pub struct EpisodesArgs {
    #[arg(long, help = "Podcast name or 1-based index")]
    pub podcast: String,
    #[arg(long, help = "Maximum number of episodes to show")]
    pub limit: Option<usize>,
    #[arg(
        long,
        value_name = "QUERY",
        allow_hyphen_values = true,
        help = "Show only episodes matching QUERY, as the TUI's / search does: words, \"phrases\", -exclusions, a date, month first or year first (11/2/2026, 2026-11-02), a month (11-2026, 2026-11) or a day in any year (11/2), #500 or #100-120 episode numbers, and since:/until:/after:/before: ranges taking a date, a month, a year, today, yesterday or 30d/2w/6m/1y"
    )]
    pub search: Option<String>,
}

#[derive(Args)]
#[command(group(
    ArgGroup::new("download_selector")
        .required(true)
        .args(["episodes", "latest", "oldest", "all"])
))]
pub struct DownloadArgs {
    #[arg(long, help = "Podcast name or 1-based index")]
    pub podcast: String,
    #[arg(long, value_delimiter = ',', num_args = 1.., help = "Episode indices to download (comma-separated)")]
    pub episodes: Option<Vec<usize>>,
    #[arg(
        long,
        help = "Download the first N episodes in feed order (the newest, in a feed that lists newest first)"
    )]
    pub latest: Option<usize>,
    #[arg(
        long,
        help = "Download the last N episodes in feed order (the oldest, in a feed that lists newest first)"
    )]
    pub oldest: Option<usize>,
    #[arg(long, help = "Download all episodes")]
    pub all: bool,
}

#[derive(Args)]
pub struct PodcastFieldArgs {
    #[arg(long, help = "Album name for downloads folder and album tag")]
    pub album_name: Option<String>,
    #[arg(long, help = "Artist name for the artist tag")]
    pub artist: Option<String>,
    #[arg(
        long,
        help = "Custom User-Agent for this podcast's downloads (blank = built-in default)"
    )]
    pub user_agent: Option<String>,
    #[arg(
        long,
        help = "Write ID3 tags (MP3) or MP4 metadata (M4A) to downloaded files"
    )]
    pub overwrite_tags: Option<bool>,
    #[arg(
        long,
        help = "Remove source file's existing tags before writing a fresh minimal one \
            (useful when the source has huge embedded metadata)"
    )]
    pub remove_existing_tags: Option<bool>,
    #[arg(
        long,
        visible_alias = "remove-image",
        help = "Remove embedded cover art from downloaded audio files"
    )]
    pub remove_images: Option<bool>,
    #[arg(long, help = "Write album name into the album tag")]
    pub overwrite_album_name: Option<bool>,
    #[arg(long, help = "Write artist into the artist tag")]
    pub overwrite_artist: Option<bool>,
    #[arg(long, help = "Rename files to match episode title")]
    pub overwrite_file_name: Option<bool>,
    #[arg(
        long,
        help = "Re-encode episodes served as M4A, AAC, Ogg, FLAC or WAV to MP3 with ffmpeg (off by default); the original is kept when ffmpeg is missing or fails"
    )]
    pub convert_to_mp3: Option<bool>,
    #[arg(long, help = "Write episode title into the title tag")]
    pub overwrite_title: Option<bool>,
    #[arg(long, help = "Prepend episode number to title tag")]
    pub append_number_to_title: Option<bool>,
    #[arg(long, help = "Number of leading zeros before episode number (0 = off)")]
    pub leading_zeros_amount: Option<usize>,
}

#[derive(Args)]
pub struct AddPodcastArgs {
    #[arg(long, help = "Podcast display name (optional with --from-feed)")]
    pub name: Option<String>,
    #[arg(long, help = "RSS feed URL")]
    pub feed_url: String,
    #[arg(long, help = "Auto-fill name, album, and artist from the RSS feed")]
    pub from_feed: bool,
    #[command(flatten)]
    pub fields: PodcastFieldArgs,
}

#[derive(Args)]
pub struct EditPodcastArgs {
    #[arg(long, help = "Podcast name or 1-based index to edit")]
    pub podcast: String,
    #[arg(long, help = "New podcast display name")]
    pub name: Option<String>,
    #[arg(long, help = "New RSS feed URL")]
    pub feed_url: Option<String>,
    #[command(flatten)]
    pub fields: PodcastFieldArgs,
}

#[derive(Args)]
pub struct DeletePodcastArgs {
    #[arg(long, help = "Podcast name or 1-based index to delete")]
    pub podcast: String,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    #[command(about = "Show current configuration")]
    Show,
    #[command(about = "Update configuration settings")]
    Set(ConfigSetArgs),
    #[command(about = "Print the config directory path")]
    Path,
}

#[derive(Args)]
pub struct ConfigSetArgs {
    #[arg(long, help = "Set the download directory path")]
    pub download_dir: Option<String>,
    #[arg(long, value_enum, help = "Set the default launch mode (tui or cli)")]
    pub default_mode: Option<DefaultMode>,
    #[arg(
        long,
        value_enum,
        help = "Set the TUI banner wordmark (joined or ascii)"
    )]
    pub banner_style: Option<BannerStyle>,
}

#[derive(Subcommand)]
pub enum OpenCommand {
    #[command(about = "Open the download folder")]
    Downloads {
        #[arg(long, help = "Create the folder if it does not exist")]
        create: bool,
    },
    #[command(about = "Open a podcast's download folder")]
    Podcast(OpenPodcastArgs),
}

#[derive(Args)]
pub struct OpenPodcastArgs {
    #[arg(long, help = "Podcast name or 1-based index")]
    pub podcast: String,
    #[arg(long, help = "Create the folder if it does not exist")]
    pub create: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn search_of(args: &[&str]) -> Option<String> {
        match Cli::try_parse_from(args).ok()?.command? {
            Command::Podcast {
                command: PodcastCommand::Episodes(episodes),
            } => episodes.search,
            _ => None,
        }
    }

    #[test]
    fn search_accepts_a_query_that_starts_with_an_exclusion() {
        let base = [
            "agentp",
            "podcast",
            "episodes",
            "--podcast",
            "1",
            "--search",
        ];
        for query in ["-intro", "-\"live show\"", "-11/2026 copilot"] {
            let args: Vec<&str> = base.iter().copied().chain([query]).collect();
            assert_eq!(search_of(&args).as_deref(), Some(query), "{query}");
        }
    }
}
