//! Thin wrapper around the `nb` CLI. We only ever ask `nb` for machine-
//! friendly output (no color/header/footer, absolute paths) and parse the
//! resulting lines -- `nb` owns storage, search and git sync.

use anyhow::{Context, Result};
use std::io::BufRead;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Note {
    /// The `id` (or `notebook:id`) used to address this note via `nb`.
    pub id: String,
    /// Absolute path to the markdown file on disk.
    pub path: String,
    /// Display title: the first heading, falling back to the filename.
    pub title: String,
}

/// Run an `nb` subcommand with color disabled and return stdout.
fn run(args: &[&str]) -> Result<String> {
    let out = Command::new("nb")
        .args(args)
        .arg("--no-color")
        .output()
        .context("failed to spawn `nb` -- is it installed and on PATH?")?;
    if !out.status.success() {
        anyhow::bail!(
            "`nb {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// All notebook names, one per line.
pub fn notebooks() -> Result<Vec<String>> {
    Ok(run(&["notebooks"])?
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// The name of the currently-selected notebook.
pub fn current_notebook() -> Result<String> {
    Ok(run(&["notebooks", "current"])?.trim().to_string())
}

/// Every note in `notebook`, newest first (the order `nb` returns).
pub fn notes(notebook: &str) -> Result<Vec<Note>> {
    let scope = format!("{notebook}:");
    let out = run(&["ls", "-a", "--no-header", "--no-footer", "--paths", &scope])?;

    let mut notes = Vec::new();
    for line in out.lines() {
        // Each line looks like: `[8] /home/me/.nb/thoth/some_note.md`
        let line = line.trim();
        let Some(rest) = line.strip_prefix('[') else {
            continue;
        };
        let Some((id, path)) = rest.split_once("] ") else {
            continue;
        };
        let path = path.trim().to_string();
        let title = title_for(&path);
        notes.push(Note {
            id: id.to_string(),
            path,
            title,
        });
    }
    Ok(notes)
}

/// Derive a display title from a note's first line (markdown heading), or
/// fall back to the filename stem if the file is empty/unreadable.
fn title_for(path: &str) -> String {
    if let Ok(file) = std::fs::File::open(path) {
        let mut first = String::new();
        if std::io::BufReader::new(file).read_line(&mut first).is_ok() {
            let first = first.trim();
            if !first.is_empty() {
                return first.trim_start_matches('#').trim().to_string();
            }
        }
    }
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}
