use std::env::consts::{ARCH, OS};

use super::state::{App, Screen};

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
pub const REPOSITORY_URL: &str = "https://github.com/ByteMagicInc/AgentP";
pub const GIT_SHA: &str = match option_env!("AGENTP_GIT_SHA") {
    Some(sha) => sha,
    None => "unknown",
};
pub const ISSUES_URL: &str = "https://github.com/ByteMagicInc/AgentP/issues/new";

/// Short human-readable summary of the running build.
pub fn build_summary() -> String {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    format!("{GIT_SHA} | {ARCH}-{OS} | {profile}")
}

/// One-line version string the About screen shows and copies.
pub fn version_info() -> String {
    format!("AgentP {APP_VERSION} ({})", build_summary())
}

fn new_issue_url() -> String {
    let body = format!(
        "## Describe the issue\n\n\n## Build information\n\n{}",
        version_info()
    );
    format!("{ISSUES_URL}?body={}", urlencoding::encode(&body))
}

impl App {
    /// Open the About screen, remembering where to return to.
    pub fn enter_about(&mut self) {
        if self.screen == Screen::About {
            return;
        }
        self.podcast_delete_pending = None;
        self.about_back_to = self.screen;
        self.about_notice = None;
        self.screen = Screen::About;
    }

    /// Leave the About screen for the screen it opened from.
    pub fn about_back(&mut self) {
        self.screen = self.about_back_to;
        self.about_notice = None;
    }

    /// Open the repository in the browser, where it can be starred.
    pub fn open_repository(&self) {
        let _ = open::that(REPOSITORY_URL);
    }

    /// Open the new-issue page for the repository.
    pub fn open_issue(&self) {
        let _ = open::that(new_issue_url());
    }

    /// Copy the version string to the clipboard and confirm it on screen.
    pub fn copy_version_info(&mut self) {
        let info = version_info();
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(info)) {
            Ok(()) => self.about_notice = Some("Copied version info to clipboard".to_string()),
            Err(e) => self.about_notice = Some(format!("Clipboard unavailable: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_app_one_podcast as test_app;
    use super::*;

    #[test]
    fn version_info_names_the_running_build() {
        let info = version_info();
        assert!(info.contains("AgentP"));
        assert!(info.contains(APP_VERSION));
        assert!(info.contains(GIT_SHA));
    }

    #[test]
    fn new_issue_url_prefills_build_information() {
        let url = new_issue_url();
        let (base, encoded_body) = url.split_once("?body=").unwrap();
        assert_eq!(base, ISSUES_URL);
        assert!(!encoded_body.contains(' '));
        let body = urlencoding::decode(encoded_body).unwrap();
        assert!(body.contains("## Describe the issue"));
        assert!(body.contains("## Build information"));
        assert!(body.contains(&version_info()));
    }

    #[test]
    fn about_returns_to_the_screen_it_opened_from() {
        let mut app = test_app();
        app.enter_config();
        app.enter_about();
        assert_eq!(app.screen, Screen::About);
        app.about_back();
        assert_eq!(app.screen, Screen::Config);
    }
}
