//! RSS feed fetching: latest episode title, full episode list, and channel metadata.

use anyhow::{Context, Result};
use rss::Channel;

use super::data::{EpisodeInfo, Podcast};

/// Channel-level metadata extracted from an RSS feed.
#[derive(Debug, Clone)]
pub struct FeedMetadata {
    pub title: String,
    pub author: String,
}

async fn fetch_channel(url: &str) -> Result<Channel> {
    let content = super::http::client().get(url).send().await?.bytes().await?;
    Ok(Channel::read_from(&content[..])?)
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

/// Reduce a feed's HTML description to searchable plain text: tags dropped,
/// common entities decoded, whitespace collapsed.
fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                text.push(' ');
            }
            '>' if in_tag => in_tag = false,
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
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
            let description = plain_text(
                item.description()
                    .filter(|text| !text.trim().is_empty())
                    .or_else(|| item.itunes_ext().and_then(|ext| ext.summary()))
                    .or_else(|| item.content())
                    .unwrap_or(""),
            );
            EpisodeInfo {
                title,
                feed_index,
                pub_date,
                description,
            }
        })
        .collect();

    Ok(episodes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_drops_tags_and_decodes_entities() {
        assert_eq!(
            plain_text(
                "<p>Fish &amp; chips</p><p>with&nbsp;<b>vinegar</b></p>
"
            ),
            "Fish & chips with vinegar"
        );
        assert_eq!(plain_text(""), "");
    }
}
