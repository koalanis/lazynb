//! Application state and the actions the key handler drives.

use crate::nb::{self, Note};
use anyhow::Result;

/// Which panel currently has keyboard focus.
#[derive(PartialEq, Clone, Copy)]
pub enum Panel {
    Notebooks,
    Notes,
}

/// State for the `nb` shell overlay: a small REPL that runs nb commands and
/// shows their output, keeping a command history.
pub struct ShellModal {
    /// The line currently being typed.
    pub input: String,
    /// Scrollback: prompts entered and the output they produced.
    pub output: Vec<String>,
    /// Previously-entered commands, for up/down recall.
    pub history: Vec<String>,
    /// Cursor into `history` while recalling; `None` means "at the live input".
    history_pos: Option<usize>,
}

impl ShellModal {
    fn new() -> Self {
        ShellModal {
            input: String::new(),
            output: vec![
                "nb shell -- type a command (the leading `nb` is optional).".into(),
                "Enter runs it, up/down recalls history, Esc closes.".into(),
                String::new(),
            ],
            history: Vec::new(),
            history_pos: None,
        }
    }
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
    /// The nb shell overlay, when open.
    pub shell: Option<ShellModal>,
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
            shell: None,
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

    /// Re-query notebooks and notes, preserving the selected notebook if it
    /// still exists. Called after shell commands, which may have changed data.
    pub fn refresh(&mut self) {
        if let Ok(notebooks) = nb::notebooks() {
            if !notebooks.is_empty() {
                let selected = self.notebooks.get(self.notebook_idx).cloned();
                self.notebooks = notebooks;
                self.notebook_idx = selected
                    .and_then(|name| self.notebooks.iter().position(|n| *n == name))
                    .unwrap_or(0)
                    .min(self.notebooks.len() - 1);
            }
        }
        self.reload_notes();
    }

    pub fn open_shell(&mut self) {
        self.shell = Some(ShellModal::new());
    }

    pub fn close_shell(&mut self) {
        self.shell = None;
        self.refresh();
    }

    pub fn shell_input(&mut self, c: char) {
        if let Some(modal) = self.shell.as_mut() {
            modal.input.push(c);
            modal.history_pos = None;
        }
    }

    pub fn shell_backspace(&mut self) {
        if let Some(modal) = self.shell.as_mut() {
            modal.input.pop();
            modal.history_pos = None;
        }
    }

    /// Run the typed command, append its output to the scrollback, and refresh
    /// the panels behind the modal. `exit`/`quit` closes the shell.
    pub fn shell_submit(&mut self) {
        let line = match self.shell.as_ref() {
            Some(modal) => modal.input.trim().to_string(),
            None => return,
        };
        if line.is_empty() {
            return;
        }
        if matches!(line.as_str(), "exit" | "quit") {
            self.close_shell();
            return;
        }

        let output = nb::run_command(&line);
        if let Some(modal) = self.shell.as_mut() {
            modal.output.push(format!("nb> {line}"));
            if output.is_empty() {
                modal.output.push("(no output)".into());
            } else {
                modal.output.extend(output);
            }
            modal.output.push(String::new());
            modal.history.push(line);
            modal.history_pos = None;
            modal.input.clear();
        }
        self.refresh();
    }

    pub fn shell_history_prev(&mut self) {
        if let Some(modal) = self.shell.as_mut() {
            if modal.history.is_empty() {
                return;
            }
            let pos = match modal.history_pos {
                None => modal.history.len() - 1,
                Some(p) => p.saturating_sub(1),
            };
            modal.history_pos = Some(pos);
            modal.input = modal.history[pos].clone();
        }
    }

    pub fn shell_history_next(&mut self) {
        if let Some(modal) = self.shell.as_mut() {
            match modal.history_pos {
                Some(p) if p + 1 < modal.history.len() => {
                    modal.history_pos = Some(p + 1);
                    modal.input = modal.history[p + 1].clone();
                }
                _ => {
                    modal.history_pos = None;
                    modal.input.clear();
                }
            }
        }
    }
}
