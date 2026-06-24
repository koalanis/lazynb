//! Runtime configuration. Constructed once in `main` and threaded into the UI
//! and overlays, so colors live in one place and are trivial to change in code
//! (or, later, to load from a file).

use ratatui::style::Color;

#[derive(Clone)]
pub struct Config {
    /// Border/prompt color for overlays and the focused-panel accent.
    pub accent: Color,
    /// Border color of the focused panel.
    pub focus: Color,
    /// Selected-row color in lists.
    pub highlight: Color,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            accent: Color::Magenta,
            focus: Color::Green,
            highlight: Color::Yellow,
        }
    }
}
