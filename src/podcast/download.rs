//! Episode downloading with optional ID3 tagging.

use std::fs;
use std::fs::File;
use std::io::copy;
use std::path::Path;

use anyhow::Result;
use id3::{Error, ErrorKind, Tag, TagLike};
use reqwest::Url;
use tokio::sync::mpsc;

use super::data::{DownloadEvent, Podcast};

/// Download selected episodes by feed index, writing files and optionally applying ID3 tags.
pub async fn download_selected_episodes(
    main_path: &str,
    podcast: &Podcast,
    feed_indices: Vec<usize>,
    tx: mpsc::UnboundedSender<DownloadEvent>,
) -> Result<()> {
    let content = super::http::client()
        .get(&podcast.feed_url)
        .send()
        .await?
        .bytes()
        .await?;
    let channel = rss::Channel::read_from(&content[..])?;
    let length = channel.items.len();

    let channel_path = Path::new(main_path).join(podcast.effective_album_name());

    if !channel_path.is_dir() {
        fs::create_dir_all(&channel_path)?;
    }

    let total = feed_indices.len();

    for (idx, feed_index) in feed_indices.iter().enumerate() {
        if *feed_index == 0 || *feed_index > length {
            let _ = tx.send(DownloadEvent::Error {
                message: format!(
                    "Episode index {} is out of range (feed has {} episodes) — skipping",
                    feed_index, length
                ),
            });
            continue;
        }
        let rss_idx = length - feed_index;
        let item = &channel.items[rss_idx];
        let enclosure = match item.enclosure.as_ref() {
            Some(enc) => enc,
            None => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Episode at index {} has no enclosure (download URL) — skipping",
                        feed_index
                    ),
                });
                continue;
            }
        };
        let episode_url = &enclosure.url;
        let episode_title = match item.title.as_ref() {
            Some(t) => t.clone(),
            None => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!("Episode at index {} has no title — skipping", feed_index),
                });
                continue;
            }
        };

        let _ = tx.send(DownloadEvent::Started {
            episode_name: episode_title.clone(),
            index: idx + 1,
            total,
        });

        let url = match Url::parse(episode_url) {
            Ok(u) => u,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!("Invalid URL for episode {} — skipping: {}", feed_index, e),
                });
                continue;
            }
        };
        let mut request = super::http::client().get(url.as_ref());
        if !podcast.user_agent.is_empty() {
            request = request.header(reqwest::header::USER_AGENT, &podcast.user_agent);
        }
        let response = match request.send().await {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Failed to download episode {} — skipping: {}",
                        feed_index, e
                    ),
                });
                continue;
            }
        };

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let final_url = response.url().clone();

        if content_type.starts_with("text/") {
            let blocking_domain = response.url().host_str().unwrap_or("unknown").to_string();
            let _ = tx.send(DownloadEvent::Error {
                message: format!(
                    "While downloading Episode {} — {}",
                    feed_index,
                    super::http::filter_block_message(&blocking_domain, url.as_str())
                ),
            });
            continue;
        }

        let content = match response.bytes().await {
            Ok(b) => b,
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Failed to read response for episode {} — skipping: {}",
                        feed_index, e
                    ),
                });
                continue;
            }
        };

        let container = AudioContainer::sniff(&content);
        let file_name = download_file_name(
            podcast.overwrite_file_name,
            &episode_title,
            &url,
            &final_url,
            container,
        );

        let episode_add = channel_path.join(&file_name);

        match File::create(&episode_add) {
            Ok(mut file) => {
                if let Err(e) = copy(&mut content.as_ref(), &mut file) {
                    let _ = tx.send(DownloadEvent::Error {
                        message: format!(
                            "Failed to write episode {} — skipping: {}",
                            feed_index, e
                        ),
                    });
                    continue;
                }
            }
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Failed to create file for episode {} — skipping: {}",
                        feed_index, e
                    ),
                });
                continue;
            }
        }

        let _ = tx.send(DownloadEvent::Completed {
            episode_name: episode_title.clone(),
        });

        if !podcast.overwrite_tags && !podcast.remove_images {
            continue;
        }

        let title = tag_title(podcast, *feed_index, &episode_title);
        let tagged = match container {
            AudioContainer::Mp3 => write_id3_tags(&episode_add, podcast, &title),
            AudioContainer::Mpeg4 => write_mp4_tags(&episode_add, podcast, &title),
            AudioContainer::Unknown => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Episode {} is not MP3 or MP4 audio — saved without tagging",
                        feed_index
                    ),
                });
                continue;
            }
        };

        match tagged {
            Ok(true) => {
                let _ = tx.send(DownloadEvent::Tagged {
                    episode_name: episode_title,
                });
            }
            Ok(false) => {}
            Err(e) => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Failed to write tags for episode {} — skipping tagging: {}",
                        feed_index, e
                    ),
                });
            }
        }
    }

    let _ = tx.send(DownloadEvent::Finished);
    Ok(())
}

/// The audio container of a downloaded episode, sniffed from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AudioContainer {
    Mp3,
    Mpeg4,
    Unknown,
}

impl AudioContainer {
    fn sniff(bytes: &[u8]) -> Self {
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
            Self::Mpeg4
        } else if bytes.starts_with(b"ID3")
            || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        {
            Self::Mp3
        } else {
            Self::Unknown
        }
    }

    fn default_extension(self) -> &'static str {
        match self {
            Self::Mpeg4 => "m4a",
            Self::Mp3 | Self::Unknown => "mp3",
        }
    }
}

fn tag_title(podcast: &Podcast, feed_index: usize, episode_title: &str) -> String {
    if !podcast.append_number_to_title {
        return episode_title.to_string();
    }
    let leading_zeros = match podcast.leading_zeros_amount {
        0 => String::new(),
        _ => get_leading_zeros(&podcast.leading_zeros_amount, feed_index),
    };
    format!("{}{} {}", leading_zeros, feed_index, episode_title)
}

/// Applies the podcast's tag settings as ID3; returns `false` when there was nothing to change.
fn write_id3_tags(path: &Path, podcast: &Podcast, title: &str) -> Result<bool> {
    let mut tag = if podcast.remove_existing_tags && podcast.overwrite_tags {
        Tag::new()
    } else {
        match Tag::read_from_path(path) {
            Ok(tag) => tag,
            Err(Error {
                kind: ErrorKind::NoTag,
                ..
            }) => {
                if podcast.remove_images && !podcast.overwrite_tags {
                    return Ok(false);
                }
                Tag::new()
            }
            Err(err) => return Err(err.into()),
        }
    };

    if podcast.remove_images {
        tag.remove("APIC");
    }

    if podcast.overwrite_tags {
        if podcast.overwrite_title {
            tag.set_title(title);
        }
        if podcast.overwrite_album_name {
            tag.set_album(podcast.effective_album_name());
        }
        if podcast.overwrite_artist {
            tag.set_album_artist(&podcast.artist);
            tag.set_artist(&podcast.artist);
        }
    }

    tag.write_to_path(path, tag.version())?;
    Ok(true)
}

/// Applies the podcast's tag settings as MP4 metadata atoms, leaving the container intact.
fn write_mp4_tags(path: &Path, podcast: &Podcast, title: &str) -> Result<bool> {
    let mut tag = if podcast.remove_existing_tags && podcast.overwrite_tags {
        mp4ameta::Userdata::default()
    } else {
        mp4ameta::Tag::read_from_path(path)?.userdata
    };

    if podcast.remove_images {
        tag.remove_artworks();
    }

    if podcast.overwrite_tags {
        if podcast.overwrite_title {
            tag.set_title(title);
        }
        if podcast.overwrite_album_name {
            tag.set_album(podcast.effective_album_name());
        }
        if podcast.overwrite_artist {
            tag.set_album_artist(&podcast.artist);
            tag.set_artist(&podcast.artist);
        }
    }

    tag.write_to_path(path)?;
    Ok(true)
}

fn get_leading_zeros(leading_zeros_amount: &usize, i: usize) -> String {
    let number_of_zeros = leading_zeros_amount.saturating_sub(i.to_string().len() - 1);
    std::iter::repeat_n('0', number_of_zeros).collect()
}

fn download_file_name(
    overwrite_file_name: bool,
    episode_title: &str,
    request_url: &Url,
    response_url: &Url,
    container: AudioContainer,
) -> String {
    let url_name = url_file_name(request_url).or_else(|| url_file_name(response_url));

    if overwrite_file_name {
        let extension = url_name
            .as_deref()
            .and_then(file_extension)
            .unwrap_or_else(|| container.default_extension().to_string());
        return format!("{}.{}", sanitize_filename(episode_title), extension);
    }

    url_name.unwrap_or_else(|| format!("default.{}", container.default_extension()))
}

fn file_extension(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .filter(|e| !e.is_empty() && e.len() <= 4 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .map(|e| e.to_ascii_lowercase())
}

fn url_file_name(url: &Url) -> Option<String> {
    let segment = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|segment| !segment.is_empty())?;

    let decoded = urlencoding::decode(segment).unwrap_or(std::borrow::Cow::Borrowed(segment));
    if let Ok(nested_url) = Url::parse(&decoded)
        && let Some(name) = nested_url
            .path_segments()
            .and_then(|mut s| s.next_back())
            .filter(|s| !s.is_empty())
            .map(sanitize_filename)
            .filter(|n| !n.is_empty())
    {
        return Some(name);
    }

    let sanitized = sanitize_filename(segment);
    if sanitized.is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

pub(crate) fn sanitize_filename(filename: &str) -> String {
    let options = sanitize_filename::Options {
        truncate: true,
        windows: true,
        replacement: "_",
    };
    sanitize_filename::sanitize_with_options(filename, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_images_strips_embedded_picture_via_round_trip() {
        use id3::Version;
        use id3::frame::{Content, Picture, PictureType};

        let dir = std::env::temp_dir().join("agentp_remove_images_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("episode.mp3");
        File::create(&path).unwrap();

        let mut tag = Tag::new();
        tag.set_title("Episode One");
        tag.add_frame(id3::Frame::with_content(
            "APIC",
            Content::Picture(Picture {
                mime_type: "image/jpeg".to_string(),
                picture_type: PictureType::CoverFront,
                description: "cover".to_string(),
                data: vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46],
            }),
        ));
        tag.write_to_path(&path, Version::Id3v24).unwrap();

        assert_eq!(
            Tag::read_from_path(&path).unwrap().pictures().count(),
            1,
            "sanity: the embedded picture should be present before removal"
        );

        let mut tag = Tag::read_from_path(&path).unwrap();
        tag.remove("APIC");
        tag.write_to_path(&path, tag.version()).unwrap();

        let result = Tag::read_from_path(&path).unwrap();
        assert_eq!(
            result.pictures().count(),
            0,
            "the embedded picture should be removed"
        );
        assert_eq!(
            result.title(),
            Some("Episode One"),
            "non-image tags should be preserved after image removal"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn leading_zeros_3_digits_index_1() {
        assert_eq!(get_leading_zeros(&3, 1), "000");
    }

    #[test]
    fn leading_zeros_3_digits_index_10() {
        assert_eq!(get_leading_zeros(&3, 10), "00");
    }

    #[test]
    fn leading_zeros_3_digits_index_100() {
        assert_eq!(get_leading_zeros(&3, 100), "0");
    }

    #[test]
    fn leading_zeros_zero_returns_empty() {
        assert_eq!(get_leading_zeros(&0, 5), "");
    }

    #[test]
    fn sanitize_normal_filename() {
        let result = sanitize_filename("hello world");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn sanitize_special_chars() {
        let result = sanitize_filename("test/file:name");
        assert!(!result.contains('/'));
        assert!(!result.contains(':'));
    }

    #[test]
    fn overwrite_file_name_true_uses_episode_title() {
        let request_url = Url::parse("https://example.com/request.mp3").unwrap();
        let response_url = Url::parse("https://cdn.example.com/final.mp3").unwrap();

        let result = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &response_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "Episode Title.mp3");
    }

    #[test]
    fn overwrite_file_name_false_prefers_request_url() {
        let request_url = Url::parse("https://example.com/original-file.mp3").unwrap();
        let response_url = Url::parse("https://cdn.example.com/transcoded.mp3").unwrap();

        let result = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &response_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "original-file.mp3");
    }

    #[test]
    fn overwrite_file_name_false_falls_back_to_response_url() {
        let request_url = Url::parse("https://example.com/").unwrap();
        let response_url = Url::parse("https://cdn.example.com/actual-file-123.mp3").unwrap();

        let result = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &response_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "actual-file-123.mp3");
    }

    #[test]
    fn overwrite_file_name_false_defaults_when_urls_have_no_filename() {
        let request_url = Url::parse("https://example.com/").unwrap();
        let response_url = Url::parse("https://cdn.example.com/").unwrap();

        let result = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &response_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "default.mp3");
    }

    #[test]
    fn url_file_name_extracts_from_encoded_nested_url() {
        let url = Url::parse("https://anchor.fm/s/abc123/podcast/play/118326598/https%3A%2F%2Fd3ctxlq1ktw2nl.cloudfront.net%2Fstaging%2F2026-3-12%2Fepisode-file.mp3").unwrap();

        let result = url_file_name(&url);

        assert_eq!(result, Some("episode-file.mp3".to_string()));
    }

    #[test]
    fn url_file_name_normal_segment_unchanged() {
        let url = Url::parse("https://example.com/test.mp3").unwrap();

        let result = url_file_name(&url);

        assert_eq!(result, Some("test.mp3".to_string()));
    }

    #[test]
    fn sniff_detects_mpeg4_by_ftyp_box() {
        let bytes = b"\x00\x00\x00\x1cftypM4A \x00\x00\x02\x00";

        assert_eq!(AudioContainer::sniff(bytes), AudioContainer::Mpeg4);
    }

    #[test]
    fn sniff_detects_mp3_with_id3_header() {
        assert_eq!(
            AudioContainer::sniff(b"ID3\x04\x00\x00\x00\x00\x01\x1b"),
            AudioContainer::Mp3
        );
    }

    #[test]
    fn sniff_detects_mp3_frame_sync() {
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xFB, 0x90, 0x64]),
            AudioContainer::Mp3
        );
    }

    #[test]
    fn sniff_reports_unknown_for_other_formats() {
        assert_eq!(
            AudioContainer::sniff(b"OggS\x00\x02"),
            AudioContainer::Unknown
        );
        assert_eq!(AudioContainer::sniff(b""), AudioContainer::Unknown);
    }

    #[test]
    fn overwrite_file_name_true_keeps_m4a_extension_from_url() {
        let request_url = Url::parse("https://podcast.latent.space/audio/LL0YmIyBKSU.m4a").unwrap();

        let result = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mpeg4,
        );

        assert_eq!(result, "Episode Title.m4a");
    }

    #[test]
    fn overwrite_file_name_true_uses_container_extension_without_url_extension() {
        let request_url = Url::parse("https://example.com/play/12345").unwrap();

        let result = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mpeg4,
        );

        assert_eq!(result, "Episode Title.m4a");
    }

    #[test]
    fn default_file_name_uses_container_extension() {
        let request_url = Url::parse("https://example.com/").unwrap();

        let result = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mpeg4,
        );

        assert_eq!(result, "default.m4a");
    }
}
