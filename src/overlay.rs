//! Overlays: modal widgets that float over the browse view.
//!
//! The design goal is that adding a feature is cheap and uniform. Every
//! overlay implements [`Overlay`] (handle a key, draw itself), and instead of
//! mutating the app directly it returns an [`Action`] describing what should
//! happen. `App::apply` is the single place that interprets actions.
//!
//! Most overlays are just lists of choices, so [`Picker`] is a reusable,
//! data-configured widget: build it from `(label, action)` pairs and it gives
//! you navigation, incremental filtering and selection for free. The three
//! note features (tag filter, backlinks, link-jump) are each a few lines that
//! assemble a `Picker`. [`Shell`] shows the trait also fits a non-list widget.
//!
//! To add a new overlay: add an `Action` variant if you need a new effect,
//! handle it in `App::apply`, then either build a `Picker` or implement
//! `Overlay` for your own type and open it from an `App::open_*` method.

use crate::config::Config;
use crate::nb::{self, Note};
use std::collections::HashMap;
use std::process::Command;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

/// What an overlay asks the app to do after handling a key. The app applies it
/// in `App::apply`; several variants implicitly close the overlay.
#[derive(Clone)]
pub enum Action {
    /// Do nothing; keep the overlay open.
    None,
    /// Close the overlay.
    Close,
    /// Re-query notebooks and notes (e.g. after a shell command).
    Refresh,
    /// Select the note at this path in the panel, switching notebook if needed.
    SelectNote(String),
    /// Set (`Some`) or clear (`None`) the notes-panel tag filter.
    SetTagFilter(Option<String>),
}

/// A floating widget. Owns its own state; communicates outward via [`Action`].
pub trait Overlay {
    /// Handle a key press, returning an action for the app to apply.
    fn on_key(&mut self, key: KeyEvent) -> Action;
    /// Draw into `area` (already cleared by the caller).
    fn render(&self, f: &mut Frame, area: Rect, cfg: &Config);
    /// Preferred popup size as (width%, height%) of the screen.
    fn size(&self) -> (u16, u16) {
        (70, 60)
    }
    /// Advance any animation by one frame; called once per UI tick.
    fn tick(&mut self) {}
    /// Whether the overlay wants frequent redraws (drives the poll interval).
    fn animating(&self) -> bool {
        false
    }
}

/// One row in a [`Picker`]: a label and the action selecting it emits.
pub struct PickItem {
    pub label: String,
    pub action: Action,
}

impl PickItem {
    pub fn new(label: impl Into<String>, action: Action) -> Self {
        PickItem {
            label: label.into(),
            action,
        }
    }
}

/// A reusable list-picker with incremental filtering. Configure it purely with
/// data: a title and a list of `(label, action)` rows.
pub struct Picker {
    title: String,
    items: Vec<PickItem>,
    /// Indices into `items` that match the current query.
    filtered: Vec<usize>,
    /// Position within `filtered`.
    selected: usize,
    query: String,
    empty: String,
    size: (u16, u16),
}

impl Picker {
    pub fn new(title: impl Into<String>, items: Vec<PickItem>) -> Self {
        let filtered = (0..items.len()).collect();
        Picker {
            title: title.into(),
            items,
            filtered,
            selected: 0,
            query: String::new(),
            empty: "Nothing here.".into(),
            size: (60, 70),
        }
    }

    /// Message shown when there are no rows. Builder-style.
    pub fn empty_note(mut self, note: impl Into<String>) -> Self {
        self.empty = note.into();
        self
    }

    /// Override the popup size, as (width%, height%). Builder-style.
    pub fn sized(mut self, width: u16, height: u16) -> Self {
        self.size = (width, height);
        self
    }

    fn refilter(&mut self) {
        let q = self.query.to_lowercase();
        self.filtered = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, it)| q.is_empty() || it.label.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect();
        self.selected = self.selected.min(self.filtered.len().saturating_sub(1));
    }

    fn selected_action(&self) -> Action {
        self.filtered
            .get(self.selected)
            .and_then(|&i| self.items.get(i))
            .map(|it| it.action.clone())
            .unwrap_or(Action::None)
    }
}

impl Overlay for Picker {
    fn size(&self) -> (u16, u16) {
        self.size
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => self.selected_action(),
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.filtered.len() {
                    self.selected += 1;
                }
                Action::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.refilter();
                Action::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.refilter();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, cfg: &Config) {
        let title = if self.query.is_empty() {
            format!(" {} ", self.title)
        } else {
            format!(" {} : {} ", self.title, self.query)
        };
        let block = Block::default()
            .title(title)
            .title_bottom(" ↑↓ select · type to filter · Enter open · Esc close ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(cfg.accent));
        let inner = block.inner(area);
        f.render_widget(block, area);

        if self.filtered.is_empty() {
            let msg = Paragraph::new(self.empty.as_str()).style(Style::default().fg(Color::DarkGray));
            f.render_widget(msg, inner);
            return;
        }

        let rows: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&i| ListItem::new(self.items[i].label.as_str()))
            .collect();
        let list = List::new(rows)
            .highlight_style(Style::default().fg(cfg.highlight).add_modifier(Modifier::BOLD))
            .highlight_symbol("▌ ");
        let mut state = ListState::default();
        state.select(Some(self.selected));
        f.render_stateful_widget(list, inner, &mut state);
    }
}

/// The nb shell: a small REPL that runs nb commands and shows their output.
pub struct Shell {
    input: String,
    output: Vec<String>,
    history: Vec<String>,
    history_pos: Option<usize>,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
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

    fn submit(&mut self) -> Action {
        let line = self.input.trim().to_string();
        if line.is_empty() {
            return Action::None;
        }
        if matches!(line.as_str(), "exit" | "quit") {
            return Action::Close;
        }

        let output = nb::run_command(&line);
        self.output.push(format!("nb> {line}"));
        if output.is_empty() {
            self.output.push("(no output)".into());
        } else {
            self.output.extend(output);
        }
        self.output.push(String::new());
        self.history.push(line);
        self.history_pos = None;
        self.input.clear();
        // Commands may have changed the data; let the app reload the panels.
        Action::Refresh
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let pos = match self.history_pos {
            None => self.history.len() - 1,
            Some(p) => p.saturating_sub(1),
        };
        self.history_pos = Some(pos);
        self.input = self.history[pos].clone();
    }

    fn history_next(&mut self) {
        match self.history_pos {
            Some(p) if p + 1 < self.history.len() => {
                self.history_pos = Some(p + 1);
                self.input = self.history[p + 1].clone();
            }
            _ => {
                self.history_pos = None;
                self.input.clear();
            }
        }
    }
}

impl Overlay for Shell {
    fn on_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => self.submit(),
            KeyCode::Up => {
                self.history_prev();
                Action::None
            }
            KeyCode::Down => {
                self.history_next();
                Action::None
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.history_pos = None;
                Action::None
            }
            KeyCode::Char(c) => {
                self.input.push(c);
                self.history_pos = None;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, cfg: &Config) {
        let block = Block::default()
            .title(" nb shell ")
            .title_bottom(" Esc close · ↑↓ history ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(cfg.accent));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(inner);

        let height = parts[0].height as usize;
        let start = self.output.len().saturating_sub(height);
        let body: Vec<Line> = self.output[start..]
            .iter()
            .map(|l| Line::raw(l.as_str()))
            .collect();
        f.render_widget(Paragraph::new(body), parts[0]);

        let prompt = Line::from(vec![
            Span::styled(
                "nb> ",
                Style::default().fg(cfg.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.input.as_str()),
            Span::styled("█", Style::default().fg(cfg.accent)),
        ]);
        f.render_widget(Paragraph::new(prompt), parts[1]);
    }
}

/// One ripgrep match: a note path, the line number, and the matched text.
struct Hit {
    path: String,
    line: u32,
    text: String,
}

/// Telescope-style live grep over a notebook's notes, powered by ripgrep.
/// Each keystroke re-runs `rg` over the note files; Enter jumps to the match.
pub struct Search {
    notebook: String,
    files: Vec<String>,
    /// path -> display label (`[id] Title`) for results.
    labels: HashMap<String, String>,
    query: String,
    results: Vec<Hit>,
    selected: usize,
    note: String,
}

impl Search {
    pub fn new(notebook: impl Into<String>, notes: &[Note]) -> Self {
        let files = notes.iter().map(|n| n.path.clone()).collect();
        let labels = notes
            .iter()
            .map(|n| (n.path.clone(), format!("[{}] {}", n.id, n.title)))
            .collect();
        Search {
            notebook: notebook.into(),
            files,
            labels,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
            note: "type to grep this notebook (ripgrep)".into(),
        }
    }

    fn run(&mut self) {
        self.results.clear();
        self.selected = 0;
        let query = self.query.trim();
        if query.is_empty() || self.files.is_empty() {
            self.note = "type to grep this notebook (ripgrep)".into();
            return;
        }

        let mut cmd = Command::new("rg");
        cmd.args([
            "--line-number",
            "--no-heading",
            "--color",
            "never",
            "--smart-case",
            "--max-count",
            "20",
            "-e",
            query,
            "--",
        ]);
        cmd.args(&self.files);

        match cmd.output() {
            Ok(out) => {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    if let Some(hit) = parse_rg(line) {
                        self.results.push(hit);
                    }
                    if self.results.len() >= 200 {
                        break;
                    }
                }
                self.note = match self.results.len() {
                    0 => "no matches".into(),
                    n => format!("{n} matches"),
                };
            }
            Err(_) => self.note = "ripgrep (rg) not found on PATH".into(),
        }
    }
}

/// Parse a `path:line:text` ripgrep line.
fn parse_rg(line: &str) -> Option<Hit> {
    let mut parts = line.splitn(3, ':');
    let path = parts.next()?.to_string();
    let line = parts.next()?.parse().ok()?;
    let text = parts.next()?.trim().to_string();
    Some(Hit { path, line, text })
}

impl Overlay for Search {
    fn size(&self) -> (u16, u16) {
        (80, 70)
    }

    fn on_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => Action::Close,
            KeyCode::Enter => self
                .results
                .get(self.selected)
                .map(|h| Action::SelectNote(h.path.clone()))
                .unwrap_or(Action::None),
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                Action::None
            }
            KeyCode::Down => {
                if self.selected + 1 < self.results.len() {
                    self.selected += 1;
                }
                Action::None
            }
            KeyCode::Backspace => {
                self.query.pop();
                self.run();
                Action::None
            }
            KeyCode::Char(c) => {
                self.query.push(c);
                self.run();
                Action::None
            }
            _ => Action::None,
        }
    }

    fn render(&self, f: &mut Frame, area: Rect, cfg: &Config) {
        let block = Block::default()
            .title(format!(" Search · {} ", self.notebook))
            .title_bottom(" type to filter · ↑↓ select · Enter open · Esc close ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(cfg.accent));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let parts = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        let prompt = Line::from(vec![
            Span::styled(
                "rg> ",
                Style::default().fg(cfg.accent).add_modifier(Modifier::BOLD),
            ),
            Span::raw(self.query.as_str()),
            Span::styled("█", Style::default().fg(cfg.accent)),
        ]);
        f.render_widget(Paragraph::new(prompt), parts[0]);

        if self.results.is_empty() {
            let msg = Paragraph::new(self.note.as_str()).style(Style::default().fg(Color::DarkGray));
            f.render_widget(msg, parts[1]);
            return;
        }

        let rows: Vec<ListItem> = self
            .results
            .iter()
            .map(|h| {
                let label = self.labels.get(&h.path).map(String::as_str).unwrap_or("?");
                ListItem::new(format!("{label}  :{}  {}", h.line, h.text))
            })
            .collect();
        let list = List::new(rows)
            .highlight_style(Style::default().fg(cfg.highlight).add_modifier(Modifier::BOLD))
            .highlight_symbol("▌ ");
        let mut state = ListState::default();
        state.select(Some(self.selected));
        f.render_stateful_widget(list, parts[1], &mut state);
    }
}
