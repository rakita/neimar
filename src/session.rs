use crate::types::{
    CliType, ClaudeStatus, PermissionMode, SessionState, SessionStatus, IDLE_THRESHOLD,
};
use portable_pty::{Child, MasterPty, PtySize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

pub(crate) struct Session {
    pub(crate) id: usize,
    pub(crate) name: String,
    pub(crate) cli_type: CliType,
    pub(crate) status: SessionStatus,
    #[allow(dead_code)]
    pub(crate) created_at: Instant,
    parser: vt100::Parser,
    pty_master: Option<Box<dyn MasterPty + Send>>,
    pty_writer: Option<Box<dyn Write + Send>>,
    #[allow(dead_code)]
    child: Option<Box<dyn Child + Send>>,
    last_size: (u16, u16),
    status_file: PathBuf,
    pub(crate) claude_status: Option<ClaudeStatus>,
    status_file_mtime: Option<SystemTime>,
    last_pty_output: Option<Instant>,
    pub(crate) scroll_offset: usize,
    pub(crate) turn_count: u32,
    #[allow(dead_code)]
    pub(crate) ralph_loop: bool,
    pending_ralph_command: Option<String>,
    ralph_created_at: Option<Instant>,
    pub(crate) permission_mode: PermissionMode,
    transcript_mtime: Option<SystemTime>,
}

impl Session {
    /// Create a new session with all fields.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        id: usize,
        name: String,
        cli_type: CliType,
        status: SessionStatus,
        parser: vt100::Parser,
        pty_master: Option<Box<dyn MasterPty + Send>>,
        pty_writer: Option<Box<dyn Write + Send>>,
        child: Option<Box<dyn Child + Send>>,
        last_size: (u16, u16),
        status_file: PathBuf,
        is_ralph: bool,
    ) -> Self {
        let has_pty = pty_writer.is_some();
        Self {
            id,
            name,
            cli_type,
            status,
            created_at: Instant::now(),
            parser,
            pty_master,
            pty_writer,
            child,
            last_size,
            status_file,
            claude_status: None,
            status_file_mtime: None,
            last_pty_output: if has_pty { Some(Instant::now()) } else { None },
            scroll_offset: 0,
            turn_count: 0,
            ralph_loop: is_ralph,
            pending_ralph_command: None,
            ralph_created_at: None,
            permission_mode: PermissionMode::Unknown,
            transcript_mtime: None,
        }
    }

    /// Create a failed session (no PTY).
    pub(crate) fn new_failed(
        id: usize,
        name: String,
        cli_type: CliType,
        parser: vt100::Parser,
        last_size: (u16, u16),
        status_file: PathBuf,
        is_ralph: bool,
    ) -> Self {
        Self::new(
            id,
            name,
            cli_type,
            SessionStatus::Failed,
            parser,
            None,
            None,
            None,
            last_size,
            status_file,
            is_ralph,
        )
    }

    // ── PTY I/O ──────────────────────────────────────────

    /// Process raw PTY output bytes through the vt100 parser.
    pub(crate) fn process_pty_output(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.last_pty_output = Some(Instant::now());
    }

    /// Mark the session as exited/completed.
    pub(crate) fn mark_exited(&mut self) {
        self.status = SessionStatus::Completed;
        self.pty_writer = None;
        self.pending_ralph_command = None;
    }

    /// Write raw bytes to the PTY.
    pub(crate) fn write_to_pty(&mut self, bytes: &[u8]) {
        if let Some(writer) = &mut self.pty_writer {
            let _ = writer.write_all(bytes);
            let _ = writer.flush();
        }
    }

    /// Send Ctrl+C then Ctrl+D for graceful shutdown.
    pub(crate) fn send_shutdown_signals(&mut self) {
        if self.status == SessionStatus::Running {
            self.write_to_pty(b"\x03"); // Ctrl+C
            self.write_to_pty(b"\x04"); // Ctrl+D
        }
    }

    /// Drop PTY writer and master to trigger reader thread exit.
    pub(crate) fn drop_pty(&mut self) {
        self.pty_writer = None;
        self.pty_master = None;
    }

    // ── Screen access ────────────────────────────────────

    /// Get a reference to the vt100 screen.
    pub(crate) fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Set scrollback offset on the vt100 parser.
    pub(crate) fn set_scrollback(&mut self, offset: usize) {
        self.parser.screen_mut().set_scrollback(offset);
    }

    /// Resize the PTY and vt100 parser.
    pub(crate) fn resize(&mut self, rows: u16, cols: u16) {
        if (rows, cols) != self.last_size {
            self.parser.screen_mut().set_size(rows, cols);
            if let Some(master) = &self.pty_master {
                let _ = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                });
            }
            self.last_size = (rows, cols);
        }
    }

    // ── Scrolling ────────────────────────────────────────

    pub(crate) fn max_scrollback(&mut self) -> usize {
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let max = self.parser.screen().scrollback();
        self.parser.screen_mut().set_scrollback(0);
        max
    }

    pub(crate) fn clamp_scroll(&mut self) {
        let max = self.max_scrollback();
        self.scroll_offset = self.scroll_offset.min(max);
    }

    // ── State inference ──────────────────────────────────

    pub(crate) fn is_actively_working(&self) -> bool {
        match self.last_pty_output {
            Some(t) => t.elapsed() < IDLE_THRESHOLD,
            None => false,
        }
    }

    pub(crate) fn is_showing_plan_prompt(&self) -> bool {
        let screen = self.parser.screen();
        let contents = screen.contents();
        contents.contains("Claude has written up a plan and is ready to execute. Would you like to proceed?")
    }

    pub(crate) fn is_waiting_for_input(&self) -> bool {
        let screen = self.parser.screen();
        let contents = screen.contents();

        // Check for AskUserQuestion picker
        if contents.contains("Enter to select") && contents.contains("to navigate") {
            return true;
        }

        // Check for permission prompt
        if contents.contains("Allow Claude") || contents.contains("Allow Amp") {
            return true;
        }

        // Check for regular REPL prompt: last non-empty line ends with '>'
        contents
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim_end().ends_with('>'))
    }

    pub(crate) fn inferred_state(&self) -> SessionState {
        match self.status {
            SessionStatus::Completed => SessionState::Closed,
            SessionStatus::Failed => SessionState::Failed,
            SessionStatus::Running => {
                if self.is_actively_working() {
                    SessionState::Working
                } else if self.last_pty_output.is_none() {
                    SessionState::Starting
                } else if self.cli_type != CliType::Console && self.is_showing_plan_prompt() {
                    SessionState::Planned
                } else if self.cli_type != CliType::Console && self.is_waiting_for_input() {
                    SessionState::Input
                } else {
                    SessionState::Done
                }
            }
        }
    }

    // ── Status file polling ──────────────────────────────

    /// Path to the status file for this session.
    pub(crate) fn status_file_path(&self) -> &Path {
        &self.status_file
    }

    /// Check if status file has been updated and deserialize new status.
    pub(crate) fn poll_status_file(&mut self) -> bool {
        if self.status_file.as_os_str().is_empty() {
            return false;
        }
        let meta = match std::fs::metadata(&self.status_file) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let mtime = meta.modified().ok();
        if mtime == self.status_file_mtime {
            return false;
        }
        if let Ok(contents) = std::fs::read_to_string(&self.status_file)
            && let Ok(status) = serde_json::from_str::<ClaudeStatus>(&contents)
        {
            self.claude_status = Some(status);
            self.turn_count += 1;
            self.status_file_mtime = mtime;
            true
        } else {
            false
        }
    }

    /// Poll transcript file for permission mode changes.
    pub(crate) fn poll_transcript(&mut self) {
        use std::io::{Read as _, Seek, SeekFrom};

        let transcript_path = match &self.claude_status {
            Some(cs) if !cs.transcript_path.is_empty() => cs.transcript_path.clone(),
            _ => return,
        };

        let meta = match std::fs::metadata(&transcript_path) {
            Ok(m) => m,
            Err(_) => return,
        };
        let mtime = meta.modified().ok();
        if mtime == self.transcript_mtime {
            return;
        }
        self.transcript_mtime = mtime;

        // Read last ~8KB of transcript for efficiency
        let mut file = match std::fs::File::open(&transcript_path) {
            Ok(f) => f,
            Err(_) => return,
        };
        let file_len = meta.len();
        let tail_size: u64 = 8192;
        let mut buf = Vec::new();
        if file_len > tail_size {
            let _ = file.seek(SeekFrom::End(-(tail_size as i64)));
        }
        if file.read_to_end(&mut buf).is_err() {
            return;
        }
        let text = String::from_utf8_lossy(&buf);

        // Scan lines in reverse for last permissionMode value
        for line in text.lines().rev() {
            if let Some(mode) = crate::types::extract_permission_mode(line) {
                self.permission_mode = mode;
                break;
            }
        }
    }

    // ── Ralph command injection ──────────────────────────

    /// Set pending ralph command to inject after prompt appears.
    pub(crate) fn set_pending_ralph(&mut self, command: String) {
        self.pending_ralph_command = Some(command);
        self.ralph_created_at = Some(Instant::now());
    }

    /// Try to inject the pending ralph command if the prompt is ready.
    pub(crate) fn try_inject_ralph_command(&mut self, min_wait: Duration, max_wait: Duration) {
        let Some(ref cmd) = self.pending_ralph_command else {
            return;
        };
        if self.status != SessionStatus::Running {
            self.pending_ralph_command = None;
            return;
        }
        let Some(created) = self.ralph_created_at else {
            return;
        };
        let elapsed = created.elapsed();
        if elapsed < min_wait {
            return;
        }

        // Check if Claude's REPL prompt is visible (last non-empty line ends with '>')
        let screen = self.parser.screen();
        let contents = screen.contents();
        let prompt_ready = contents
            .lines()
            .rev()
            .find(|line| !line.trim().is_empty())
            .is_some_and(|line| line.trim_end().ends_with('>'));

        if prompt_ready || elapsed >= max_wait {
            let cmd_bytes = format!("{}\r", cmd);
            self.write_to_pty(cmd_bytes.as_bytes());
            self.pending_ralph_command = None;
        }
    }

    // ── Selection text ───────────────────────────────────

    /// Read selected text from the terminal screen.
    pub(crate) fn read_selection_text(
        &mut self,
        start_row: u16,
        start_col: u16,
        end_row: u16,
        end_col: u16,
        scroll_offset: usize,
    ) -> String {
        self.parser.screen_mut().set_scrollback(scroll_offset);
        let screen = self.parser.screen();
        let text = screen.contents_between(
            start_row,
            start_col,
            end_row,
            end_col.saturating_add(1),
        );
        self.parser.screen_mut().set_scrollback(0);
        text.trim_end().to_string()
    }
}
