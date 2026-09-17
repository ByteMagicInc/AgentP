//! Reading and writing `config.json` and `podcasts.json` from the AgentP config directory.

use std::fs;
use std::fs::File;
use std::io::BufReader;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::data::{BannerStyle, Config, DefaultMode, Podcast};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ConfigFile {
    download_dir_location: String,
    #[serde(default)]
    default_podcast: Podcast,
    #[serde(default)]
    default_mode: DefaultMode,
    #[serde(default)]
    banner_style: BannerStyle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PodcastsFile {
    podcasts: Vec<Podcast>,
}

/// Return the default download directory path, expanding `~/` to the user's home.
pub fn default_download_dir_location() -> String {
    #[derive(Deserialize)]
    struct Defaults {
        download_dir_location: String,
    }
    let defaults: Defaults = serde_json::from_str(include_str!("../../default.config.json"))
        .expect("default.config.json must be valid");
    let raw = defaults.download_dir_location;
    if raw.starts_with("~/") {
        dirs::home_dir()
            .map(|h| {
                let mut p = h;
                for seg in raw[2..].split('/') {
                    p = p.join(seg);
                }
                let mut s = p.to_string_lossy().to_string();
                if !s.ends_with(std::path::MAIN_SEPARATOR) {
                    s.push(std::path::MAIN_SEPARATOR);
                }
                s
            })
            .unwrap_or(raw)
    } else {
        raw
    }
}

/// Return the path to the AgentP config directory (`~/.config/AgentP`).
pub fn config_dir() -> Result<std::path::PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(".config").join("AgentP"))
}

/// Return the full path to `config.json`.
pub fn config_path() -> Result<std::path::PathBuf> {
    Ok(config_dir()?.join("config.json"))
}

/// Write both `config.json` and `podcasts.json` to the config directory.
pub fn save_config(config: &Config) -> Result<()> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;

    let config_file = ConfigFile {
        download_dir_location: config.download_dir_location.clone(),
        default_podcast: config.default_podcast.clone(),
        default_mode: config.default_mode,
        banner_style: config.banner_style,
    };
    fs::write(
        dir.join("config.json"),
        serde_json::to_string_pretty(&config_file)?,
    )?;

    let podcasts_file = PodcastsFile {
        podcasts: config.podcasts.clone(),
    };
    fs::write(
        dir.join("podcasts.json"),
        serde_json::to_string_pretty(&podcasts_file)?,
    )?;

    Ok(())
}

/// Load config from disk, creating default files if missing. Returns `(config, freshly_created)`.
pub fn load_config() -> Result<(Config, bool)> {
    let dir = config_dir()?;
    fs::create_dir_all(&dir)?;

    let cfg_path = dir.join("config.json");
    let pods_path = dir.join("podcasts.json");

    let mut freshly_created = false;

    if !cfg_path.exists() {
        let config_file = ConfigFile {
            download_dir_location: default_download_dir_location(),
            default_podcast: Podcast::default(),
            default_mode: DefaultMode::default(),
            banner_style: BannerStyle::default(),
        };
        fs::write(&cfg_path, serde_json::to_string_pretty(&config_file)?)?;
        freshly_created = true;
    }

    if !pods_path.exists() {
        let example: PodcastsFile =
            serde_json::from_str(include_str!("../../example.podcasts.json"))?;
        let mut podcasts = example.podcasts;
        podcasts.retain(|p| !p.name.is_empty() && !p.feed_url.is_empty());
        let podcasts_file = PodcastsFile { podcasts };
        fs::write(&pods_path, serde_json::to_string_pretty(&podcasts_file)?)?;
        freshly_created = true;
    }

    let cfg: ConfigFile = serde_json::from_reader(BufReader::new(
        File::open(&cfg_path).with_context(|| format!("Failed to open {}", cfg_path.display()))?,
    ))?;

    let pods: PodcastsFile = serde_json::from_reader(BufReader::new(
        File::open(&pods_path)
            .with_context(|| format!("Failed to open {}", pods_path.display()))?,
    ))?;

    let mut config = Config {
        download_dir_location: cfg.download_dir_location,
        podcasts: pods.podcasts,
        default_podcast: cfg.default_podcast,
        default_mode: cfg.default_mode,
        banner_style: cfg.banner_style,
    };
    config
        .podcasts
        .retain(|p| !p.name.is_empty() && !p.feed_url.is_empty());

    Ok((config, freshly_created))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_dir_ends_with_agent_p() {
        let dir = config_dir().unwrap();
        let s = dir.to_string_lossy();
        assert!(
            s.ends_with(".config/AgentP") || s.ends_with(".config\\AgentP"),
            "config_dir should end with .config/AgentP or .config\\AgentP, got: {}",
            s
        );
    }

    #[test]
    fn config_path_ends_with_config_json() {
        let path = config_path().unwrap();
        let s = path.to_string_lossy();
        assert!(
            s.ends_with("config.json"),
            "config_path should end with config.json, got: {}",
            s
        );
    }

    #[test]
    fn config_without_a_banner_style_keeps_loading_and_defaults_to_joined() {
        let cfg: ConfigFile =
            serde_json::from_str(r#"{"download_dir_location": "/tmp/pods"}"#).unwrap();
        assert_eq!(cfg.banner_style, BannerStyle::Joined);
    }

    #[test]
    fn banner_style_round_trips_through_json() {
        let cfg: ConfigFile = serde_json::from_str(
            r#"{"download_dir_location": "/tmp/pods", "banner_style": "ascii"}"#,
        )
        .unwrap();
        assert_eq!(cfg.banner_style, BannerStyle::Ascii);
        let text = serde_json::to_string(&cfg).unwrap();
        assert!(text.contains("\"banner_style\":\"ascii\""), "{text}");
    }

    #[test]
    fn default_download_dir_is_not_empty() {
        let dir = default_download_dir_location();
        assert!(!dir.is_empty());
    }
}
