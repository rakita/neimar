# Neimar

TUI multiplexer for managing multiple AI CLI sessions in a terminal. Run several Claude, Amp, or shell sessions side-by-side with full terminal fidelity.

```
┌─ Sessions ─────────────────┬─ claude-refactor | opus-4 | T:12 | $0.42 | 38% ──┐
│ > 🧱 claude-refactor  WORK │                                                   │
│   Refactoring auth module  │  ● I'll refactor the authentication module.       │
│                            │                                                   │
│   💬 amp-review      INPUT │  First, let me read the existing code...          │
│   Waiting for approval     │                                                   │
│                            │  src/auth.rs                                      │
│   🟢 bugfix-123       DONE │  ┌──────────────────────────────────────────┐     │
│   Fixed null pointer       │  │ pub fn authenticate(token: &str) -> bool │   ▓ │
│                            │  │     validate(token)                      │   ░ │
│                            │  │ }                                        │   ░ │
│                            │  └──────────────────────────────────────────┘     │
├─ Agents ───────────────────┤                                                   │
│   code-reviewer            │                                                   │
│   test-writer              │                                                   │
└────────────────────────────┴───────────────────────────────────────────────────┘
┌─ neimar ──────────────────────────────────────────────────────────────────────┐
│ Shift(⇧)+←/→: panel  ←/→: tab  n: new  l: ralph  e: rename  q: quit        │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Features

- **Multi-session** — Run multiple AI sessions simultaneously, switch between them instantly
- **Full PTY fidelity** — Each session runs in a real pseudo-terminal; colors, cursor movement, and line wrapping all work correctly
- **Live status monitoring** — Tracks model name, cost, context window usage, turn count, and permission mode per session
- **AI state classification** — Automatically detects whether each session is working, waiting for input, or done
- **Mouse support** — Click to select sessions, drag to resize panels, scroll output, drag-select text
- **Clipboard integration** — Auto-copy text selections, Cmd+C to copy
- **Scrollback** — Scroll through session output history with keyboard or mouse
- **Multiple CLI types** — Claude, Amp, or plain shell sessions
- **Agents directory** — Browse and view agent definition files from a dedicated tab
- **Ralph loop** — Automated prompt injection for iterative Claude workflows

## Requirements

- **Rust** (2024 edition)
- **`claude` CLI** on PATH (for Claude sessions)
- **`amp` CLI** on PATH (optional, for Amp sessions)

## Build & Run

```bash
cargo build          # compile
cargo run            # run the app
```

## Keybindings

### Session List (left panel focused)

| Key | Action |
|-----|--------|
| `n` | Create new session |
| `l` | Create Ralph loop session |
| `e` | Rename selected session |
| `r` | Remove selected session |
| `h` | Toggle half-width left panel |
| `j` / `↓` | Navigate down |
| `k` / `↑` | Navigate up |
| `←` / `→` | Switch between Sessions and Agents tabs |
| `q` | Quit |

### Terminal Panel (right panel focused)

| Key | Action |
|-----|--------|
| Any key | Forwarded to the session's PTY |
| `Cmd+C` | Copy selection to clipboard (when text is selected) |
| `Esc` | Snap to bottom, clear selection (when scrolled) |

### Navigation (works from either panel)

| Key | Action |
|-----|--------|
| `Shift+←` / `Shift+→` | Switch focus between panels |
| `Shift+PgUp` / `Shift+PgDn` | Scroll session output |
| `Shift+↑` / `Shift+↓` | Scroll one line |

### Mouse

| Action | Effect |
|--------|--------|
| Click session | Select and focus session |
| Click right panel | Start text selection, focus terminal |
| Drag on divider | Resize panels |
| Drag on right panel | Select text (auto-copied on release) |
| Scroll wheel | Scroll session output |

## Session Types

When creating a new session (`n`), choose a type:

| # | Type | Icon | Description |
|---|------|------|-------------|
| 1 | Claude | 🤖 | Claude CLI session |
| 2 | Amp | ⚡ | Amp CLI session |
| 3 | Console | >_ | Plain shell session |

## Status Indicators

### Session State

| Emoji | State | Meaning |
|-------|-------|---------|
| 🧱 | Working | Session is actively producing output |
| 💬 | Prompt | Waiting for user input |
| ⏳ | Starting | Session just started |
| 🟢 | Done | Session completed |
| 🔴 | Failed | Session failed to start |

### Permission Mode (Claude sessions)

| Emoji | Mode | Meaning |
|-------|------|---------|
| ⏸ | Plan | Claude is in plan mode |
| ⏩ | Edit | Claude is auto-accepting edits |
