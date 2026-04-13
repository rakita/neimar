<p align="center">
  <img src="assets/logo.png" alt="Neimar Logo" width="300" />
</p>

# Neimar

TUI multiplexer for managing multiple AI CLI sessions in a terminal. Run several Claude, Amp, or shell sessions side-by-side with full terminal fidelity.

The name *neimar* is a Serbian word meaning "builder" or "master builder" — fitting for a tool that helps you build things with AI.

![Neimar](assets/console.png)

## Install

Install [Rust](https://www.rust-lang.org/tools/install) (if you don't have it):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Then install neimar:

```bash
cargo install neimar
```

## Features

- **Multi-session** — Run multiple AI sessions simultaneously, switch between them instantly
- **Full PTY fidelity** — Each session runs in a real pseudo-terminal; colors, cursor movement, and line wrapping all work correctly
- **Live status monitoring** — Tracks model name, cost, context window usage, turn count, and permission mode per session
- **AI state classification** — Automatically detects whether each session is working, waiting for input, or done
- **Mouse support** — Click to select sessions, drag to resize panels, scroll output, drag-select text, double-click to select a word
- **Drag-and-drop** — Reorder sessions and labels by dragging them in the sidebar
- **Clipboard integration** — Auto-copy text selections on mouse-up, Cmd+C to copy
- **Scrollback** — Scroll through session output history with keyboard or mouse
- **Multiple CLI types** — Claude, Claude (danger mode), Amp, or plain shell sessions
- **Session labels** — Group sessions under named labels for organization
- **Agents directory** — Browse and view agent definition files from a dedicated tab
- **Resizable panels** — Drag the divider between sidebar and terminal to resize

## Keyboard Shortcuts

### Global

| Key | Action |
|-----|--------|
| `Ctrl+N` | Create a new session |
| `Ctrl+Q` | Quit |

### Sidebar (Sessions panel focused)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate sessions |
| `←` / `→` | Switch between Sessions and Agents tabs |
| `Shift+↑` / `Shift+↓` | Reorder selected session or label |
| `Shift+←` / `Shift+→` | Switch focus between sidebar and terminal |
| `e` | Rename selected session or label |
| `r` | Remove selected session or label |
| `g` | Create a new label |

### Terminal (Terminal panel focused)

| Key | Action |
|-----|--------|
| `Shift+←` / `Shift+→` | Switch focus back to sidebar |
| `Shift+PageUp` / `Shift+PageDown` | Scroll terminal output |
| `Esc` | Snap to bottom (when scrolled) / clear selection |
| `Cmd+C` | Copy selection to clipboard (when text is selected) |

All other keys are forwarded directly to the PTY.

### Session Creation

| Key | Action |
|-----|--------|
| `1` | Quick-select Claude |
| `2` | Quick-select Claude (danger mode) |
| `3` | Quick-select Amp |
| `4` | Quick-select Console |
| `←` / `→` / `Tab` | Navigate options |
| `Enter` | Confirm selection |
| `Esc` | Cancel |

## Mouse Interactions

- **Click session** — Select and focus sidebar
- **Click terminal** — Focus terminal, start text selection
- **Double-click terminal** — Select word and copy to clipboard
- **Drag session/label** — Reorder in sidebar
- **Drag divider** — Resize panels
- **Drag on terminal** — Select text (auto-copied on mouse-up)
- **Drag scrollbar** — Scroll terminal or session list
- **Scroll wheel** — Scroll sessions, terminal output, or agent content

## Status Indicators

### Session State

| Emoji | State | Color | Meaning |
|-------|-------|-------|---------|
| 🧱 | Working | Yellow | Session is actively producing output |
| 💬 | Input | Cyan | Waiting for user input |
| 📋 | Planned | Magenta | Plan prompt shown, awaiting approval |
| 🟢 | Done | Green | Session is idle |
| ⏳ | Starting | Gray | Session just started |
| 🔒 | Closed | Gray | Session exited normally |
| 🔴 | Failed | Red | Session failed to start |

### Permission Mode

| Emoji | Mode | Meaning |
|-------|------|---------|
| ⏸ | Plan | Claude is in plan mode |
| ⏩ | AcceptEdits | Claude auto-accepts edits |

## Session Types

When creating a new session (`Ctrl+N`), choose a type:

| # | Type | Icon | Description |
|---|------|------|-------------|
| 1 | Claude | 🤖 | Claude CLI session |
| 2 | Claude (Danger) | 🤖💥 | Claude CLI with `--dangerously-skip-permissions` |
| 3 | Amp | ⚡ | Amp CLI session |
| 4 | Console | 🖥️ | Plain shell session |

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT License](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this crate by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
