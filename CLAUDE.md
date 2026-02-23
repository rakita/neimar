# Neimar

TUI app for managing multiple Claude conversation sessions in a terminal.

## Build & Run

```bash
cargo build          # compile
cargo run            # run the app
```

Requires `claude` CLI on PATH.

## Architecture

Multi-module app using ratatui + crossterm + tokio. PTY-based multiplexer — each Claude session runs inside a real pseudo-terminal.

### Module Structure

```
src/
  main.rs      — mod declarations, process_frame(), main(), run()
  session.rs   — Session, SessionStatus, ClaudeStatus*, constants
  event.rs     — AppEvent enum, apply_event()
  app.rs       — App struct, Focus, InputMode, LeftTab, AgentFile, session lifecycle
  input.rs     — impl App input handlers, key_to_bytes()
  ui.rs        — impl App rendering methods
  mouse.rs     — mouse click hit-testing
```

### Core Approach: PTY Multiplexer

Each session spawns `claude` inside a pseudo-terminal via `portable-pty`. Raw terminal output is captured through `vt100::Parser` and rendered with `tui-term::PseudoTerminal`. This gives full terminal fidelity (colors, cursor movement, line wrapping) without needing to parse Claude's output format. Keyboard input is forwarded as raw terminal bytes.

### Event Loop

- **Dedicated keyboard thread** (OS thread, not tokio task) reads crossterm events and sends `KeyEvent` via `mpsc::unbounded_channel` — never blocked by async runtime
- **Tokio select loop** with biased priorities: key events first, then 30fps render tick
- **`process_frame()`** — shared helper that drains pending keys and PTY events (capped at `MAX_PTY_EVENTS_PER_FRAME = 500`), polls status files, and renders
- Status file polling throttled to every 2 seconds (not every frame)

### Key Components

- **`App`** — main state: sessions list, session ID→index map, focus tracking, input buffer, event sender
- **`Session`** — per-conversation state: name, status (Running/Completed/Failed), vt100 parser, PTY master/writer, child process handle, scroll offset
- **`Focus`** (Sessions | Terminal) + **`InputMode`** (Normal | NamingSession) — two-axis input routing
- **`AppEvent`** enum — channel messages from PTY reader threads to UI: `PtyOutput(id, bytes)`, `PtyExited(id)`

### Claude Subprocess

- Spawns `claude` CLI inside a PTY with `TERM=xterm-256color`
- Sets `NEIMAR_STATUS_FILE` env var for Claude's status line hook to write metadata (model, cost, context%)
- Single OS thread per session reads PTY master and sends `AppEvent::PtyOutput` / `AppEvent::PtyExited`
- Child process handle stored in `Session` for lifecycle management

### Rendering

Three-panel layout: session list (left, fixed 30 cols) | PTY output (right) | status bar (bottom, 3 rows)
- `PseudoTerminal` widget renders the vt100 parser screen directly
- Color-coded borders by session status (yellow=running, green=completed, red=failed)
- Terminal resize propagated to ALL sessions (not just the visible one) so background sessions stay in sync
- New sessions created with the last known panel size instead of hardcoded defaults

### Session Lifecycle

```
[n key] → NamingSession → [Enter] → create_session()
                                        │
                                  PTY alloc + spawn claude
                                        │
                                  spawn reader thread
                                        │
                                    Running ──► Completed (PtyExited)
                                       │
                                  [x key] → archived + Ctrl+D sent
                                  [r key] → removed from Vec
```

### Shutdown

On quit, `shutdown()` sends Ctrl+C then Ctrl+D to all running sessions, drops PTY writers/masters (triggering SIGHUP to child processes and EOF on reader threads), and cleans up status files.
