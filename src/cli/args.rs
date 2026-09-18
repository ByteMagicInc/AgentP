use clap::{ArgGroup, Args, Parser, Subcommand};

use crate::podcast::{BannerStyle, DefaultMode};

#[derive(Parser)]
#[command(
    name = "agentp",
    version,
    about = "AgentP — a fast podcast downloader with customizable ID3 tagging",
    long_about = "AgentP is a fast podcast downloader with customizable ID3 tagging.\n\n\
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
    #[arg(long, help = "Download the N newest episodes")]
    pub latest: Option<usize>,
    #[arg(long, help = "Download the N oldest episodes")]
    pub oldest: Option<usize>,
    #[arg(long, help = "Download all episodes")]
    pub all: bool,
}

#[derive(Args)]
pub struct PodcastFieldArgs {
    #[arg(long, help = "Album name for downloads folder and ID3 tag")]
    pub album_name: Option<String>,
    #[arg(long, help = "Artist name for ID3 tag")]
    pub artist: Option<String>,
    #[arg(
        long,
        help = "Custom User-Agent for this podcast's downloads (blank = built-in default)"
    )]
    pub user_agent: Option<String>,
    #[arg(long, help = "Write ID3 tags to downloaded files")]
    pub overwrite_tags: Option<bool>,
    #[arg(
        long,
        help = "Remove source file's existing ID3 tag before writing a fresh minimal one \
            (useful when the source has huge embedded metadata)"
    )]
    pub remove_existing_tags: Option<bool>,
    #[arg(
        long,
        visible_alias = "remove-image",
        help = "Remove embedded cover art from downloaded audio files"
    )]
    pub remove_images: Option<bool>,
    #[arg(long, help = "Write album name into ID3 album tag")]
    pub overwrite_album_name: Option<bool>,
    #[arg(long, help = "Write artist into ID3 artist tag")]
    pub overwrite_artist: Option<bool>,
    #[arg(long, help = "Rename files to match episode title")]
    pub overwrite_file_name: Option<bool>,
    #[arg(long, help = "Write episode title into ID3 title tag")]
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
