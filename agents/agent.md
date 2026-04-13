# Agent — Project Context

Read the CLAUDE.md file for full architecture details before making changes.

## Project

### Neimar (`/Users/draganrakita/workspace/neimar`)

TUI terminal multiplexer for managing multiple AI bot sessions (Claude, Amp) and shell consoles. Built with Rust using ratatui + crossterm + tokio.

**How it works**: Each session spawns a CLI process inside a real PTY via `portable-pty`. Terminal output flows through `vt100::Parser` and renders with `tui-term::PseudoTerminal`. Keyboard input is forwarded as raw terminal bytes.

**Module layout**:
- `main.rs` — entry point, event loop (`tokio::select!` with biased key events + 30fps render tick)
- `app.rs` — `App` struct: session management, sidebar, labels, agents, clipboard, shutdown
- `session.rs` — `Session`: PTY I/O, vt100 parser, resize, scrollback, status polling, state inference
- `input.rs` — key/mouse event handling, `key_to_bytes()` terminal escape translation
- `ui.rs` — rendering: session list, PTY output panel, agent viewer, status bar
- `event.rs` — `AppEvent` enum (`PtyOutput`, `PtyExited`), `apply_event()`
- `mouse.rs` — click hit-testing for session list
- `types.rs` — all types: `CliType`, `SessionStatus`, `SessionState`, `ClaudeStatus`, `Focus`, `InputMode`, constants

**Session types**: Claude (`claude`), ClaudeDangerous (`claude --dangerously-skip-permissions`), Amp (`amp`), Console (user's shell)

**Key interactions**: `n` = new session, `e` = rename, `r` = remove, `g` = create label, `q` = quit. Shift+Arrow switches panels. Terminal focus forwards all keys to PTY.

**Build**: `cargo build` / `cargo run`. Requires `claude` CLI on PATH.

## After Code Modifications

Always run these after changing code:

```bash
cargo fmt --all          # format all code
cargo clippy             # lint check
cargo build              # verify it compiles
```

Fix any warnings or errors before considering the task done.

## Rules

- Read CLAUDE.md before modifying the project
- The app is PTY-based so unit testing is limited — verify with `cargo build`
- Do not push to `main` without explicit permission
