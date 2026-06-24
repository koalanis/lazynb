//! Application state and the actions the key handler drives.

use crate::config::Config;
use crate::graph::{EdgeKind, Graph};
use crate::nb::{self, Note};
use crate::overlay::{Action, Overlay, PickItem, Picker, Search, Shell};
use anyhow::Result;
use std::collections::HashSet;

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
    /// Active tag filter for the notes panel, if any.
    pub tag_filter: Option<String>,
    /// The floating overlay (shell, picker, ...), when one is open.
    pub overlay: Option<Box<dyn Overlay>>,
    pub config: Config,
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
            tag_filter: None,
            overlay: None,
            config: Config::default(),
        };
        app.reload_notes();
        Ok(app)
    }

    pub fn current_note(&self) -> Option<&Note> {
        self.notes.get(self.note_idx)
    }

    fn current_notebook(&self) -> Option<String> {
        self.notebooks.get(self.notebook_idx).cloned()
    }

    /// Reload the notes list for the selected notebook, applying the tag filter,
    /// and refresh the preview.
    pub fn reload_notes(&mut self) {
        let Some(name) = self.current_notebook() else {
            return;
        };
        let mut notes = match nb::notes(&name) {
            Ok(notes) => notes,
            Err(e) => {
                self.status = format!("error: {e}");
                return;
            }
        };
        if let Some(tag) = &self.tag_filter {
            let allowed: HashSet<String> = nb::note_paths_with_tag(&name, tag).into_iter().collect();
            notes.retain(|n| allowed.contains(&n.path));
        }
        self.notes = notes;
        self.note_idx = 0;
        let filter = self
            .tag_filter
            .as_ref()
            .map(|t| format!(" · #{t}"))
            .unwrap_or_default();
        self.status = format!("{} · {} notes{}", name, self.notes.len(), filter);
        self.load_preview();
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
                let selected = self.current_notebook();
                self.notebooks = notebooks;
                self.notebook_idx = selected
                    .and_then(|name| self.notebooks.iter().position(|n| *n == name))
                    .unwrap_or(0)
                    .min(self.notebooks.len() - 1);
            }
        }
        self.reload_notes();
    }

    // --- Overlays -----------------------------------------------------------

    /// Apply an action emitted by the active overlay.
    pub fn apply(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Close => self.overlay = None,
            Action::Refresh => self.refresh(),
            Action::SelectNote(path) => {
                self.overlay = None;
                self.select_note_by_path(&path);
            }
            Action::SetTagFilter(filter) => {
                self.overlay = None;
                self.tag_filter = filter;
                self.reload_notes();
            }
        }
    }

    /// Move the panel selection to the note at `path`, switching notebook and
    /// clearing the tag filter if needed so the target is reachable.
    fn select_note_by_path(&mut self, path: &str) {
        if let Some(name) = notebook_of_path(path) {
            if self.current_notebook().as_deref() != Some(name.as_str()) {
                if let Some(idx) = self.notebooks.iter().position(|n| *n == name) {
                    self.notebook_idx = idx;
                }
            }
        }
        self.tag_filter = None;
        self.reload_notes();
        if let Some(idx) = self.notes.iter().position(|n| n.path == path) {
            self.note_idx = idx;
            self.load_preview();
        }
    }

    pub fn open_shell(&mut self) {
        self.overlay = Some(Box::new(Shell::new()));
    }

    /// Picker of every tag in the current notebook; selecting one filters the
    /// notes panel (and a sentinel row clears the filter).
    pub fn open_tag_list(&mut self) {
        let Some(name) = self.current_notebook() else {
            return;
        };
        let mut items = vec![PickItem::new("[ all notes ]", Action::SetTagFilter(None))];
        for tag in nb::tags(&name) {
            items.push(PickItem::new(
                format!("#{tag}"),
                Action::SetTagFilter(Some(tag)),
            ));
        }
        self.overlay = Some(Box::new(
            Picker::new(format!("Tags in {name}"), items).empty_note("No tags in this notebook."),
        ));
    }

    /// Picker of notes that link to the current note via `[[Title]]`.
    pub fn open_backlinks(&mut self) {
        let (Some(note), Some(name)) = (self.current_note(), self.current_notebook()) else {
            return;
        };
        let title = note.title.clone();
        let self_path = note.path.clone();

        let paths = nb::search_paths(&name, &format!("[[{title}]]"));
        let all = nb::notes(&name).unwrap_or_default();
        let items: Vec<PickItem> = paths
            .into_iter()
            .filter(|p| *p != self_path)
            .filter_map(|p| all.iter().find(|n| n.path == p))
            .map(|n| {
                PickItem::new(
                    format!("[{}] {}", n.id, n.title),
                    Action::SelectNote(n.path.clone()),
                )
            })
            .collect();
        self.overlay = Some(Box::new(
            Picker::new(format!("Backlinks: {title}"), items)
                .empty_note("No notes link here.")
                .sized(70, 70),
        ));
    }

    /// Picker of the `[[wiki links]]` found in the current note; selecting one
    /// jumps to that note.
    pub fn open_links(&mut self) {
        let items: Vec<PickItem> = extract_wiki_links(&self.preview)
            .into_iter()
            .map(|target| match self
                .notes
                .iter()
                .find(|n| n.title.eq_ignore_ascii_case(&target) || n.id == target)
            {
                Some(n) => PickItem::new(
                    format!("→ [{}] {}", n.id, n.title),
                    Action::SelectNote(n.path.clone()),
                ),
                None => PickItem::new(format!("→ {target}  (unresolved)"), Action::None),
            })
            .collect();
        self.overlay = Some(Box::new(
            Picker::new("Links in this note", items)
                .empty_note("No [[links]] in this note.")
                .sized(70, 70),
        ));
    }

    /// Open a ripgrep live-search over the selected notebook.
    pub fn open_search(&mut self) {
        let Some(name) = self.current_notebook() else {
            return;
        };
        let notes = nb::notes(&name).unwrap_or_default();
        self.overlay = Some(Box::new(Search::new(name, &notes)));
    }

    /// Build the relationship graph for the selected notebook (nodes = notes,
    /// edges = `[[links]]` and shared `#tags`) and open it.
    pub fn open_graph(&mut self) {
        let Some(name) = self.current_notebook() else {
            return;
        };
        let notes = nb::notes(&name).unwrap_or_default();

        let mut raw = Vec::with_capacity(notes.len());
        let mut links_per: Vec<Vec<String>> = Vec::with_capacity(notes.len());
        let mut tags_per: Vec<HashSet<String>> = Vec::with_capacity(notes.len());
        for note in &notes {
            let content = std::fs::read_to_string(&note.path).unwrap_or_default();
            let lines: Vec<String> = content.lines().map(String::from).collect();
            raw.push((note.id.clone(), note.title.clone(), note.path.clone()));
            links_per.push(extract_wiki_links(&lines));
            tags_per.push(extract_tags(&lines).into_iter().collect());
        }

        let resolve = |target: &str| {
            notes
                .iter()
                .position(|n| n.title.eq_ignore_ascii_case(target) || n.id == target)
        };

        let mut linked: HashSet<(usize, usize)> = HashSet::new();
        let mut edges = Vec::new();
        for (i, links) in links_per.iter().enumerate() {
            for target in links {
                if let Some(j) = resolve(target) {
                    if i != j && linked.insert((i.min(j), i.max(j))) {
                        edges.push((i, j, EdgeKind::Link));
                    }
                }
            }
        }
        // Tag edges between notes sharing a tag, skipping already-linked pairs.
        for i in 0..notes.len() {
            for j in (i + 1)..notes.len() {
                if !tags_per[i].is_disjoint(&tags_per[j]) && !linked.contains(&(i, j)) {
                    edges.push((i, j, EdgeKind::Tag));
                }
            }
        }

        self.overlay = Some(Box::new(Graph::new(raw, edges)));
    }
}

/// The notebook a note path belongs to: the name of its parent directory.
fn notebook_of_path(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .parent()?
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
}

/// Extract `#tags` from note lines (lowercased, de-duplicated). A `#` only
/// starts a tag at a word boundary and when followed by a tag character, so
/// markdown headings (`# Title`) are not mistaken for tags.
fn extract_tags(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            if chars[i] != '#' {
                i += 1;
                continue;
            }
            let boundary = i == 0
                || chars[i - 1].is_whitespace()
                || matches!(chars[i - 1], '(' | ',' | '[' | '\'' | '"');
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && (chars[j].is_alphanumeric() || matches!(chars[j], '-' | '_')) {
                j += 1;
            }
            if boundary && j > start {
                let tag: String = chars[start..j].iter().collect::<String>().to_lowercase();
                if !out.contains(&tag) {
                    out.push(tag);
                }
            }
            i = j.max(i + 1);
        }
    }
    out
}

/// Extract `[[wiki link]]` targets from note lines, in order, de-duplicated.
/// For aliased links `[[target|text]]`, the part before `|` is the target.
fn extract_wiki_links(lines: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in lines {
        let mut rest = line.as_str();
        while let Some(start) = rest.find("[[") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("]]") else {
                break;
            };
            let target = after[..end]
                .split('|')
                .next()
                .unwrap_or(&after[..end])
                .trim()
                .to_string();
            if !target.is_empty() && !out.contains(&target) {
                out.push(target);
            }
            rest = &after[end + 2..];
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::extract_wiki_links;

    fn lines(s: &[&str]) -> Vec<String> {
        s.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn extracts_links_in_order_deduped() {
        let input = lines(&[
            "See [[Architecture]] and [[Roadmap]].",
            "Again [[Architecture]] plus [[Home|the index]].",
        ]);
        assert_eq!(
            extract_wiki_links(&input),
            ["Architecture", "Roadmap", "Home"]
        );
    }

    #[test]
    fn ignores_unterminated_brackets() {
        assert_eq!(extract_wiki_links(&lines(&["a [[ b"])), Vec::<String>::new());
    }
}
