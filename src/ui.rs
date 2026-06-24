//! Rendering. A lazygit-style layout: a left sidebar with the notebooks and
//! notes panels stacked, a preview pane on the right, and a help line below.
//! Any open [`Overlay`] is drawn last, as a centered popup.

use crate::app::{App, Panel};
use crate::config::Config;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
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

    if let Some(overlay) = &app.overlay {
        let (pw, ph) = overlay.size();
        let popup = centered_rect(pw, ph, f.area());
        f.render_widget(Clear, popup);
        overlay.render(f, popup, &app.config);
    }
}

fn border_style(focused: bool, cfg: &Config) -> Style {
    if focused {
        Style::default().fg(cfg.focus)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn panel_block<'a>(title: &str, focused: bool, cfg: &Config) -> Block<'a> {
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(border_style(focused, cfg))
}

fn highlight(cfg: &Config) -> Style {
    Style::default().fg(cfg.highlight).add_modifier(Modifier::BOLD)
}

fn draw_notebooks(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .notebooks
        .iter()
        .map(|n| ListItem::new(n.as_str()))
        .collect();
    let list = List::new(items)
        .block(panel_block("Notebooks", app.focus == Panel::Notebooks, &app.config))
        .highlight_style(highlight(&app.config))
        .highlight_symbol("▌ ");
    let mut state = ListState::default();
    state.select(Some(app.notebook_idx));
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_notes(f: &mut Frame, app: &App, area: Rect) {
    let title = match &app.tag_filter {
        Some(tag) => format!("Notes  #{tag}"),
        None => "Notes".to_string(),
    };
    let items: Vec<ListItem> = app
        .notes
        .iter()
        .map(|note| ListItem::new(format!("[{}] {}", note.id, note.title)))
        .collect();
    let list = List::new(items)
        .block(panel_block(&title, app.focus == Panel::Notes, &app.config))
        .highlight_style(highlight(&app.config))
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
    let para = Paragraph::new(text).block(panel_block(&title, false, &app.config));
    f.render_widget(para, area);
}

fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let key = |k: &'static str| Span::styled(k, Style::default().fg(Color::Cyan));
    let line = Line::from(vec![
        key("j/k"),
        Span::raw(" move  "),
        key("tab"),
        Span::raw(" switch  "),
        key("t"),
        Span::raw(" tags  "),
        key("b"),
        Span::raw(" backlinks  "),
        key("l"),
        Span::raw(" links  "),
        key("enter"),
        Span::raw(" open  "),
        key(":"),
        Span::raw(" shell  "),
        key("q"),
        Span::raw(" quit  "),
        Span::styled(app.status.clone(), Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// A rectangle centered in `area`, sized as a percentage of it.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(rows[1])[1]
}
