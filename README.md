# boroda

A minimalistic console text and code editor with LSP support, (vibe)coded  on Rust.

`boroda` runs entirely in the terminal and combines a small, keyboard-driven editor with
asynchronous LSP integration: syntax highlighting, diagnostics, go-to-definition, modal
dialogs and multi-file editing.

## Key features

### Terminal-native editing

- Full-screen TUI built on `crossterm`; mouse support, alternate screen and raw mode.
- Cursor movement by character, word, line and page; Home/End and mouse clicks.
- Character selection with `Shift` + arrows or `Shift` + click, including multi-line
  selection for copy, cut and delete.
- Insert, newline with automatic indentation, Backspace/Delete, Tab as 4 spaces,
  clear-current-line and auto-indent toggle.
- System clipboard support (with an in-memory fallback when no clipboard is available).
- Line-based undo/redo with merging of consecutive edits on the same line.

### Multi-file workflow

- Open files passed as command-line arguments.
- With no arguments, automatically loads supported files from the current directory.
- Falls back to an empty `untitled.txt` buffer when nothing is found.
- Switch between open files with `Ctrl+Tab`, `Ctrl+PageUp`/`Ctrl+PageDown`, or the
  "Opened files" menu available from the status bar.
- Go-to-definition automatically opens the target file if it is not open yet.

### Syntax highlighting

| Files | Highlighting source |
| --- | --- |
| Rust, PHP and other configured LSP file types | LSP semantic tokens |
| Python | Built-in regex tokenizer |
| Markdown | Built-in Markdown tokenizer (headings, code blocks, bold/italic, inline code, links) |
| JSON / TOML / INI / CONF | Built-in config tokenizer (keys, sections, strings, numbers, literals) |
| Everything else | Plain text |

Colors are driven by configurable token palettes, with per-extension overrides.

### LSP integration

- Dynamic LSP server pool configured in `~/.boroda.json`.
- Preconfigured support for Python (`pyright-langserver`), Rust (`rust-analyzer`)
  and PHP (`intelephense`).
- Lazy server startup: a server is launched only when a matching file is opened.
- Handles the `initialize` / `initialized` handshake, `didOpen`, `didChange`,
  `didSave` and semantic-token requests.
- Debounced document synchronization (700 ms after the last edit).
- Go-to-definition via `F4` or `Ctrl` + left click.

### Diagnostics

- LSP `publishDiagnostics` errors and warnings are collected per editor.
- Error/warning icons are shown in the status bar.
- The current line's diagnostic message is displayed in the bottom bar.
- Clicking the error/warning icons opens a menu with all diagnostics; selecting an
  entry jumps directly to the relevant line and column.

### Modal dialogs

- Text-input modal for search: plain substring search from the cursor with wrap-around.
- Confirmation modal for quitting (`Y`/`N`, arrow keys, Tab, mouse or Enter/Esc).
- Scrollable menu modal for the open-files list and the diagnostics list.
- Modal lifecycle (open, input, result handling and redraw) is owned by
  `IntermediateState`; input translation stays in `InputDriver`.

### Configuration

- Automatically creates `~/.boroda.json` on first launch.
- Configures file-extension → LSP server mappings, LSP server commands/arguments,
  and color palettes.
- Palettes can be defined per file extension, with a shared default palette.
- A panic hook restores the terminal before printing a panic message.

## Keyboard shortcuts

| Key | Action |
| --- | --- |
| `F1` | Toggle automatic indentation on new lines |
| `F2` | Save file |
| `F3` | Open search modal |
| `F4` | Go to definition |
| `F5` | Copy selection |
| `F6` | Paste |
| `F7` | Clear current line |
| `F8` | Cut selection |
| `F9` | Move to start of line |
| `F10` | Confirm and quit |
| `F11` | Save and quit |
| `F12` | Reload file from disk |
| `Ctrl+Z` | Undo |
| `Ctrl+Y`, `Ctrl+Shift+Z` | Redo |
| `Ctrl+Left` / `Ctrl+Right` | Move by word |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Previous / next open file |
| `Ctrl+Tab` | Open file switcher |
| `Ctrl+Insert` | Paste |
| `Tab` | Insert 4 spaces |
| `Shift+Arrows`, `Shift+Click` | Extend selection |
| `Ctrl+Click` | Go to definition |
| `Mouse wheel` | Scroll |

Mouse shortcuts:

- Click the status bar header to open the file switcher.
- Click the error/warning icons in the header to open the diagnostics menu.
- Right-click the header to switch to the next open file.

## Configuration file

`~/.boroda.json` has the following shape:

```json
{
  "file_types": {
    "py": "python",
    "rs": "rust",
    "php": "php"
  },
  "autoload_extra": ["md", "ini", "conf", "txt"],
  "lsp_servers": {
    "python": { "command": "pyright-langserver", "args": ["--stdio"] },
    "rust": { "command": "rust-analyzer", "args": [] },
    "php": { "command": "intelephense", "args": ["--stdio"] }
  },
  "color_palettes": {},
  "default_palette": {
    "0": "\u001b[90m",
    "1": "\u001b[0;36m",
    "default": "\u001b[0m"
  }
}
```

`file_types` maps a file extension to a server ID from `lsp_servers`.
`autoload_extra` lists extensions that are opened automatically even when no LSP server
is configured for them. Files selected for loading are sorted alphabetically by their
full path.
`color_palettes` maps an extension to palette overrides; missing entries fall back to
`default_palette`, where keys are semantic token type IDs and `default` is the reset style.

## Build and run

```bash
cargo build --release
./target/release/boroda [FILE...]
```

Requires a Rust toolchain with edition 2021 support and the LSP servers you want to use
available in `PATH` (for example `pyright-langserver`, `rust-analyzer`, `intelephense`).

## Project layout

| Module | Responsibility |
| --- | --- |
| `src/main.rs` | Async event loop, terminal setup and LSP debounce |
| `src/traits.rs` | `TextInteractive`, `FileOperations`, `ModalInteractive` contracts |
| `src/base_editor.rs` | Text buffer, cursor, selection, history and rendering |
| `src/editor.rs` | `Editor` = `BaseEditor` + `FileContext` composition |
| `src/intermediate.rs` | Hotkeys, modal lifecycle and modal result handling |
| `src/input.rs` | Keyboard/mouse → editor and modal actions |
| `src/modal.rs` | Text input, confirmation and menu modals |
| `src/history.rs` | Line-based undo/redo history |
| `src/loader.rs` | Project scanning and editor creation |
| `src/lsp.rs` | LSP process management and JSON-RPC transport |
| `src/lsp_adapter.rs` | LSP message parsing, tokens, diagnostics and definitions |
| `src/rx_worker.rs` | Async dispatch of LSP responses to editors |
| `src/tokens.rs` | Shared token, selection and diagnostic types |
| `src/config.rs` | `~/.boroda.json` loading and palette compilation |
| `src/clipboard.rs` | System clipboard with in-memory fallback |
| `src/*_tokenizer.rs` | Local Python, Markdown and config highlighters |
