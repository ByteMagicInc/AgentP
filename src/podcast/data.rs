//! Core data types for podcasts, episodes, and download events.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum DefaultMode {
    #[default]
    Tui,
    Cli,
}

impl fmt::Display for DefaultMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefaultMode::Tui => write!(f, "tui"),
            DefaultMode::Cli => write!(f, "cli"),
        }
    }
}

/// Which wordmark the TUI banner draws.
///
/// `Joined` uses box-drawing characters whose strokes meet across cell edges,
/// which needs a terminal that draws those characters itself; `Ascii` is the
/// plain `/ \ | _` art that renders anywhere.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, clap::ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum BannerStyle {
    #[default]
    Joined,
    Ascii,
}

impl fmt::Display for BannerStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BannerStyle::Joined => write!(f, "joined"),
            BannerStyle::Ascii => write!(f, "ascii"),
        }
    }
}

/// A single podcast with its feed URL, naming, and tag-override settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Podcast {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub feed_url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub album_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub artist: String,
    /// Per-podcast `User-Agent` for audio downloads; empty falls back to the
    /// built-in `AgentP/<version>`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub user_agent: String,
    #[serde(alias = "override_tags")]
    pub overwrite_tags: bool,
    #[serde(alias = "override_album_name")]
    pub overwrite_album_name: bool,
    #[serde(alias = "override_artist")]
    pub overwrite_artist: bool,
    #[serde(alias = "override_title")]
    pub overwrite_title: bool,
    #[serde(alias = "override_file_name")]
    pub overwrite_file_name: bool,
    #[serde(default)]
    pub append_number_to_title: bool,
    #[serde(alias = "leading_zeros_to_title")]
    pub leading_zeros_amount: usize,
    /// When true, remove the source file's existing ID3 tag before writing a fresh
    /// minimal tag containing only the frames AgentP sets (title/album/artist).
    /// Useful when the source's existing tag is huge (e.g. Adobe XMP metadata) and
    /// confuses MP3 players. Only applies when `overwrite_tags` is true. Defaults
    /// to false.
    #[serde(default, alias = "create_new_tags")]
    pub remove_existing_tags: bool,
    #[serde(default, alias = "remove_image")]
    pub remove_images: bool,
    /// When true, episodes the feed serves in another format (M4A, AAC, Ogg, ...)
    /// are re-encoded to MP3 with `ffmpeg` after download. The original is kept
    /// when `ffmpeg` is missing or fails. Defaults to true, including for
    /// podcasts saved before the option existed.
    #[serde(default = "default_true")]
    pub convert_to_mp3: bool,
}

fn default_true() -> bool {
    true
}

impl Podcast {
    pub fn effective_album_name(&self) -> &str {
        if self.album_name.is_empty() {
            &self.name
        } else {
            &self.album_name
        }
    }

    pub fn field_text(&self, step: usize) -> String {
        match step {
            0 => self.feed_url.clone(),
            1 => self.name.clone(),
            2 => self.album_name.clone(),
            3 => self.artist.clone(),
            14 => self.user_agent.clone(),
            _ => String::new(),
        }
    }

    pub fn field_bool(&self, step: usize) -> bool {
        match step {
            4 => self.overwrite_tags,
            5 => self.remove_images,
            6 => self.remove_existing_tags,
            7 => self.overwrite_album_name,
            8 => self.overwrite_artist,
            9 => self.overwrite_file_name,
            10 => self.convert_to_mp3,
            11 => self.overwrite_title,
            12 => self.append_number_to_title,
            _ => false,
        }
    }
}

impl Default for Podcast {
    fn default() -> Self {
        serde_json::from_str(include_str!("../../default.config.json"))
            .expect("default.config.json must be valid")
    }
}

/// Top-level configuration combining download path, podcast list, and defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub download_dir_location: String,
    pub podcasts: Vec<Podcast>,
    #[serde(default)]
    pub default_podcast: Podcast,
    #[serde(default)]
    pub default_mode: DefaultMode,
    #[serde(default)]
    pub banner_style: BannerStyle,
}

/// Metadata for a single episode parsed from an RSS feed.
#[derive(Debug, Clone)]
pub struct EpisodeInfo {
    pub title: String,
    pub feed_index: usize,
    pub pub_date: Option<String>,
    /// The feed's description of the episode as plain text; empty when it has none.
    pub description: String,
}

/// Progress events sent from the download task to the UI over mpsc.
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    Started {
        episode_name: String,
        index: usize,
        total: usize,
    },
    Completed {
        episode_name: String,
    },
    Converted {
        episode_name: String,
    },
    Tagged {
        episode_name: String,
    },
    Error {
        message: String,
    },
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_to_mp3_defaults_to_true_when_missing_from_saved_json() {
        let podcast: Podcast = serde_json::from_str(
            r#"{
                "name": "Old Podcast",
                "feed_url": "https://example.com/feed.xml",
                "overwrite_tags": true,
                "overwrite_album_name": true,
                "overwrite_artist": true,
                "overwrite_title": true,
                "overwrite_file_name": false,
                "leading_zeros_amount": 3
            }"#,
        )
        .unwrap();

        assert!(podcast.convert_to_mp3);
    }

    #[test]
    fn convert_to_mp3_false_round_trips() {
        let podcast = Podcast {
            convert_to_mp3: false,
            ..Podcast::default()
        };

        let json = serde_json::to_string(&podcast).unwrap();
        let back: Podcast = serde_json::from_str(&json).unwrap();

        assert!(json.contains("\"convert_to_mp3\":false"));
        assert!(!back.convert_to_mp3);
    }

    #[test]
    fn new_podcast_template_converts_to_mp3_by_default() {
        assert!(Podcast::default().convert_to_mp3);
        assert!(Podcast::default().field_bool(10));
    }
}
