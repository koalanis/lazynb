//! lazynb -- a lazygit-style terminal UI for the `nb` notebook CLI.
//!
//! Runs as a self-contained ratatui app. When launched inside Neovim's
//! built-in terminal (`:terminal lazynb`), opening a note hands the file off
//! to the parent Neovim via its `$NVIM` RPC socket instead of nesting a new
//! editor -- the same trick lazygit uses.

mod app;
mod config;
mod nb;
mod overlay;
mod ui;

use anyhow::Result;
use app::App;
use ratatui::backend::{Backend, CrosstermBackend, TestBackend};
use ratatui::crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::Terminal;
use std::io::stdout;
use std::process::Command;
use std::time::Duration;

fn main() -> Result<()> {
    // `lazynb snapshot [WxH]` renders one frame to text and exits, for CI and
    // scripting where there's no TTY to host the interactive UI.
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("snapshot") {
        return snapshot(args.next().as_deref());
    }

    let mut app = App::new()?;

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;

    let res = run(&mut terminal, &mut app);

    // Always restore the terminal, even if the loop errored.
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res?;

    // The handoff happens after the alternate screen is torn down so the
    // editor (or the parent nvim) draws on a clean terminal.
    if let Some(path) = app.open_request {
        open_note(&path)?;
    }
    Ok(())
}

fn run<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<()> {
    while !app.should_quit {
        terminal.draw(|f| ui::draw(f, app))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        // While an overlay is open it owns all keys.
        if app.overlay.is_some() {
            let action = app.overlay.as_mut().unwrap().on_key(key);
            app.apply(action);
            continue;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => app.next(),
            KeyCode::Char('k') | KeyCode::Up => app.prev(),
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Char('1') => app.focus = app::Panel::Notebooks,
            KeyCode::Char('2') => app.focus = app::Panel::Notes,
            KeyCode::Char('r') => app.reload_notes(),
            KeyCode::Char('t') => app.open_tag_list(),
            KeyCode::Char('b') => app.open_backlinks(),
            KeyCode::Char('l') => app.open_links(),
            KeyCode::Char(':') => app.open_shell(),
            KeyCode::Enter => app.open_selected(),
            _ => {}
        }
    }
    Ok(())
}

/// Render a single frame to stdout as plain text and exit. Uses ratatui's
/// in-memory `TestBackend`, so it needs no terminal -- handy for CI and for
/// eyeballing the layout. `size` is an optional `WIDTHxHEIGHT` (default 110x30).
fn snapshot(size: Option<&str>) -> Result<()> {
    let (width, height) = size.and_then(parse_size).unwrap_or((110, 30));
    let app = App::new()?;

    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|f| ui::draw(f, &app))?;

    let buf = terminal.backend().buffer();
    let area = buf.area;
    let mut out = String::with_capacity((area.width as usize + 1) * area.height as usize);
    for y in 0..area.height {
        for x in 0..area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    print!("{out}");
    Ok(())
}

/// Parse a `WIDTHxHEIGHT` string like `100x28`.
fn parse_size(s: &str) -> Option<(u16, u16)> {
    let (w, h) = s.split_once(['x', 'X'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Open `path` in the user's editor. Prefers the parent Neovim instance when
/// running inside its terminal, so notes land in the existing session.
fn open_note(path: &str) -> Result<()> {
    if let Ok(socket) = std::env::var("NVIM") {
        Command::new("nvim")
            .args(["--server", &socket, "--remote", path])
            .status()?;
        return Ok(());
    }
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
    Command::new(editor).arg(path).status()?;
    Ok(())
}
