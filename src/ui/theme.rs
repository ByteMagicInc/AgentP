//! Dracula-inspired color palette and ASCII art banner.

use ratatui::style::Color;

pub(super) const ACCENT: Color = Color::Rgb(80, 250, 123);
pub(super) const ACCENT_DIM: Color = Color::Rgb(60, 180, 90);
pub(super) const HIGHLIGHT: Color = Color::Rgb(241, 250, 140);
pub(super) const TITLE_FG: Color = Color::Rgb(189, 147, 249);
pub(super) const TEXT: Color = Color::Rgb(248, 248, 242);
pub(super) const TEXT_DIM: Color = Color::Rgb(98, 114, 164);
pub(super) const BORDER: Color = Color::Rgb(68, 71, 90);
pub(super) const SELECTED_BG: Color = Color::Rgb(68, 71, 90);
pub(super) const GAUGE_FG: Color = Color::Rgb(139, 233, 253);
pub(super) const CHECK_ON: Color = Color::Rgb(80, 250, 123);
pub(super) const ERROR_FG: Color = Color::Rgb(255, 85, 85);
pub(super) const BG: Color = Color::Rgb(40, 42, 54);
pub(super) const PINK: Color = Color::Rgb(255, 121, 198);
pub(super) const ORANGE: Color = Color::Rgb(255, 184, 108);

pub const ASCII_ART_MIC: [&str; 7] = [
    " ▄██████▄  ",
    " █ ▀▀▀▀ █  ",
    " █ ▄▄▄▄ █  ",
    " ▀██▄▄██▀  ",
    "   ████    ",
    " ▀██████▀  ",
    "            ",
];

pub const ASCII_ART_TEXT: [&str; 7] = [
    "   _                    _   ____  ",
    "  / \\   __ _  ___ _ __ | |_|  _ \\ ",
    " / _ \\ / _` |/ _ \\ '_ \\| __| |_) |",
    "/ ___ \\ (_| |  __/ | | | |_|  __/ ",
    "/_/   \\_\\__, |\\___|_| |_|\\__|_|    ",
    "        |___/                      ",
    "                                   ",
];
