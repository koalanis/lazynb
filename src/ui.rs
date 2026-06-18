//! Rendering. A lazygit-style layout: a left sidebar with the notebooks and
//! notes panels stacked, a preview pane on the right, and a help line below.

use crate::app::{App, Panel};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(f.area());

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(root[0]);

    let sidebar = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            // Notebooks panel grows with the list, capped so notes stay visible.
            Constraint::Length((app.notebooks.len() as u16).clamp(1, 8) + 2),
            Constraint::Min(3),
        ])
        .split(cols[0]);

    draw_notebooks(f, app, sidebar[0]);
    draw_notes(f, app, sidebar[1]);
    draw_preview(f, app, cols[1]);
    draw_help(f, app, root[1]);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn panel_block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(border_style(focused))
}

fn highlight() -> Style {
    Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD)
}

fn draw_notebooks(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .notebooks
        .iter()
        .map(|n| ListItem::new(n.as_str()))
        .collect();
    let list = List::new(items)
        .block(panel_block("Notebooks", app.focus == Panel::Notebooks))
        .highlight_style(highlight())
        .highlight_symbol("▌ ");
    let mut state = ListState::default();
    state.select(Some(app.notebook_idx));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_notes(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .notes
        .iter()
        .map(|note| ListItem::new(format!("[{}] {}", note.id, note.title)))
        .collect();
    let list = List::new(items)
        .block(panel_block("Notes", app.focus == Panel::Notes))
        .highlight_style(highlight())
        .highlight_symbol("▌ ");
    let mut state = ListState::default();
    if !app.notes.is_empty() {
        state.select(Some(app.note_idx));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let title = app
        .current_note()
        .map(|n| n.title.clone())
        .unwrap_or_else(|| "Preview".to_string());
    let text: Vec<Line> = app.preview.iter().map(|l| Line::raw(l.as_str())).collect();
    let para = Paragraph::new(text).block(panel_block(&title, false));
    f.render_widget(para, area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let key = |k: &'static str| Span::styled(k, Style::default().fg(Color::Cyan));
    let sep = Span::raw("  ");
    let line = Line::from(vec![
        key("j/k"),
        Span::raw(" move  "),
        key("tab"),
        Span::raw(" switch  "),
        key("enter"),
        Span::raw(" open  "),
        key("r"),
        Span::raw(" reload  "),
        key("q"),
        Span::raw(" quit"),
        sep,
        Span::styled(
            app.status.clone(),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
