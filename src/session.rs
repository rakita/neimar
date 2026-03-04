use portable_pty::{Child, MasterPty};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub(crate) const IDLE_THRESHOLD: Duration = Duration::from_millis(1500);
pub(crate) const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const MAX_PTY_EVENTS_PER_FRAME: usize = 500;
pub(crate) const SESSION_ITEM_HEIGHT: usize = 1;

// ── Claude Status (from statusline) ─────────────────────

#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ClaudeStatusModel {
    #[serde(default)]
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) display_name: String,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ClaudeStatusCost {
    #[serde(default)]
    pub(crate) total_cost_usd: f64,
    #[serde(default)]
    pub(crate) total_duration_ms: f64,
    #[serde(default)]
    pub(crate) total_lines_added: u64,
    #[serde(default)]
    pub(crate) total_lines_removed: u64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ClaudeStatusCurrentUsage {
    #[serde(default)]
    pub(crate) input_tokens: u64,
    #[serde(default)]
    pub(crate) output_tokens: u64,
    #[serde(default)]
    pub(crate) cache_creation_input_tokens: u64,
    #[serde(default)]
    pub(crate) cache_read_input_tokens: u64,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ClaudeStatusContext {
    #[serde(default)]
    pub(crate) used_percentage: f64,
    #[serde(default)]
    pub(crate) context_window_size: u64,
    #[serde(default)]
    pub(crate) remaining_percentage: f64,
    #[serde(default)]
    pub(crate) total_input_tokens: u64,
    #[serde(default)]
    pub(crate) total_output_tokens: u64,
    #[serde(default)]
    pub(crate) current_usage: ClaudeStatusCurrentUsage,
}

#[derive(Deserialize, Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct ClaudeStatus {
    #[serde(default)]
    pub(crate) model: ClaudeStatusModel,
    #[serde(default)]
    pub(crate) cost: ClaudeStatusCost,
    #[serde(default)]
    pub(crate) context_window: ClaudeStatusContext,
    #[serde(default)]
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) exceeds_200k_tokens: bool,
    #[serde(default)]
    pub(crate) transcript_path: String,
}

// ── Permission Mode ─────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PermissionMode {
    Plan,
    AcceptEdits,
    Unknown,
}

impl PermissionMode {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            PermissionMode::Plan => "PLAN",
            PermissionMode::AcceptEdits => "EDIT",
            PermissionMode::Unknown => "",
        }
    }

    pub(crate) fn emoji(&self) -> &'static str {
        match self {
            PermissionMode::Plan => "⏸",
            PermissionMode::AcceptEdits => "⏩",
            PermissionMode::Unknown => "",
        }
    }
}

// ── CLI Type ────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CliType {
    Claude,
    Amp,
    Console,
}

impl CliType {
    pub(crate) fn command(&self) -> String {
        match self {
            CliType::Claude => "claude".to_string(),
            CliType::Amp => "amp".to_string(),
            CliType::Console => std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()),
        }
    }

    pub(crate) fn emoji(&self) -> &'static str {
        match self {
            CliType::Claude => "🤖",
            CliType::Amp => "⚡",
            CliType::Console => "🖥️",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label(&self) -> &'static str {
        match self {
            CliType::Claude => "claude",
            CliType::Amp => "amp",
            CliType::Console => "console",
        }
    }
}

// ── Session ─────────────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(crate) enum SessionStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Clone, PartialEq)]
pub(crate) enum SessionState {
    Working,
    Input,
    Planned,
    Done,
    Starting,
    Closed,
    Failed,
}

impl SessionState {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            SessionState::Working => "🧱",
            SessionState::Input => "💬",
            SessionState::Planned => "📋",
            SessionState::Done => "🟢",
            SessionState::Starting => "⏳",
            SessionState::Closed => "🔒",
            SessionState::Failed => "🔴",
        }
    }

    pub(crate) fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            SessionState::Working => Color::Yellow,
            SessionState::Input => Color::Cyan,
            SessionState::Planned => Color::Magenta,
            SessionState::Done => Color::Green,
            SessionState::Starting => Color::DarkGray,
            SessionState::Closed => Color::DarkGray,
            SessionState::Failed => Color::Red,
        }
    }

    pub(crate) fn text_label(&self) -> &'static str {
        match self {
            SessionState::Working => "WORKING",
            SessionState::Input => "INPUT",
            SessionState::Planned => "PLANNED",
            SessionState::Done => "DONE",
            SessionState::Starting => "STARTING",
            SessionState::Closed => "CLOSED",
            SessionState::Failed => "FAILED",
        }
    }
}

pub(crate) struct Session {
    pub(crate) id: usize,
    pub(crate) name: String,
    #[allow(dead_code)]
    pub(crate) cli_type: CliType,
    pub(crate) status: SessionStatus,
    #[allow(dead_code)]
    pub(crate) created_at: Instant,
    pub(crate) parser: vt100::Parser,
    pub(crate) pty_master: Option<Box<dyn MasterPty + Send>>,
    pub(crate) pty_writer: Option<Box<dyn Write + Send>>,
    #[allow(dead_code)]
    pub(crate) child: Option<Box<dyn Child + Send>>,
    pub(crate) last_size: (u16, u16),
    pub(crate) status_file: PathBuf,
    pub(crate) claude_status: Option<ClaudeStatus>,
    pub(crate) status_file_mtime: Option<SystemTime>,
    pub(crate) last_pty_output: Option<Instant>,
    pub(crate) scroll_offset: usize,
    pub(crate) turn_count: u32,
    #[allow(dead_code)]
    pub(crate) ralph_loop: bool,
    pub(crate) pending_ralph_command: Option<String>,
    pub(crate) ralph_created_at: Option<Instant>,
    pub(crate) permission_mode: PermissionMode,
    pub(crate) transcript_mtime: Option<SystemTime>,
}

impl Session {
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
}
