//! RSS feed fetching: latest episode title, full episode list, and channel metadata.

use anyhow::{Context, Result};
use reqwest::StatusCode;
use rss::Channel;

use super::data::{EpisodeInfo, Podcast};

/// Channel-level metadata extracted from an RSS feed.
#[derive(Debug, Clone)]
pub struct FeedMetadata {
    pub title: String,
    pub author: String,
}

async fn fetch_channel(url: &str) -> Result<Channel> {
    let response = super::http::client()
        .get(url)
        .send()
        .await
        .context("could not reach the feed")?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let host = response.url().host_str().unwrap_or("unknown").to_string();
    let body = response
        .bytes()
        .await
        .context("failed to read the feed response")?;

    parse_feed_response(url, &host, status, &content_type, &body)
}

fn parse_feed_response(
    url: &str,
    host: &str,
    status: StatusCode,
    content_type: &str,
    body: &[u8],
) -> Result<Channel> {
    let is_web_page = content_type.starts_with("text/") && !content_type.contains("xml");
    if !status.is_success() {
        if is_web_page {
            anyhow::bail!(
                "the feed server returned HTTP {status} and a web page - {}",
                super::http::filter_block_message(host, url)
            );
        }
        anyhow::bail!("the feed server returned HTTP {status}");
    }
    match Channel::read_from(body) {
        Ok(channel) => Ok(channel),
        Err(_) if is_web_page => {
            anyhow::bail!(super::http::filter_block_message(host, url))
        }
        Err(error) => Err(error).context("the response is not a valid RSS feed"),
    }
}

/// Fetch channel-level metadata (title and author) from an RSS feed URL.
pub async fn fetch_feed_metadata(feed_url: &str) -> Result<FeedMetadata> {
    let channel = fetch_channel(feed_url).await?;
    let title = channel.title().to_string();
    let author = channel
        .itunes_ext()
        .and_then(|e| e.author())
        .or_else(|| channel.managing_editor())
        .unwrap_or("")
        .to_string();
    Ok(FeedMetadata { title, author })
}

/// Fetch the title and publish date of the most recent episode from the feed.
pub async fn get_last_podcast_name(podcast: &Podcast) -> Result<(String, Option<String>)> {
    let channel = fetch_channel(&podcast.feed_url).await?;
    let item = channel.items.first().context("RSS feed has no episodes")?;
    let episode_title = item
        .title
        .as_ref()
        .context("Episode has no title")?
        .to_string();
    let pub_date = item.pub_date.clone();
    Ok((episode_title, pub_date))
}

/// Fetch the full episode list from an RSS feed URL.
pub async fn fetch_episode_list(feed_url: &str) -> Result<Vec<EpisodeInfo>> {
    let channel = fetch_channel(feed_url).await?;
    let length = channel.items.len();

    let episodes: Vec<EpisodeInfo> = channel
        .items
        .iter()
        .enumerate()
        .map(|(rss_idx, item)| {
            let title = item
                .title
                .as_ref()
                .cloned()
                .unwrap_or_else(|| format!("Episode {}", length - rss_idx));
            let feed_index = length - rss_idx;
            let pub_date = item.pub_date.clone();
            EpisodeInfo {
                title,
                feed_index,
                pub_date,
            }
        })
        .collect();

    Ok(episodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FEED: &str = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>Show</title><link>https://example.com</link><description>d</description></channel></rss>"#;
    const PAGE: &str = "<html><body>Blocked by your administrator</body></html>";
    const URL: &str = "https://feeds.example.com/show";

    fn parse(status: u16, content_type: &str, body: &str) -> Result<Channel> {
        let status = StatusCode::from_u16(status).unwrap();
        parse_feed_response(URL, "filter.corp", status, content_type, body.as_bytes())
    }

    #[test]
    fn feed_served_as_plain_text_still_parses() {
        assert_eq!(parse(200, "text/plain", FEED).unwrap().title(), "Show");
    }

    #[test]
    fn rss_content_types_parse() {
        for content_type in ["application/rss+xml", "text/xml", ""] {
            assert_eq!(parse(200, content_type, FEED).unwrap().title(), "Show");
        }
    }

    #[test]
    fn web_page_instead_of_feed_is_reported_as_filter_block() {
        let message = parse(200, "text/html; charset=utf-8", PAGE)
            .unwrap_err()
            .to_string();
        assert!(message.contains("(internet-filter/proxy) blocked URL"));
        assert!(message.contains("filter.corp"));
        assert!(message.contains(URL));
    }

    #[test]
    fn error_status_is_shown_and_flags_a_filter_only_for_web_pages() {
        let page = parse(403, "text/html", PAGE).unwrap_err().to_string();
        assert!(page.contains("HTTP 403"));
        assert!(page.contains("internet-filter/proxy"));

        let plain = parse(404, "application/xml", "").unwrap_err().to_string();
        assert!(plain.contains("HTTP 404"));
        assert!(!plain.contains("internet-filter/proxy"));
    }

    #[test]
    fn invalid_non_page_body_is_not_blamed_on_a_filter() {
        let message = parse(200, "application/xml", "not a feed")
            .unwrap_err()
            .to_string();
        assert!(message.contains("not a valid RSS feed"));
        assert!(!message.contains("internet-filter/proxy"));
    }
}
