//! Application state and the actions the key handler drives.

use crate::nb::{self, Note};
use anyhow::Result;

/// Which panel currently has keyboard focus.
#[derive(PartialEq, Clone, Copy)]
pub enum Panel {
    Notebooks,
    Notes,
}

pub struct App {
    pub notebooks: Vec<String>,
    pub notes: Vec<Note>,
    pub notebook_idx: usize,
    pub note_idx: usize,
    pub focus: Panel,
    /// Lines of the selected note, for the preview pane.
    pub preview: Vec<String>,
    pub should_quit: bool,
    /// Set when the user opens a note: the path to hand to the editor once
    /// the TUI has torn down. Drives the `$NVIM`/`$EDITOR` handoff in `main`.
    pub open_request: Option<String>,
    pub status: String,
}

impl App {
    pub fn new() -> Result<Self> {
        let notebooks = nb::notebooks()?;
        let current = nb::current_notebook().unwrap_or_default();
        let notebook_idx = notebooks.iter().position(|n| *n == current).unwrap_or(0);

        let mut app = App {
            notebooks,
            notes: Vec::new(),
            notebook_idx,
            note_idx: 0,
            focus: Panel::Notes,
            preview: Vec::new(),
            should_quit: false,
            open_request: None,
            status: String::new(),
        };
        app.reload_notes();
        Ok(app)
    }

    pub fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.note_idx)
    }

    /// Reload the notes list for the selected notebook and refresh the preview.
    pub fn reload_notes(&mut self) {
        let Some(name) = self.notebooks.get(self.notebook_idx).cloned() else {
            return;
        };
        match nb::notes(&name) {
            Ok(notes) => {
                self.notes = notes;
                self.note_idx = 0;
                self.status = format!("{} · {} notes", name, self.notes.len());
                self.load_preview();
            }
            Err(e) => self.status = format!("error: {e}"),
        }
    }

    fn load_preview(&mut self) {
        self.preview = match self.current_note() {
            Some(note) => std::fs::read_to_string(&note.path)
                .unwrap_or_else(|e| format!("<could not read note: {e}>"))
                .lines()
                .map(String::from)
                .collect(),
            None => Vec::new(),
        };
    }

    pub fn next(&mut self) {
        match self.focus {
            Panel::Notebooks => {
                if !self.notebooks.is_empty() {
                    self.notebook_idx = (self.notebook_idx + 1) % self.notebooks.len();
                    self.reload_notes();
                }
            }
            Panel::Notes => {
                if !self.notes.is_empty() {
                    self.note_idx = (self.note_idx + 1) % self.notes.len();
                    self.load_preview();
                }
            }
        }
    }

    pub fn prev(&mut self) {
        match self.focus {
            Panel::Notebooks => {
                let len = self.notebooks.len();
                if len != 0 {
                    self.notebook_idx = (self.notebook_idx + len - 1) % len;
                    self.reload_notes();
                }
            }
            Panel::Notes => {
                let len = self.notes.len();
                if len != 0 {
                    self.note_idx = (self.note_idx + len - 1) % len;
                    self.load_preview();
                }
            }
        }
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Panel::Notebooks => Panel::Notes,
            Panel::Notes => Panel::Notebooks,
        };
    }

    /// Request that the selected note be opened in an editor and exit the TUI.
    pub fn open_selected(&mut self) {
        if let Some(note) = self.current_note() {
            self.open_request = Some(note.path.clone());
            self.should_quit = true;
        }
    }
}
