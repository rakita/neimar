use crate::event::AppEvent;
use crate::session::{
    ClaudeStatus, CliType, PermissionMode, STATUS_POLL_INTERVAL, Session, SessionStatus,
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::io::{Read as _, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// ── Focus & Input Mode ──────────────────────────────────

#[derive(PartialEq)]
pub(crate) enum Focus {
    Sessions,
    Terminal,
}

pub(crate) enum InputMode {
    Normal,
    NamingSession,
    RenamingSession,
    SelectingSessionType,
    NamingRalph,
    EnteringRalphPrompt,
}

#[derive(PartialEq)]
pub(crate) enum LeftTab {
    Sessions,
    Agents,
}

pub(crate) struct AgentFile {
    pub(crate) name: String,
    pub(crate) content: String,
}

pub(crate) struct RalphConfig {
    pub(crate) prompt: String,
    pub(crate) max_iterations: usize,
    pub(crate) completion_promise: String,
}

const RALPH_MIN_WAIT: Duration = Duration::from_secs(3);
const RALPH_MAX_WAIT: Duration = Duration::from_secs(10);

// ── Selection ───────────────────────────────────────────

pub(crate) struct Selection {
    /// Anchor point (where the drag started) in vt100 screen coords
    pub(crate) anchor_row: u16,
    pub(crate) anchor_col: u16,
    /// Current end point of the drag in vt100 screen coords
    pub(crate) end_row: u16,
    pub(crate) end_col: u16,
    /// Scroll offset at the time the selection started
    pub(crate) scroll_offset: usize,
}

impl Selection {
    /// Returns (start_row, start_col, end_row, end_col) in normalized order (start <= end).
    pub(crate) fn ordered(&self) -> (u16, u16, u16, u16) {
        if (self.anchor_row, self.anchor_col) <= (self.end_row, self.end_col) {
            (self.anchor_row, self.anchor_col, self.end_row, self.end_col)
        } else {
            (self.end_row, self.end_col, self.anchor_row, self.anchor_col)
        }
    }
}

// ── App ─────────────────────────────────────────────────

pub(crate) struct App {
    pub(crate) sessions: Vec<Session>,
    pub(crate) session_id_map: HashMap<usize, usize>,
    pub(crate) list_state: ListState,
    pub(crate) focus: Focus,
    pub(crate) input_mode: InputMode,
    pub(crate) input_buffer: String,
    pub(crate) event_tx: mpsc::UnboundedSender<AppEvent>,
    pub(crate) should_quit: bool,
    pub(crate) next_session_id: usize,
    pub(crate) show_archived: bool,
    pub(crate) last_status_poll: Instant,
    pub(crate) last_right_panel_size: (u16, u16),
    pub(crate) last_sessions_area: Rect,
    pub(crate) last_right_panel_area: Rect,
    pub(crate) left_tab: LeftTab,
    pub(crate) agents: Vec<AgentFile>,
    pub(crate) agent_list_state: ListState,
    pub(crate) agent_scroll_offset: u16,
    pub(crate) pending_session_name: Option<String>,
    pub(crate) selected_cli_type: CliType,
    pub(crate) selection: Option<Selection>,
    pub(crate) last_right_panel_inner: Rect,
    pub(crate) left_panel_width: u16,
    pub(crate) dragging_divider: bool,
    pub(crate) left_panel_half: bool,
}

impl App {
    pub(crate) fn new(event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let agents = Self::load_agents();
        let mut agent_list_state = ListState::default();
        if !agents.is_empty() {
            agent_list_state.select(Some(0));
        }
        Self {
            sessions: Vec::new(),
            session_id_map: HashMap::new(),
            list_state: ListState::default(),
            focus: Focus::Sessions,
            input_mode: InputMode::Normal,
            input_buffer: String::new(),
            event_tx,
            should_quit: false,
            next_session_id: 0,
            show_archived: false,
            last_status_poll: Instant::now(),
            last_right_panel_size: (0, 0),
            last_sessions_area: Rect::default(),
            last_right_panel_area: Rect::default(),
            left_tab: LeftTab::Sessions,
            agents,
            agent_list_state,
            agent_scroll_offset: 0,
            pending_session_name: None,
            selected_cli_type: CliType::Claude,
            selection: None,
            last_right_panel_inner: Rect::default(),
            left_panel_width: 42,
            dragging_divider: false,
            left_panel_half: false,
        }
    }

    fn load_agents() -> Vec<AgentFile> {
        let agents_dir = std::env::current_dir().unwrap_or_default().join("agents");
        let mut agents = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && let Ok(content) = std::fs::read_to_string(&path) {
                    let name = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    agents.push(AgentFile { name, content });
                }
            }
        }
        agents.sort_by(|a, b| a.name.cmp(&b.name));
        agents
    }

    pub(crate) fn rebuild_session_id_map(&mut self) {
        self.session_id_map.clear();
        for (idx, session) in self.sessions.iter().enumerate() {
            self.session_id_map.insert(session.id, idx);
        }
    }

    pub(crate) fn create_session(&mut self, name: String, cli_type: CliType, rows: u16, cols: u16) {
        self.create_session_inner(name, cli_type, rows, cols, false);
    }

    pub(crate) fn create_ralph_session(
        &mut self,
        name: String,
        rows: u16,
        cols: u16,
        config: RalphConfig,
    ) {
        self.create_session_inner(name, CliType::Claude, rows, cols, true);

        // Build the ralph command to inject later
        let escaped_prompt = config
            .prompt
            .replace('"', "\"\"")
            .replace('\n', " ");
        let ralph_cmd = format!(
            "/ralph-loop {} --max-iterations {} --completion-promise \"{}\"",
            escaped_prompt, config.max_iterations, config.completion_promise
        );

        // Store pending command on the just-created session
        if let Some(session) = self.sessions.last_mut() {
            session.pending_ralph_command = Some(ralph_cmd);
            session.ralph_created_at = Some(Instant::now());
        }
    }

    fn create_session_inner(
        &mut self,
        name: String,
        cli_type: CliType,
        rows: u16,
        cols: u16,
        is_ralph: bool,
    ) -> usize {
        let id = self.next_session_id;
        self.next_session_id += 1;
        let parser = vt100::Parser::new(rows, cols, 1000);

        let pty_system = native_pty_system();
        let pty_size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = match pty_system.openpty(pty_size) {
            Ok(pair) => pair,
            Err(e) => {
                let mut session = Session {
                    id,
                    name,
                    cli_type,
                    status: SessionStatus::Failed,
                    created_at: Instant::now(),
                    parser,
                    pty_master: None,
                    pty_writer: None,
                    child: None,
                    last_size: (rows, cols),
                    archived: false,
                    status_file: PathBuf::new(),
                    claude_status: None,
                    status_file_mtime: None,
                    last_pty_output: None,
                    scroll_offset: 0,
                    turn_count: 0,
                    ralph_loop: is_ralph,
                    pending_ralph_command: None,
                    ralph_created_at: None,
                    permission_mode: PermissionMode::Unknown,
                    transcript_mtime: None,
                    ai_state: None,
                    summary: None,
                    last_summary_text: String::new(),
                    summary_pending: false,
                    last_summary_at: None,
                };
                session
                    .parser
                    .process(format!("Failed to open PTY: {}\r\n", e).as_bytes());
                self.sessions.push(session);
                self.rebuild_session_id_map();
                let vis = self.visible_sessions();
                let new_real_idx = self.sessions.len() - 1;
                let vis_idx = vis.iter().position(|&i| i == new_real_idx)
                    .unwrap_or(vis.len().saturating_sub(1));
                self.list_state.select(Some(vis_idx));
                return id;
            }
        };

        let status_path =
            PathBuf::from(format!("/tmp/neimar-{}-status-{}", std::process::id(), id));

        // Ralph sessions spawn interactive claude (no -p), regular sessions use the cli_type command
        let mut cmd = CommandBuilder::new(cli_type.command());
        cmd.env("TERM", "xterm-256color");
        cmd.env("NEIMAR_STATUS_FILE", status_path.to_str().unwrap());
        cmd.cwd(std::env::current_dir().unwrap());

        let child = match pair.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(e) => {
                let mut session = Session {
                    id,
                    name,
                    cli_type,
                    status: SessionStatus::Failed,
                    created_at: Instant::now(),
                    parser,
                    pty_master: None,
                    pty_writer: None,
                    child: None,
                    last_size: (rows, cols),
                    archived: false,
                    status_file: status_path,
                    claude_status: None,
                    status_file_mtime: None,
                    last_pty_output: None,
                    scroll_offset: 0,
                    turn_count: 0,
                    ralph_loop: is_ralph,
                    pending_ralph_command: None,
                    ralph_created_at: None,
                    permission_mode: PermissionMode::Unknown,
                    transcript_mtime: None,
                    ai_state: None,
                    summary: None,
                    last_summary_text: String::new(),
                    summary_pending: false,
                    last_summary_at: None,
                };
                session
                    .parser
                    .process(format!("Failed to spawn {}: {}\r\n", cli_type.command(), e).as_bytes());
                self.sessions.push(session);
                self.rebuild_session_id_map();
                let vis = self.visible_sessions();
                let new_real_idx = self.sessions.len() - 1;
                let vis_idx = vis.iter().position(|&i| i == new_real_idx)
                    .unwrap_or(vis.len().saturating_sub(1));
                self.list_state.select(Some(vis_idx));
                return id;
            }
        };
        // Drop the slave so the PTY gets EOF when the child exits
        drop(pair.slave);

        let reader = pair.master.try_clone_reader().unwrap();
        let writer = pair.master.take_writer().unwrap();

        let tx = self.event_tx.clone();
        std::thread::spawn(move || {
            let mut reader = reader;
            let mut buf = [0u8; 4096];
            loop {
                match std::io::Read::read(&mut reader, &mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = tx.send(AppEvent::PtyExited(id));
                        break;
                    }
                    Ok(n) => {
                        let _ = tx.send(AppEvent::PtyOutput(id, buf[..n].to_vec()));
                    }
                }
            }
        });

        let session = Session {
            id,
            name,
            cli_type,
            status: SessionStatus::Running,
            created_at: Instant::now(),
            parser,
            pty_master: Some(pair.master),
            pty_writer: Some(writer),
            child: Some(child),
            last_size: (rows, cols),
            archived: false,
            status_file: status_path,
            claude_status: None,
            status_file_mtime: None,
            last_pty_output: Some(Instant::now()),
            scroll_offset: 0,
            turn_count: 0,
            ralph_loop: is_ralph,
            pending_ralph_command: None,
            ralph_created_at: None,
            permission_mode: PermissionMode::Unknown,
            transcript_mtime: None,
            ai_state: None,
            summary: None,
            last_summary_text: String::new(),
            summary_pending: false,
            last_summary_at: None,
        };

        self.sessions.push(session);
        self.rebuild_session_id_map();
        let vis = self.visible_sessions();
        let new_real_idx = self.sessions.len() - 1;
        let vis_idx = vis.iter().position(|&i| i == new_real_idx)
            .unwrap_or(vis.len().saturating_sub(1));
        self.list_state.select(Some(vis_idx));
        self.focus = Focus::Terminal;
        id
    }

    /// Returns real indices into `self.sessions` for sessions that should be displayed.
    pub(crate) fn visible_sessions(&self) -> Vec<usize> {
        let mut vis: Vec<usize> = self
            .sessions
            .iter()
            .enumerate()
            .filter(|(_, s)| !s.archived)
            .map(|(i, _)| i)
            .collect();
        if self.show_archived {
            vis.extend(
                self.sessions
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.archived)
                    .map(|(i, _)| i),
            );
        }
        vis
    }

    /// Map the current visible selection to a real session index.
    pub(crate) fn selected_real_index(&self) -> Option<usize> {
        let vis = self.visible_sessions();
        self.list_state.selected().and_then(|i| vis.get(i).copied())
    }

    pub(crate) fn selected_session(&self) -> Option<&Session> {
        self.selected_real_index()
            .and_then(|i| self.sessions.get(i))
    }

    pub(crate) fn selected_session_mut(&mut self) -> Option<&mut Session> {
        let idx = self.selected_real_index();
        idx.and_then(|i| self.sessions.get_mut(i))
    }

    pub(crate) fn cleanup_status_file(path: &PathBuf) {
        if !path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(path);
        }
    }

    pub(crate) fn cleanup_all_status_files(&self) {
        for session in &self.sessions {
            Self::cleanup_status_file(&session.status_file);
        }
    }

    pub(crate) fn update_pty_permission_modes(&mut self) {
        for session in &mut self.sessions {
            if session.status != SessionStatus::Running || session.archived {
                continue;
            }
            let detected = session.detect_permission_mode_from_pty();
            if detected != PermissionMode::Unknown {
                session.permission_mode = detected;
            }
        }
    }

    pub(crate) fn poll_status_files(&mut self) {
        if self.last_status_poll.elapsed() < STATUS_POLL_INTERVAL {
            return;
        }
        self.last_status_poll = Instant::now();

        for session in &mut self.sessions {
            if session.status_file.as_os_str().is_empty() {
                continue;
            }
            let meta = match std::fs::metadata(&session.status_file) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().ok();
            if mtime == session.status_file_mtime {
                continue;
            }
            if let Ok(contents) = std::fs::read_to_string(&session.status_file)
                && let Ok(status) = serde_json::from_str::<ClaudeStatus>(&contents)
            {
                session.claude_status = Some(status);
                session.turn_count += 1;
            }
            session.status_file_mtime = mtime;
        }

        // Poll transcript files for permission mode
        for session in &mut self.sessions {
            let transcript_path = match &session.claude_status {
                Some(cs) if !cs.transcript_path.is_empty() => cs.transcript_path.clone(),
                _ => continue,
            };

            let meta = match std::fs::metadata(&transcript_path) {
                Ok(m) => m,
                Err(_) => continue,
            };
            let mtime = meta.modified().ok();
            if mtime == session.transcript_mtime {
                continue;
            }
            session.transcript_mtime = mtime;

            // Read last ~8KB of transcript for efficiency
            let mut file = match std::fs::File::open(&transcript_path) {
                Ok(f) => f,
                Err(_) => continue,
            };
            let file_len = meta.len();
            let tail_size: u64 = 8192;
            let mut buf = Vec::new();
            if file_len > tail_size {
                let _ = file.seek(SeekFrom::End(-(tail_size as i64)));
            }
            if file.read_to_end(&mut buf).is_err() {
                continue;
            }
            let text = String::from_utf8_lossy(&buf);

            // Scan lines in reverse for last permissionMode value
            for line in text.lines().rev() {
                if let Some(mode) = extract_permission_mode(line) {
                    session.permission_mode = mode;
                    break;
                }
            }
        }

        self.poll_summaries();
    }

    // ── Summary polling ─────────────────────────────────

    pub(crate) fn poll_summaries(&mut self) {
        const SUMMARY_FORCE_INTERVAL: Duration = Duration::from_secs(30);

        for session in &mut self.sessions {
            if session.archived || session.summary_pending {
                continue;
            }

            // Completed sessions: allow one final summary if stale, then skip
            if session.status != SessionStatus::Running {
                match session.last_summary_at {
                    Some(t) if t.elapsed() < SUMMARY_FORCE_INTERVAL => continue,
                    _ => {}
                }
            }

            let mut lines = session.last_n_lines(100);
            if lines.len() <= 3 {
                continue;
            }
            lines.truncate(lines.len() - 1);

            let text = lines.join("\n");

            // Check if enough has changed, with a time-based override
            if !session.last_summary_text.is_empty() {
                let diff_ratio = text_diff_ratio(&session.last_summary_text, &text);
                if diff_ratio <= 0.15 {
                    // Force refresh if last summary is older than 30s
                    match session.last_summary_at {
                        Some(t) if t.elapsed() < SUMMARY_FORCE_INTERVAL => continue,
                        _ => {}
                    }
                }
            }

            session.last_summary_text = text.clone();
            session.summary_pending = true;
            session.last_summary_at = Some(Instant::now());

            let id = session.id;
            let tx = self.event_tx.clone();
            tokio::spawn(async move {
                let result = tokio::process::Command::new("claude")
                    .arg("-p")
                    .arg(format!(
                        "Your PRIMARY task: classify what Claude is doing in this terminal session.\n\
                         Output exactly ONE line: STATE brief_summary\n\n\
                         STATE rules (apply first match):\n\
                         \x20 INPUT   — Claude stopped and needs user response. Signals: question mark, \"Would you like\", \"Should I\", \"Do you want\", \"Please provide\", permission prompt, or choices listed.\n\
                         \x20 WORKING — Claude is actively executing. Signals: tool calls, file edits, command output, reading files, compiling, testing.\n\
                         \x20 DONE    — Claude finished and is idle. Signals: summary of work done, \"I've finished\", \"I've implemented\", \"Complete\", or idle prompt after work.\n\n\
                         brief_summary: 3-5 words. For INPUT: what user must decide/answer. For WORKING: what Claude is doing. For DONE: what was accomplished.\n\n\
                         Examples:\n\
                         \x20 INPUT need database schema decision\n\
                         \x20 WORKING running test suite\n\
                         \x20 DONE auth module implemented\n\n\
                         Terminal output:\n\n{}",
                        text
                    ))
                    .arg("--model")
                    .arg("haiku")
                    .stdin(std::process::Stdio::null())
                    .output()
                    .await;

                let summary = match result {
                    Ok(output) if output.status.success() => {
                        String::from_utf8_lossy(&output.stdout).trim().to_string()
                    }
                    _ => String::new(),
                };
                let _ = tx.send(AppEvent::SummaryResult(id, summary));
            });
        }
    }

    // ── Ralph command injection ──────────────────────────

    pub(crate) fn check_pending_ralph_commands(&mut self) {
        for session in &mut self.sessions {
            let Some(ref cmd) = session.pending_ralph_command else {
                continue;
            };
            if session.status != SessionStatus::Running {
                session.pending_ralph_command = None;
                continue;
            }
            let Some(created) = session.ralph_created_at else {
                continue;
            };
            let elapsed = created.elapsed();
            if elapsed < RALPH_MIN_WAIT {
                continue;
            }

            // Check if Claude's REPL prompt is visible (last non-empty line ends with '>')
            let screen = session.parser.screen();
            let contents = screen.contents();
            let prompt_ready = contents
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .is_some_and(|line| line.trim_end().ends_with('>'));

            if prompt_ready || elapsed >= RALPH_MAX_WAIT {
                let cmd_bytes = format!("{}\r", cmd);
                if let Some(writer) = &mut session.pty_writer {
                    let _ = writer.write_all(cmd_bytes.as_bytes());
                    let _ = writer.flush();
                }
                session.pending_ralph_command = None;
            }
        }
    }

    /// Convert absolute screen (column, row) to vt100 screen-space (row, col),
    /// returning None if the point is outside the inner right panel rect.
    pub(crate) fn screen_coords_from_mouse(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let inner = self.last_right_panel_inner;
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        if column < inner.x || column >= inner.x + inner.width {
            return None;
        }
        if row < inner.y || row >= inner.y + inner.height {
            return None;
        }
        let vt_col = column - inner.x;
        let vt_row = row - inner.y;
        Some((vt_row, vt_col))
    }

    /// Copy the currently selected text to the system clipboard.
    pub(crate) fn copy_selection_to_clipboard(&mut self) {
        let sel = match &self.selection {
            Some(s) => s,
            None => return,
        };
        let idx = match self.selected_real_index() {
            Some(i) => i,
            None => return,
        };

        let (start_row, start_col, end_row, end_col) = sel.ordered();
        let scroll_offset = sel.scroll_offset;

        // Set scrollback to the offset at selection time so we read the right content
        self.sessions[idx]
            .parser
            .screen_mut()
            .set_scrollback(scroll_offset);

        let screen = self.sessions[idx].parser.screen();
        let text = screen.contents_between(
            start_row,
            start_col,
            end_row,
            end_col.saturating_add(1), // end_col is inclusive for the user, but contents_between wants exclusive
        );

        // Reset scrollback
        self.sessions[idx].parser.screen_mut().set_scrollback(0);

        let text = text.trim_end().to_string();
        if text.is_empty() {
            return;
        }

        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        // Send Ctrl+C then Ctrl+D to all running sessions for graceful shutdown
        for session in &mut self.sessions {
            if session.status == SessionStatus::Running
                && let Some(writer) = &mut session.pty_writer
            {
                let _ = writer.write_all(b"\x03"); // Ctrl+C
                let _ = writer.flush();
                let _ = writer.write_all(b"\x04"); // Ctrl+D
                let _ = writer.flush();
            }
        }
        // Drop PTY writers and masters to close the PTY, triggering reader thread exit
        for session in &mut self.sessions {
            session.pty_writer = None;
            session.pty_master = None;
        }
        self.cleanup_all_status_files();
    }
}

fn text_diff_ratio(old: &str, new: &str) -> f64 {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let total = old_lines.len().max(new_lines.len());
    if total == 0 {
        return 0.0;
    }
    let common = old_lines.iter().filter(|l| new_lines.contains(l)).count();
    1.0 - (common as f64 / total as f64)
}

fn extract_permission_mode(line: &str) -> Option<PermissionMode> {
    // Fast substring check — avoid full JSON parse
    if !line.contains("\"permissionMode\"") {
        return None;
    }
    if line.contains("\"permissionMode\":\"plan\"") || line.contains("\"permissionMode\": \"plan\"")
    {
        Some(PermissionMode::Plan)
    } else if line.contains("\"permissionMode\":\"acceptEdits\"")
        || line.contains("\"permissionMode\": \"acceptEdits\"")
    {
        Some(PermissionMode::AcceptEdits)
    } else {
        None
    }
}
