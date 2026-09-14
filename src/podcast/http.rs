//! Shared `reqwest` client carrying a descriptive `User-Agent`.
//!
//! Some podcast hosts (e.g. Buzzsprout's `audio.buzzsprout.com` CDN) reject
//! requests with no `User-Agent` header, returning a `403` HTML page instead of
//! the audio file. `reqwest::get` sends no `User-Agent` by default, so all HTTP
//! access goes through this client to avoid being mistaken for a blocked bot.
//!
//! The client carries the built-in `AgentP/<version>` default; a podcast can
//! override it per audio download via its `user_agent` field (applied as a
//! per-request header in `download`).

use std::sync::LazyLock;

use reqwest::Client;

const DEFAULT_USER_AGENT: &str = concat!(
    "AgentP/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/ByteMagicInc/AgentP)"
);

static CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .build()
        .expect("failed to build reqwest client")
});

/// Shared `reqwest` client with the built-in `User-Agent` set as its default.
pub fn client() -> &'static Client {
    &CLIENT
}
