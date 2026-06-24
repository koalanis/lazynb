# lazynb

A [lazygit](https://github.com/jesseduffield/lazygit)-style terminal UI for
[`nb`](https://github.com/xwmx/nb), the plain-text notebook CLI. Built with
[ratatui](https://ratatui.rs).

`nb` handles storage, search and git sync. lazynb sits on top of it so you
can browse notebooks and notes, preview them, and open them in your editor
without typing `nb` commands.

## Status

Early days. So far it can:

- Show notebooks, notes and a live preview side by side
- Move around with `j`/`k`, switch panels with `tab`, reload with `r`
- Open the selected note with `enter` (in your editor, or the Neovim session
  it was launched from)

## Install

```sh
cargo install --path .
# or: cargo build --release, then run target/release/lazynb
```

You need [`nb`](https://github.com/xwmx/nb) on your `PATH`.

## Keys

| Key        | Action                              |
| ---------- | ----------------------------------- |
| `j` / `k`  | Move down / up in the focused panel |
| `tab`      | Switch focus (notebooks / notes)    |
| `1` / `2`  | Focus notebooks / notes directly    |
| `enter`    | Open the selected note              |
| `r`        | Reload the current notebook         |
| `t`        | Tag list / filter the notes panel   |
| `b`        | Backlinks to the selected note      |
| `l`        | Jump to a `[[link]]` in the note    |
| `/`        | Live grep the notebook (ripgrep)    |
| `g`        | Relationship graph of the notebook  |
| `:`        | Open the nb shell (run nb commands) |
| `q` / `esc`| Quit                                |

## Tags and links

- `t` opens a picker of every tag in the current notebook. Pick one to filter
  the notes panel to that tag; the `[ all notes ]` row clears the filter. The
  active filter shows in the Notes panel title and the status bar.
- `b` lists the notes that link to the selected note via `[[Title]]`. Pick one
  to jump to it.
- `l` lists the `[[wiki links]]` inside the selected note. Pick one to jump to
  its target (switching notebook if needed).

All of these are thin wrappers over nb primitives (`nb ls --tag`,
`nb <nb>:search`), and they're built on a small reusable overlay system —
see [Extending](#extending).

## Search

`/` opens a Telescope-style live grep over the selected notebook, powered by
ripgrep. Each keystroke re-runs `rg` across the notebook's note files; results
show `[id] Title :line matched-text`. Enter jumps to the match. Needs `rg` on
your `PATH`.

## Graph

`g` opens a relationship graph of the current notebook, like Obsidian or
Quartz: each note is a node, `[[links]]` are solid edges and shared `#tags`
are dim edges. The layout is a Fruchterman-Reingold force simulation that
visibly settles over a few seconds, drawn on a braille canvas so edges read as
smooth lines rather than blocky ASCII.

- `↑`/`↓` (or `j`/`k`, `tab`) move the selection; the selected node and its
  edges highlight.
- `enter` jumps to the selected note.
- `t` toggles the shared-tag edges; `r` re-runs the layout.
- `esc` closes.

## Extending

Modal widgets live in `src/overlay.rs` and implement the `Overlay` trait
(handle a key, draw yourself). Instead of mutating the app directly, an
overlay returns an `Action`; `App::apply` is the one place that interprets
actions. Most overlays are just lists, so `Picker` is a reusable widget you
configure with data — `(label, action)` rows — and it provides navigation,
incremental filtering, and selection for free. The tag/backlink/link features
are each a few lines that assemble a `Picker`. To add a new overlay: add an
`Action` variant if you need a new effect, handle it in `App::apply`, then
build a `Picker` (or implement `Overlay`) and open it from an `App::open_*`
method bound to a key in `main.rs`. Colors live in `src/config.rs`.

The overlays span the range the trait is meant to cover: `Picker` (tags,
backlinks, links) is pure data; `Shell` and `Search` are input + output;
`Graph` (`src/graph.rs`) is a fully custom widget that animates via the
trait's `tick`/`animating` hooks (the event loop ticks the overlay and polls
faster while it reports `animating`).

## nb shell

Press `:` to open a small command modal. Type any nb command (the leading
`nb` is optional, like nb's own shell), press enter, and the output shows
inline. Up and down recall history, `exit` or `esc` closes it, and the
panels reload afterward so new or deleted notes show up. `EDITOR` is set to
a no-op while a command runs, so `add` and friends can't hang waiting for an
editor that has nowhere to draw.

## Neovim integration

lazynb is a standalone binary, not a Neovim plugin. Like lazygit, you run it
in a Neovim terminal buffer. When you open a note from inside Neovim, lazynb
sends the file to that Neovim session over its `$NVIM` RPC socket rather than
starting a second editor.

Here's a command that opens it in a floating window:

```lua
vim.api.nvim_create_user_command("LazyNb", function()
  local buf = vim.api.nvim_create_buf(false, true)
  local width = math.floor(vim.o.columns * 0.9)
  local height = math.floor(vim.o.lines * 0.9)
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = width,
    height = height,
    row = math.floor((vim.o.lines - height) / 2),
    col = math.floor((vim.o.columns - width) / 2),
    style = "minimal",
    border = "rounded",
  })
  -- Neovim sets $NVIM in terminal buffers; lazynb reads it to open notes
  -- back in this session.
  vim.fn.termopen("lazynb", {
    on_exit = function()
      if vim.api.nvim_win_is_valid(win) then
        vim.api.nvim_win_close(win, true)
      end
    end,
  })
  vim.cmd.startinsert()
end, {})
```

Then run `:LazyNb`.
