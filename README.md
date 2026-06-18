# lazynb

A [lazygit](https://github.com/jesseduffield/lazygit)-style terminal UI for
[`nb`](https://github.com/xwmx/nb), the plain-text notebook CLI. Built with
[ratatui](https://ratatui.rs).

`nb` does the heavy lifting (storage, search, git sync). `lazynb` is a fast,
self-contained TUI front-end: browse notebooks and notes, preview them, and
open them in your editor.

## Status

Early skeleton. Working today:

- Notebooks panel + notes panel + live preview pane
- `j`/`k` navigation, `tab` to switch panels, `r` to reload
- `enter` opens the selected note in your editor (or the parent Neovim
  instance — see below)

## Install

```sh
cargo install --path .
# or: cargo build --release  ->  target/release/lazynb
```

Requires [`nb`](https://github.com/xwmx/nb) on your `PATH`.

## Keys

| Key        | Action                              |
| ---------- | ----------------------------------- |
| `j` / `k`  | Move down / up in the focused panel |
| `tab`      | Switch focus (notebooks ↔ notes)    |
| `1` / `2`  | Focus notebooks / notes directly    |
| `enter`    | Open the selected note              |
| `r`        | Reload the current notebook         |
| `q` / `esc`| Quit                                |

## Neovim integration

`lazynb` is a standalone binary, not a Neovim plugin. Like lazygit, you run
it in a Neovim terminal buffer. When launched inside Neovim, opening a note
hands the file to the *parent* Neovim via its `$NVIM` RPC socket instead of
nesting a new editor.

Drop-in floating-terminal wrapper:

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
  -- $NVIM is set automatically in terminal buffers; lazynb reads it to open
  -- notes back in this session.
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

Then `:LazyNb`.
