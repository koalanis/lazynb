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

/// Run an arbitrary nb command line as typed in the shell modal and return its
/// combined stdout+stderr as display lines. Never errors -- whatever nb prints
/// (including failures) is shown to the user.
///
/// A leading `nb` is accepted and dropped, mirroring nb's own interactive
/// shell. `EDITOR` is forced to a no-op so commands like `add` (which would
/// otherwise spawn an editor) can't hang the UI with no TTY to draw into.
pub fn run_command(line: &str) -> Vec<String> {
    let mut args = tokenize(line);
    if args.first().map(String::as_str) == Some("nb") {
        args.remove(0);
    }
    if args.is_empty() {
        return Vec::new();
    }

    let output = Command::new("nb")
        .args(&args)
        .arg("--no-color")
        .env("EDITOR", "true")
        .output();
    let output = match output {
        Ok(o) => o,
        Err(e) => return vec![format!("failed to run nb: {e}")],
    };

    let mut lines = Vec::new();
    for chunk in [&output.stdout, &output.stderr] {
        for raw in String::from_utf8_lossy(chunk).lines() {
            lines.push(strip_ansi(raw));
        }
    }
    lines
}

/// Split a command line into arguments, honoring single and double quotes so
/// that `add -t "My title"` parses as three args.
fn tokenize(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut token = false;
    let (mut single, mut double) = (false, false);
    for c in line.chars() {
        match c {
            '\'' if !double => {
                single = !single;
                token = true;
            }
            '"' if !single => {
                double = !double;
                token = true;
            }
            c if c.is_whitespace() && !single && !double => {
                if token {
                    args.push(std::mem::take(&mut cur));
                    token = false;
                }
            }
            c => {
                cur.push(c);
                token = true;
            }
        }
    }
    if token {
        args.push(cur);
    }
    args
}

/// Strip ANSI escape sequences. `nb` still emits a few (charset selects, the
/// autowrap toggles around its horizontal rules) even with `--no-color`.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            // CSI sequence: ESC [ ... <final byte @..~>
            Some('[') => {
                chars.next();
                while let Some(&p) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&p) {
                        break;
                    }
                }
            }
            // Charset designation: ESC ( X  or  ESC ) X
            Some('(') | Some(')') => {
                chars.next();
                chars.next();
            }
            // Any other escape: drop the single following char.
            _ => {
                chars.next();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_splits_on_whitespace() {
        assert_eq!(tokenize("ls -a --tags todo"), ["ls", "-a", "--tags", "todo"]);
    }

    #[test]
    fn tokenize_honors_quotes() {
        assert_eq!(
            tokenize(r#"add -t "My title" -c 'a b'"#),
            ["add", "-t", "My title", "-c", "a b"]
        );
    }

    #[test]
    fn tokenize_keeps_empty_quoted_arg() {
        assert_eq!(tokenize(r#"add -c """#), ["add", "-c", ""]);
    }

    #[test]
    fn strip_ansi_removes_color_and_charset_codes() {
        // SGR color, a reset, and an ESC ( B charset select around plain text.
        let input = "\u{1b}[38;5;69mthoth\u{1b}(B\u{1b}[m";
        assert_eq!(strip_ansi(input), "thoth");
    }

    #[test]
    fn strip_ansi_leaves_plain_text_untouched() {
        assert_eq!(strip_ansi("[3] Design Bible"), "[3] Design Bible");
    }
}
