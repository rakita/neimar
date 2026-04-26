use ratatui::layout::Rect;
use serde::Deserialize;
use std::time::{Duration, Instant};

// ── Constants ────────────────────────────────────────────

pub(crate) const IDLE_THRESHOLD: Duration = Duration::from_millis(1500);
pub(crate) const STATUS_POLL_INTERVAL: Duration = Duration::from_secs(2);
pub(crate) const MAX_PTY_EVENTS_PER_FRAME: usize = 500;
pub(crate) const SESSION_ITEM_HEIGHT: usize = 1;

// ── Communication ────────────────────────────────────────

pub(crate) enum AppEvent {
    PtyOutput(usize, Vec<u8>),
    PtyExited(usize),
}

// ── CLI Type ─────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum CliType {
    Claude,
    ClaudeDangerous,
    Amp,
    Console,
}

impl CliType {
    pub(crate) fn command(&self) -> String {
        match self {
            CliType::Claude | CliType::ClaudeDangerous => "claude".to_string(),
            CliType::Amp => "amp".to_string(),
            CliType::Console => std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string()),
        }
    }

    pub(crate) fn args(&self) -> Vec<&'static str> {
        match self {
            CliType::ClaudeDangerous => vec!["--dangerously-skip-permissions"],
            _ => vec![],
        }
    }

    pub(crate) fn emoji(&self) -> &'static str {
        match self {
            CliType::Claude => "🤖",
            CliType::ClaudeDangerous => "🤖💥",
            CliType::Amp => "⚡",
            CliType::Console => "🖥️",
        }
    }

    #[allow(dead_code)]
    pub(crate) fn label(&self) -> &'static str {
        match self {
            CliType::Claude => "claude",
            CliType::ClaudeDangerous => "claude danger-accept-permissions",
            CliType::Amp => "amp",
            CliType::Console => "console",
        }
    }
}

// ── Session Status & State ───────────────────────────────

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
            SessionState::Closed => "CLOSED",
            SessionState::Failed => "FAILED",
        }
    }
}

// ── Permission Mode ──────────────────────────────────────

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

// ── Sidebar Items ───────────────────────────────────────

#[derive(Clone, PartialEq)]
pub(crate) enum SidebarItem {
    Label(usize),   // label id
    Session(usize), // session id
}

// ── Focus & Input Mode ───────────────────────────────────

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

    NamingLabel,
    ConfirmQuit,
}

#[derive(PartialEq)]
pub(crate) enum LeftTab {
    Sessions,
    Agents,
}

// ── Data Structs ─────────────────────────────────────────

pub(crate) struct AgentFile {
    pub(crate) name: String,
    pub(crate) content: String,
}

// ── Selection ────────────────────────────────────────────

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

// ── App Sub-structs ──────────────────────────────────────

pub(crate) struct UiState {
    pub(crate) focus: Focus,
    pub(crate) input_mode: InputMode,
    pub(crate) input_buffer: String,
    pub(crate) left_tab: LeftTab,
    pub(crate) selected_cli_type: CliType,
    pub(crate) copied_at: Option<Instant>,
}

pub(crate) struct LayoutCache {
    pub(crate) left_panel_width: u16,
    pub(crate) last_right_panel_size: (u16, u16),
    pub(crate) last_sessions_area: Rect,
    pub(crate) last_right_panel_area: Rect,
    pub(crate) last_right_panel_inner: Rect,
}

pub(crate) struct DraggingSession {
    pub(crate) from_index: usize,
    pub(crate) target_index: usize,
}

pub(crate) struct DragState {
    pub(crate) selection: Option<Selection>,
    pub(crate) dragging_divider: bool,
    pub(crate) dragging_scrollbar: bool,
    pub(crate) dragging_sessions_scrollbar: bool,
    pub(crate) dragging_session: Option<DraggingSession>,
    /// Last left-click position and time, for double-click detection
    pub(crate) last_click: Option<(u16, u16, Instant)>,
}

// ── Claude Status (from statusline) ──────────────────────

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

// ── Helper ───────────────────────────────────────────────

pub(crate) fn extract_permission_mode(line: &str) -> Option<PermissionMode> {
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
