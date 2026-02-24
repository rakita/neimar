use portable_pty::{Child, MasterPty};
use serde::Deserialize;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

pub(crate) const IDLE_THRESHOLD: Duration = Duration::from_millis(1500);
pub(crate) const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const MAX_PTY_EVENTS_PER_FRAME: usize = 500;
pub(crate) const SESSION_ITEM_HEIGHT: usize = 3;

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

    pub(crate) fn color(&self) -> ratatui::style::Color {
        use ratatui::style::Color;
        match self {
            PermissionMode::Plan => Color::Magenta,
            PermissionMode::AcceptEdits => Color::Green,
            PermissionMode::Unknown => Color::DarkGray,
        }
    }
}

// ── CLI Type ────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CliType {
    Claude,
    Amp,
}

impl CliType {
    pub(crate) fn command(&self) -> &'static str {
        match self {
            CliType::Claude => "claude",
            CliType::Amp => "amp",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label(&self) -> &'static str {
        match self {
            CliType::Claude => "claude",
            CliType::Amp => "amp",
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
    Archived,
}

impl SessionState {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            SessionState::Working => "WORK",
            SessionState::Prompt => "WAIT",
            SessionState::Starting => "INIT",
            SessionState::Done => "DONE",
            SessionState::Failed => "FAIL",
            SessionState::Archived => "ARCH",
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
            SessionState::Archived => Color::DarkGray,
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
    pub(crate) archived: bool,
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
    pub(crate) summary: Option<String>,
    pub(crate) last_summary_hash: u64,
    pub(crate) summary_pending: bool,
}

impl Session {
    pub(crate) fn is_actively_working(&self) -> bool {
        match self.last_pty_output {
            Some(t) => t.elapsed() < IDLE_THRESHOLD,
            None => false,
        }
    }

    pub(crate) fn inferred_state(&self) -> SessionState {
        if self.archived {
            return SessionState::Archived;
        }
        match self.status {
            SessionStatus::Completed => SessionState::Done,
            SessionStatus::Failed => SessionState::Failed,
            SessionStatus::Running => {
                if self.is_actively_working() {
                    SessionState::Working
                } else if self.last_pty_output.is_some() {
                    SessionState::Prompt
                } else {
                    SessionState::Starting
                }
            }
        }
    }

    pub(crate) fn context_bar(&self, width: usize) -> (String, ratatui::style::Color) {
        let pct = self
            .claude_status
            .as_ref()
            .map(|cs| cs.context_window.used_percentage)
            .unwrap_or(0.0);
        let filled = ((pct / 100.0) * width as f64).round() as usize;
        let filled = filled.min(width);
        let empty = width - filled;
        let bar = format!("{}{}", "▰".repeat(filled), "▱".repeat(empty));
        let color = if pct >= 80.0 {
            ratatui::style::Color::Red
        } else if pct >= 60.0 {
            ratatui::style::Color::Yellow
        } else {
            ratatui::style::Color::Green
        };
        (bar, color)
    }

    pub(crate) fn lines_changed_display(&self) -> Option<String> {
        self.claude_status.as_ref().and_then(|cs| {
            if cs.cost.total_lines_added > 0 || cs.cost.total_lines_removed > 0 {
                Some(format!(
                    "+{}/-{}",
                    cs.cost.total_lines_added, cs.cost.total_lines_removed
                ))
            } else {
                None
            }
        })
    }

    fn is_marker_line(s: &str) -> bool {
        let c = match s.chars().next() {
            Some(c) => c,
            None => return false,
        };
        c == '⏺'
    }

    fn is_dim_or_gray(cell: &vt100::Cell) -> bool {
        if cell.dim() {
            return true;
        }
        match cell.fgcolor() {
            vt100::Color::Default => false,
            vt100::Color::Idx(idx) => matches!(idx, 0 | 8),
            vt100::Color::Rgb(r, g, b) => {
                let max = r.max(g).max(b);
                let min = r.min(g).min(b);
                (max - min) < 30 && max < 180
            }
        }
    }

    fn is_bright_marker_row(screen: &vt100::Screen, row: u16) -> bool {
        let (_, cols) = screen.size();
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                break;
            };
            let contents = cell.contents();
            if contents.is_empty() || contents == " " {
                continue;
            }
            if contents.starts_with('⏺') {
                return !Self::is_dim_or_gray(cell);
            }
            return false;
        }
        false
    }

    pub(crate) fn last_output_lines(&self, n: usize) -> Vec<String> {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let all_rows: Vec<String> = screen
            .rows(0, cols)
            .take(rows as usize)
            .collect();
        // Find the last line starting with a bright marker, then collect up to n lines from there
        let mut start = None;
        for (i, line) in all_rows.iter().enumerate().rev() {
            let trimmed = line.trim();
            if Self::is_marker_line(trimmed) && Self::is_bright_marker_row(screen, i as u16) {
                start = Some(i);
                break;
            }
        }
        let Some(start) = start else {
            return Vec::new();
        };
        let mut result = Vec::with_capacity(n);
        for line in &all_rows[start..] {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            result.push(trimmed.to_string());
            if result.len() >= n {
                break;
            }
        }
        result
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
