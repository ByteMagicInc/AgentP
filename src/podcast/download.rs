//! Episode downloading with optional MP3 conversion and tagging.

use std::fs;
use std::fs::File;
use std::io::{self, copy};
use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::Result;
use id3::{Error, ErrorKind, Tag, TagLike};
use reqwest::Url;
use tokio::sync::mpsc;

use super::data::{DownloadEvent, Podcast};

/// Download selected episodes by feed index, writing files, optionally converting non-MP3
/// audio to MP3 with `ffmpeg`, and optionally applying tags.
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
    let mut ffmpeg_missing = false;

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

        let mut container = container;
        let mut episode_add = episode_add;
        if podcast.convert_to_mp3 && container == AudioContainer::Unknown {
            let _ = tx.send(DownloadEvent::Error {
                message: format!(
                    "Episode {} is not a recognised audio format — saved as downloaded, \
                        without converting to MP3 or tagging",
                    feed_index
                ),
            });
            continue;
        }
        if podcast.convert_to_mp3
            && !ffmpeg_missing
            && let Some(input_format) = container.ffmpeg_input_format()
        {
            let target = episode_add.with_extension("mp3");
            match convert_to_mp3(&episode_add, &target, input_format).await {
                Conversion::Converted { leftover } => {
                    if let Some(reason) = leftover {
                        let _ = tx.send(DownloadEvent::Error {
                            message: format!(
                                "Converted episode {} to MP3 but could not delete the original {} — remove it by hand: {}",
                                feed_index,
                                episode_add.display(),
                                reason
                            ),
                        });
                    }
                    episode_add = target;
                    container = AudioContainer::Mp3;
                    let _ = tx.send(DownloadEvent::Converted {
                        episode_name: episode_title.clone(),
                    });
                }
                Conversion::FfmpegMissing => {
                    ffmpeg_missing = true;
                    let _ = tx.send(DownloadEvent::Error {
                        message: "ffmpeg was not found on PATH, so episodes are kept in their \
                            original format — install ffmpeg or turn off Convert To MP3"
                            .to_string(),
                    });
                }
                Conversion::Failed(reason) => {
                    let _ = tx.send(DownloadEvent::Error {
                        message: format!(
                            "Could not convert episode {} to MP3 — kept the original file: {}",
                            feed_index, reason
                        ),
                    });
                }
            }
        }

        if !podcast.overwrite_tags && !podcast.remove_images {
            continue;
        }

        let title = tag_title(podcast, *feed_index, &episode_title);
        let tagged = match container {
            AudioContainer::Mp3 => write_id3_tags(&episode_add, podcast, &title),
            AudioContainer::Mpeg4 => write_mp4_tags(&episode_add, podcast, &title),
            AudioContainer::Adts
            | AudioContainer::Ogg
            | AudioContainer::Flac
            | AudioContainer::Wav => {
                let _ = tx.send(DownloadEvent::Error {
                    message: format!(
                        "Episode {} is {} audio, which AgentP does not tag — saved without tagging",
                        feed_index,
                        container.label()
                    ),
                });
                continue;
            }
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
    Adts,
    Ogg,
    Flac,
    Wav,
    Unknown,
}

impl AudioContainer {
    /// MPEG audio frames and AAC ADTS frames share the leading sync bits; ADTS always
    /// sets the two layer bits to `00`, which no MPEG audio frame uses.
    fn sniff(bytes: &[u8]) -> Self {
        if bytes.len() >= 8 && &bytes[4..8] == b"ftyp" {
            return Self::Mpeg4;
        }
        if bytes.starts_with(b"OggS") {
            return Self::Ogg;
        }
        if bytes.starts_with(b"fLaC") {
            return Self::Flac;
        }
        if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE" {
            return Self::Wav;
        }
        if bytes.starts_with(b"ID3") {
            return Self::Mp3;
        }
        if bytes.len() < 2 || bytes[0] != 0xFF || bytes[1] & 0xE0 != 0xE0 {
            return Self::Unknown;
        }
        let version = (bytes[1] >> 3) & 0b11;
        let layer = (bytes[1] >> 1) & 0b11;
        if layer == 0 {
            if bytes[1] & 0xF0 == 0xF0 {
                Self::Adts
            } else {
                Self::Unknown
            }
        } else if version == 0b01 {
            Self::Unknown
        } else {
            Self::Mp3
        }
    }

    fn default_extension(self) -> &'static str {
        match self {
            Self::Mpeg4 => "m4a",
            Self::Adts => "aac",
            Self::Ogg => "ogg",
            Self::Flac => "flac",
            Self::Wav => "wav",
            Self::Mp3 | Self::Unknown => "mp3",
        }
    }

    /// Audio extensions a file of this container may keep from its URL.
    fn allowed_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Mpeg4 => &["m4a", "m4b", "mp4"],
            Self::Mp3 => &["mp3"],
            Self::Adts => &["aac"],
            Self::Ogg => &["ogg", "oga", "opus"],
            Self::Flac => &["flac"],
            Self::Wav => &["wav"],
            Self::Unknown => &["mp3", "ogg", "oga", "opus", "flac", "wav"],
        }
    }

    /// The `ffmpeg` demuxer for this container, passed with `-f` so `ffmpeg` never
    /// probes an untrusted file into a playlist or concat demuxer. `None` means the
    /// file is not converted: it is already MP3, or it was not positively identified.
    fn ffmpeg_input_format(self) -> Option<&'static str> {
        match self {
            Self::Mpeg4 => Some("mov"),
            Self::Adts => Some("aac"),
            Self::Ogg => Some("ogg"),
            Self::Flac => Some("flac"),
            Self::Wav => Some("wav"),
            Self::Mp3 | Self::Unknown => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Mp3 => "MP3",
            Self::Mpeg4 => "MP4",
            Self::Adts => "raw AAC",
            Self::Ogg => "Ogg",
            Self::Flac => "FLAC",
            Self::Wav => "WAV",
            Self::Unknown => "unrecognised",
        }
    }

    fn accepts_extension(self, extension: &str) -> bool {
        self.allowed_extensions().contains(&extension)
    }
}

/// Outcome of re-encoding one episode to MP3.
enum Conversion {
    /// The MP3 is in place; carries the reason when the original could not be deleted.
    Converted {
        leftover: Option<String>,
    },
    FfmpegMissing,
    Failed(String),
}

/// Re-encodes `source` to MP3 at `target` with `ffmpeg`, keeping cover art and metadata.
///
/// `ffmpeg` writes to a `.part` file next to `target`, which is renamed into place only
/// after it succeeds, and `source` is removed only after that, so every failure leaves
/// the original download on disk. `source` and `target` may be the same path. When
/// copying a video stream fails, which happens for real video rather than cover art,
/// the conversion is retried once with video dropped.
async fn convert_to_mp3(source: &Path, target: &Path, input_format: &str) -> Conversion {
    let (source, target) = match (std::path::absolute(source), std::path::absolute(target)) {
        (Ok(source), Ok(target)) => (source, target),
        (Err(e), _) | (_, Err(e)) => {
            return Conversion::Failed(format!("could not resolve the file path: {}", e));
        }
    };
    let mut partial = target.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);

    let mut result = run_ffmpeg(&source, &partial, input_format, FFMPEG_KEEP_ART_ARGS).await;
    if matches!(result, FfmpegRun::Failed(_)) {
        result = run_ffmpeg(&source, &partial, input_format, FFMPEG_DROP_VIDEO_ARGS).await;
    }

    match result {
        FfmpegRun::Succeeded => {}
        FfmpegRun::Missing => return Conversion::FfmpegMissing,
        FfmpegRun::Failed(reason) => {
            let _ = fs::remove_file(&partial);
            return Conversion::Failed(reason);
        }
    }

    if let Err(e) = fs::rename(&partial, &target) {
        let _ = fs::remove_file(&partial);
        return Conversion::Failed(format!("could not move the MP3 into place: {}", e));
    }

    let leftover = if source != target {
        fs::remove_file(&source).err().map(|e| e.to_string())
    } else {
        None
    };

    Conversion::Converted { leftover }
}

enum FfmpegRun {
    Succeeded,
    Missing,
    Failed(String),
}

/// Runs one `ffmpeg` conversion from `source` to `output`, both absolute paths.
///
/// The input demuxer is forced with `-f`, only the `file` protocol is allowed, and both
/// paths carry a `file:` prefix so a path can never be read as an option or a URL.
async fn run_ffmpeg(
    source: &Path,
    output: &Path,
    input_format: &str,
    stream_args: &[&str],
) -> FfmpegRun {
    let result = tokio::process::Command::new("ffmpeg")
        .args(FFMPEG_INPUT_ARGS)
        .args(["-f", input_format, "-i"])
        .arg(file_url(source))
        .args(stream_args)
        .args(FFMPEG_OUTPUT_ARGS)
        .arg(file_url(output))
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await;

    match result {
        Ok(out) if out.status.success() => FfmpegRun::Succeeded,
        Ok(out) => FfmpegRun::Failed(
            ffmpeg_failure_reason(&out.stderr)
                .unwrap_or_else(|| format!("ffmpeg exited with {}", out.status)),
        ),
        Err(e) if e.kind() == io::ErrorKind::NotFound => FfmpegRun::Missing,
        Err(e) => FfmpegRun::Failed(format!("could not run ffmpeg: {}", e)),
    }
}

fn file_url(path: &Path) -> std::ffi::OsString {
    let mut url = std::ffi::OsString::from("file:");
    url.push(path.as_os_str());
    url
}

/// Global and input options: overwrite, never read the terminal, errors only, and no
/// protocol but local files.
const FFMPEG_INPUT_ARGS: &[&str] = &[
    "-y",
    "-nostdin",
    "-loglevel",
    "error",
    "-protocol_whitelist",
    "file",
];

/// First audio stream, plus cover art copied as an attached picture when present.
const FFMPEG_KEEP_ART_ARGS: &[&str] = &["-map", "0:a:0", "-map", "0:v?", "-c:v", "copy"];

/// First audio stream only, for sources whose video stream MP3 cannot hold.
const FFMPEG_DROP_VIDEO_ARGS: &[&str] = &["-map", "0:a:0", "-vn"];

/// Encoding: LAME VBR quality 4 (about 130–165 kbps) with the source metadata kept.
const FFMPEG_OUTPUT_ARGS: &[&str] = &[
    "-map_metadata",
    "0",
    "-codec:a",
    "libmp3lame",
    "-q:a",
    "4",
    "-id3v2_version",
    "3",
    "-f",
    "mp3",
];

/// The last non-empty line of `ffmpeg`'s stderr, cut to 200 characters.
fn ffmpeg_failure_reason(stderr: &[u8]) -> Option<String> {
    let stderr = String::from_utf8_lossy(stderr);
    let line = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())?;
    if line.chars().count() > 200 {
        Some(format!("{}…", line.chars().take(200).collect::<String>()))
    } else {
        Some(line.to_string())
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

    let url_extension = url_name
        .as_deref()
        .and_then(file_extension)
        .filter(|e| container.accepts_extension(e));

    if overwrite_file_name {
        let extension = url_extension.unwrap_or_else(|| container.default_extension().to_string());
        return format!("{}.{}", sanitize_filename(episode_title), extension);
    }

    match url_name {
        Some(name) if url_extension.is_some() => name,
        Some(name) => format!("{}.{}", name, container.default_extension()),
        None => format!("default.{}", container.default_extension()),
    }
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
            AudioContainer::sniff(b"<html><body>"),
            AudioContainer::Unknown
        );
        assert_eq!(AudioContainer::sniff(b""), AudioContainer::Unknown);
    }

    #[test]
    fn sniff_detects_ogg_flac_and_wav_by_magic_bytes() {
        assert_eq!(
            AudioContainer::sniff(b"OggS\x00\x02\x00\x00"),
            AudioContainer::Ogg
        );
        assert_eq!(
            AudioContainer::sniff(b"fLaC\x00\x00\x00\x22"),
            AudioContainer::Flac
        );
        assert_eq!(
            AudioContainer::sniff(b"RIFF\x24\x08\x00\x00WAVEfmt "),
            AudioContainer::Wav
        );
        assert_eq!(
            AudioContainer::sniff(b"RIFF\x24\x08\x00\x00AVI LIST"),
            AudioContainer::Unknown
        );
    }

    #[test]
    fn only_positively_identified_non_mp3_formats_are_converted() {
        assert_eq!(AudioContainer::Mpeg4.ffmpeg_input_format(), Some("mov"));
        assert_eq!(AudioContainer::Adts.ffmpeg_input_format(), Some("aac"));
        assert_eq!(AudioContainer::Ogg.ffmpeg_input_format(), Some("ogg"));
        assert_eq!(AudioContainer::Flac.ffmpeg_input_format(), Some("flac"));
        assert_eq!(AudioContainer::Wav.ffmpeg_input_format(), Some("wav"));
        assert_eq!(AudioContainer::Mp3.ffmpeg_input_format(), None);
        assert_eq!(
            AudioContainer::Unknown.ffmpeg_input_format(),
            None,
            "an unidentified file must never reach ffmpeg, which would probe it"
        );
    }

    #[test]
    fn sniff_treats_playlists_and_concat_lists_as_unknown() {
        assert_eq!(
            AudioContainer::sniff(b"#EXTM3U\n#EXT-X-TARGETDURATION:1\n"),
            AudioContainer::Unknown
        );
        assert_eq!(
            AudioContainer::sniff(b"ffconcat version 1.0\nfile /etc/passwd\n"),
            AudioContainer::Unknown
        );
    }

    #[test]
    fn ffmpeg_paths_carry_a_file_prefix() {
        let url = file_url(Path::new("-i evil.m4a"));

        assert_eq!(url, std::ffi::OsString::from("file:-i evil.m4a"));
    }
    #[test]
    fn sniff_detects_raw_aac_adts_instead_of_mp3() {
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xF1, 0x50, 0x80]),
            AudioContainer::Adts
        );
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xF9, 0x50, 0x80]),
            AudioContainer::Adts
        );
    }

    #[test]
    fn sniff_detects_mpeg2_and_mpeg25_layer3_as_mp3() {
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xF3, 0x90, 0x64]),
            AudioContainer::Mp3
        );
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xE3, 0x90, 0x64]),
            AudioContainer::Mp3
        );
    }

    #[test]
    fn sniff_rejects_reserved_mpeg_version() {
        assert_eq!(
            AudioContainer::sniff(&[0xFF, 0xEA, 0x90, 0x64]),
            AudioContainer::Unknown
        );
    }

    #[test]
    fn overwrite_file_name_true_names_raw_aac_with_aac_extension() {
        let request_url = Url::parse("https://example.com/play/12345").unwrap();

        let result = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Adts,
        );

        assert_eq!(result, "Episode Title.aac");
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
    fn overwrite_file_name_true_rejects_non_audio_url_extension() {
        let request_url = Url::parse("https://evil.example.com/payload.exe").unwrap();

        let result = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "Episode Title.mp3");
    }

    #[test]
    fn overwrite_file_name_true_rejects_extension_mismatching_container() {
        let request_url = Url::parse("https://example.com/episode.mp3").unwrap();

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
    fn url_file_name_with_non_audio_extension_gets_audio_extension_appended() {
        let request_url = Url::parse("https://evil.example.com/payload.exe").unwrap();

        let result = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mp3,
        );

        assert_eq!(result, "payload.exe.mp3");
    }

    #[test]
    fn converted_episode_keeps_url_stem_with_mp3_extension() {
        let request_url = Url::parse("https://podcast.latent.space/audio/LL0YmIyBKSU.m4a").unwrap();

        let name = download_file_name(
            false,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Mpeg4,
        );

        assert_eq!(
            Path::new(&name).with_extension("mp3"),
            Path::new("LL0YmIyBKSU.mp3")
        );
    }

    #[test]
    fn converted_renamed_episode_uses_title_with_mp3_extension() {
        let request_url = Url::parse("https://example.com/play/12345").unwrap();

        let name = download_file_name(
            true,
            "Episode Title",
            &request_url,
            &request_url,
            AudioContainer::Adts,
        );

        assert_eq!(
            Path::new(&name).with_extension("mp3"),
            Path::new("Episode Title.mp3")
        );
    }

    #[test]
    fn ffmpeg_failure_reason_uses_last_stderr_line() {
        let stderr = b"first line
Invalid data found when processing input

";

        assert_eq!(
            ffmpeg_failure_reason(stderr).as_deref(),
            Some("Invalid data found when processing input")
        );
        assert_eq!(
            ffmpeg_failure_reason(
                b"
  
"
            ),
            None
        );
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
