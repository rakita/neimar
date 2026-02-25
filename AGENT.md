# AGENT.md — Neimar

TUI session multiplexer for Claude/Amp CLI agents, built on PTY-based terminal emulation.

## Build & Run

```bash
cargo build          # compile
cargo run            # run the app
```

Requires `claude` (and/or `amp`) CLI on PATH.

## Module Map

| File | Owns |
|------|------|
| `main.rs` | mod declarations, `process_frame()`, tokio `run()` loop, `main()` |
| `session.rs` | `Session` struct, `SessionStatus`, `SessionState`, `AiState`, `CliType`, `PermissionMode`, `ClaudeStatus*` types, constants |
| `event.rs` | `AppEvent` enum, `apply_event()` dispatcher |
| `app.rs` | `App` struct, `Focus`, `InputMode`, `LeftTab`, `AgentFile`, `Selection`, session lifecycle (`create_session`, `shutdown`), status polling, summary polling |
| `input.rs` | `handle_key()`, `handle_mouse()`, `key_to_bytes()`, all `handle_*_key` methods |
| `ui.rs` | `render()` and all `render_*` methods, `resize_all_sessions()` |
| `mouse.rs` | `clicked_session_index()` hit-testing |

## Critical Invariants

1. **`session_id_map` must stay in sync.** After ANY mutation to `sessions` Vec (push, remove, reorder), call `rebuild_session_id_map()`. Every code path in `create_session_inner` and `handle_sessions_key` already does this — new code must too.

2. **Resize ALL sessions, not just the visible one.** `resize_all_sessions()` iterates every session's parser + PTY master. Background sessions must stay in sync with the panel size or their output will be garbled when switched to.

3. **Reset vt100 scrollback to 0 after temporary use.** `set_scrollback(offset)` changes the parser's view. It MUST be reset to 0 after rendering, clipboard copy, `last_n_lines`, or `max_scrollback`. Leaving it set will cause the parser to malfunction on new output.

4. **Keyboard reader must be an OS thread.** The `std::thread::spawn` in `run()` that calls `crossterm::event::read()` must never become a tokio task. Blocking I/O on the tokio runtime would starve the event loop.

5. **Drop the PTY slave after spawning the child.** `drop(pair.slave)` at app.rs:319 is required — without it the PTY never receives EOF when the child exits, so the reader thread never terminates.

6. **`visible_sessions()` returns real Vec indices.** The list UI selection is an index into the *visible* list. Use `selected_real_index()` to convert to the actual `sessions` Vec index. Never use `sessions[list_state.selected().unwrap()]` directly.

## Where to Put New Code

| What | Where |
|------|-------|
| New session fields | `Session` in session.rs |
| New app-level state | `App` in app.rs |
| New input handling | input.rs — add match arm in appropriate `handle_*_key` method |
| New events | Add variant to `AppEvent` in event.rs, handle in `apply_event()` |
| New UI elements | ui.rs — add to appropriate `render_*` method |
| New mouse interactions | mouse.rs for hit-testing, input.rs `handle_mouse()` for behavior |

## Patterns to Follow

- **Async communication:** Send `AppEvent` through `event_tx` channel, process in `apply_event()`. Never mutate app state from background threads directly.
- **Status polling:** Throttled with `last_status_poll` + mtime checks in `poll_status_files()`. Only re-reads file if mtime changed.
- **Session lookup by ID:** `session_id_map.get(&id)` → index, then `sessions.get(idx)` with `session.id == id` verification (guards against stale map entries).
- **New session creation:** Always ends with `rebuild_session_id_map()` + updating `list_state` selection to the new visible index.
- **PTY reader thread:** OS thread (not tokio), 4096-byte read buffer, sends `PtyOutput(id, bytes)` or `PtyExited(id)`.
- **Scrollback pattern:** Temporarily set via `set_scrollback(offset)`, always reset to 0 after use. See `render_right_panel`, `copy_selection_to_clipboard`, `last_n_lines`, `max_scrollback`.

## Common Pitfalls

- **Don't use `tokio::spawn` for PTY reading.** It blocks on I/O and will starve the tokio runtime. Use `std::thread::spawn`.
- **Don't forget `rebuild_session_id_map()`.** After any push/remove on `sessions`, the ID→index map is stale.
- **Don't resize only the selected session.** Call `resize_all_sessions()` so background sessions track the panel size.
- **Don't leave scrollback set.** After any `set_scrollback(n)` where n > 0, always reset to `set_scrollback(0)`.
- **Don't index `sessions` with list selection directly.** `list_state.selected()` is an index into the visible list. Use `selected_real_index()` or `visible_sessions()[sel]`.
- **Session IDs are monotonic and never reused.** `next_session_id` only increments. Vec indices shift on removal, but IDs are stable.
- **Don't forget to handle both `InputMode` and `Focus` axes.** Input routing depends on both: `handle_key` checks `input_mode` first, then dispatches by `focus` + `left_tab`.

## Key Constants

| Constant | Value | Location |
|----------|-------|----------|
| `IDLE_THRESHOLD` | 1.5s | session.rs:7 |
| `STATUS_POLL_INTERVAL` | 2s | session.rs:8 |
| `MAX_PTY_EVENTS_PER_FRAME` | 500 | session.rs:9 |
| `SESSION_ITEM_HEIGHT` | 2 lines | session.rs:10 |
| `SUMMARY_FORCE_INTERVAL` | 30s | app.rs:518 |
| Summary diff threshold | 30% | app.rs:544 |
| vt100 scrollback buffer | 1000 lines | app.rs:208 |
| Left panel default width | 42 cols | app.rs:135 |
| PTY read buffer | 4096 bytes | app.rs:327 |
| Render tick interval | 33ms (~30fps) | main.rs:95 |
