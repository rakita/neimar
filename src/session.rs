use portable_pty::{Child, MasterPty};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub(crate) const IDLE_THRESHOLD: Duration = Duration::from_millis(1500);
pub(crate) const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const MAX_PTY_EVENTS_PER_FRAME: usize = 500;
pub(crate) const SESSION_ITEM_HEIGHT: usize = 2;

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

    #[allow(dead_code)]
    pub(crate) fn label(&self) -> &'static str {
        match self {
            CliType::Claude => "claude",
            CliType::Amp => "amp",
            CliType::Console => "console",
        }
    }
}

// ── AI State (from LLM classification) ─────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum AiState {
    Working,
    Input,
    Done,
}

impl AiState {
    pub(crate) fn parse(word: &str) -> Option<Self> {
        match word {
            "WORKING" => Some(AiState::Working),
            "INPUT" => Some(AiState::Input),
            "DONE" => Some(AiState::Done),
            _ => None,
        }
    }

    pub(crate) fn label(&self) -> &'static str {
        match self {
            AiState::Working => "WORKING",
            AiState::Input => "INPUT",
            AiState::Done => "DONE",
        }
    }

    pub(crate) fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            AiState::Working => Color::Yellow,
            AiState::Input => Color::Cyan,
            AiState::Done => Color::Green,
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
    Prompt,
    Starting,
    Done,
    Failed,
}

impl SessionState {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            SessionState::Working => "🧱",
            SessionState::Prompt => "💬",
            SessionState::Starting => "⏳",
            SessionState::Done => "🟢",
            SessionState::Failed => "🔴",
        }
    }

    pub(crate) fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            SessionState::Working => Color::Yellow,
            SessionState::Prompt => Color::Cyan,
            SessionState::Starting => Color::DarkGray,
            SessionState::Done => Color::Green,
            SessionState::Failed => Color::Red,
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
    pub(crate) ralph_loop: bool,
    pub(crate) pending_ralph_command: Option<String>,
    pub(crate) ralph_created_at: Option<Instant>,
    pub(crate) permission_mode: PermissionMode,
    pub(crate) transcript_mtime: Option<SystemTime>,
    pub(crate) ai_state: Option<AiState>,
    pub(crate) summary: Option<String>,
    pub(crate) last_summary_text: String,
    pub(crate) summary_pending: bool,
    pub(crate) last_summary_at: Option<Instant>,
    pub(crate) forced_summary_count: u32,
}

impl Session {
    pub(crate) fn is_actively_working(&self) -> bool {
        match self.last_pty_output {
            Some(t) => t.elapsed() < IDLE_THRESHOLD,
            None => false,
        }
    }

    pub(crate) fn inferred_state(&self) -> SessionState {
        match self.status {
            SessionStatus::Completed => SessionState::Done,
            SessionStatus::Failed => SessionState::Failed,
            SessionStatus::Running => {
                if self.is_actively_working() {
                    return SessionState::Working; // real-time PTY activity = definitively working
                }
                if let Some(ai) = self.ai_state {
                    match ai {
                        AiState::Working => SessionState::Working,
                        AiState::Input => SessionState::Prompt,
                        AiState::Done => SessionState::Done,
                    }
                } else if self.last_pty_output.is_some() {
                    SessionState::Prompt
                } else {
                    SessionState::Starting
                }
            }
        }
    }

    pub(crate) fn last_n_lines(&mut self, n: usize) -> Vec<String> {
        // Enable full scrollback so we can read beyond the visible screen
        self.parser.screen_mut().set_scrollback(usize::MAX);
        let screen = self.parser.screen();
        let scrollback = screen.scrollback();
        let (rows, cols) = screen.size();
        let total_rows = rows as usize + scrollback;
        let all_rows: Vec<String> = screen.rows(0, cols).take(total_rows).collect();
        // Reset scrollback
        self.parser.screen_mut().set_scrollback(0);
        let mut result: Vec<String> = all_rows
            .iter()
            .rev()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            })
            .take(n)
            .collect();
        result.reverse();
        result
    }

    pub(crate) fn detect_permission_mode_from_pty(&mut self) -> PermissionMode {
        let lines = self.last_n_lines(3);
        for line in &lines {
            let lower = line.to_lowercase();
            if lower.contains("plan mode") {
                return PermissionMode::Plan;
            }
            if lower.contains("auto-accept") || lower.contains("accept edits") {
                return PermissionMode::AcceptEdits;
            }
        }
        PermissionMode::Unknown
    }

    pub(crate) fn detect_permission_mode_from_bytes(bytes: &[u8]) -> PermissionMode {
        let text = String::from_utf8_lossy(bytes).to_lowercase();
        if text.contains("plan mode") {
            PermissionMode::Plan
        } else if text.contains("auto-accept") || text.contains("accept edits") {
            PermissionMode::AcceptEdits
        } else {
            PermissionMode::Unknown
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
