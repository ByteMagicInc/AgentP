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

const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "blockquote",
    "br",
    "dd",
    "div",
    "dl",
    "dt",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "td",
    "th",
    "tr",
    "ul",
];

/// Whether the tag body between `<` and `>` names a block-level element,
/// which separates words, as opposed to inline markup such as `<b>`.
fn is_block_tag(tag: &str) -> bool {
    let name: String = tag
        .trim_start_matches('/')
        .chars()
        .take_while(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    BLOCK_TAGS.contains(&name.as_str())
}

/// Decode the character reference at the start of `text` (which begins with
/// `&`): numeric references and the named entities feeds commonly use.
/// Returns the character and the reference's length in bytes.
fn decode_entity(text: &str) -> Option<(char, usize)> {
    let end = text
        .char_indices()
        .take(12)
        .find(|&(_, c)| c == ';')
        .map(|(i, _)| i)?;
    let name = &text[1..end];
    let decoded = if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        char::from_u32(code)?
    } else {
        match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            "nbsp" => ' ',
            "ndash" => '–',
            "mdash" => '—',
            "lsquo" => '‘',
            "rsquo" => '’',
            "ldquo" => '“',
            "rdquo" => '”',
            "hellip" => '…',
            "bull" => '•',
            "middot" => '·',
            "copy" => '©',
            "reg" => '®',
            "trade" => '™',
            "deg" => '°',
            "eacute" => 'é',
            "egrave" => 'è',
            "aacute" => 'á',
            "iacute" => 'í',
            "oacute" => 'ó',
            "uacute" => 'ú',
            "ntilde" => 'ñ',
            "uuml" => 'ü',
            "ouml" => 'ö',
            "auml" => 'ä',
            "ccedil" => 'ç',
            _ => return None,
        }
    };
    Some((decoded, end + 1))
}

/// Reduce a feed's HTML description to searchable plain text: tags dropped,
/// with block-level tags separating words and inline ones not, character
/// references decoded, whitespace collapsed. A `<` that does not open a tag,
/// as in `<3`, is kept as text.
fn plain_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(c) = rest.chars().next() {
        if c == '<'
            && rest[1..]
                .starts_with(|n: char| n.is_ascii_alphabetic() || matches!(n, '/' | '!' | '?'))
            && let Some(end) = rest.find('>')
        {
            if is_block_tag(&rest[1..end]) {
                text.push(' ');
            }
            rest = &rest[end + 1..];
            continue;
        }
        if c == '&'
            && let Some((decoded, len)) = decode_entity(rest)
        {
            text.push(decoded);
            rest = &rest[len..];
            continue;
        }
        text.push(c);
        rest = &rest[c.len_utf8()..];
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first of an item's description, iTunes summary, and content that still
/// has text once its HTML is stripped.
fn episode_description(item: &rss::Item) -> String {
    [
        item.description(),
        item.itunes_ext().and_then(|ext| ext.summary()),
        item.content(),
    ]
    .into_iter()
    .flatten()
    .map(plain_text)
    .find(|text| !text.is_empty())
    .unwrap_or_default()
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
            let description = episode_description(item);
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

    #[test]
    fn inline_tags_keep_words_whole_and_block_tags_separate_them() {
        assert_eq!(plain_text("<b>Fish</b>ing<br/>trip"), "Fishing trip");
        assert_eq!(plain_text("<li>one</li><li>two</li>"), "one two");
    }

    #[test]
    fn a_tag_only_description_falls_back_to_the_content() {
        let item = rss::Item {
            description: Some("<p></p>".to_string()),
            content: Some("<p>The real notes</p>".to_string()),
            ..Default::default()
        };
        assert_eq!(episode_description(&item), "The real notes");
    }

    #[test]
    fn a_bare_angle_bracket_is_text() {
        assert_eq!(plain_text("I <3 this > that"), "I <3 this > that");
        assert_eq!(plain_text("a < b"), "a < b");
    }

    #[test]
    fn numeric_and_named_references_decode() {
        assert_eq!(
            plain_text("It&#8217;s &ldquo;here&rdquo; &#x2014; caf&eacute; &amp;lt;"),
            "It’s “here” — café &lt;"
        );
        assert_eq!(
            plain_text("&unknown; &#xZZ; & alone"),
            "&unknown; &#xZZ; & alone"
        );
    }
}
