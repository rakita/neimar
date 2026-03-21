use crate::session::Session;
use crate::types::{
    AgentFile, AppEvent, CliType, DragState, Focus, InputMode, LayoutCache, LeftTab, RalphConfig,
    SessionStatus, SidebarItem, STATUS_POLL_INTERVAL, UiState,
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use ratatui::layout::Rect;
use ratatui::widgets::ListState;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const RALPH_MIN_WAIT: Duration = Duration::from_secs(3);
const RALPH_MAX_WAIT: Duration = Duration::from_secs(10);

// ── App ─────────────────────────────────────────────────

pub(crate) struct App {
    // Session management
    pub(crate) sessions: Vec<Session>,
    pub(crate) session_id_map: HashMap<usize, usize>,
    pub(crate) list_state: ListState,
    next_session_id: usize,

    // Labels & sidebar ordering
    pub(crate) labels: HashMap<usize, String>,
    pub(crate) sidebar_items: Vec<SidebarItem>,
    next_label_id: usize,

    // Agents
    pub(crate) agents: Vec<AgentFile>,
    pub(crate) agent_list_state: ListState,
    pub(crate) agent_scroll_offset: u16,

    // Grouped state
    pub(crate) ui: UiState,
    pub(crate) layout: LayoutCache,
    pub(crate) drag: DragState,

    // Infrastructure
    event_tx: mpsc::UnboundedSender<AppEvent>,
    pub(crate) should_quit: bool,
    last_status_poll: Instant,
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
            next_session_id: 0,
            labels: HashMap::new(),
            sidebar_items: Vec::new(),
            next_label_id: 0,
            agents,
            agent_list_state,
            agent_scroll_offset: 0,
            ui: UiState {
                focus: Focus::Sessions,
                input_mode: InputMode::Normal,
                input_buffer: String::new(),
                left_tab: LeftTab::Sessions,
                selected_cli_type: CliType::Claude,
                pending_session_name: None,
                copied_at: None,
                left_panel_half: false,
            },
            layout: LayoutCache {
                left_panel_width: 42,
                last_right_panel_size: (0, 0),
                last_sessions_area: Rect::default(),
                last_right_panel_area: Rect::default(),
                last_right_panel_inner: Rect::default(),
            },
            drag: DragState {
                selection: None,
                dragging_divider: false,
                dragging_scrollbar: false,
                dragging_sessions_scrollbar: false,
                dragging_session: None,
            },
            event_tx,
            should_quit: false,
            last_status_poll: Instant::now(),
        }
    }

    fn load_agents() -> Vec<AgentFile> {
        let agents_dir = std::env::current_dir().unwrap_or_default().join("agents");
        let mut agents = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
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

    // ── Session lookup ───────────────────────────────────

    pub(crate) fn session_by_id_mut(&mut self, id: usize) -> Option<&mut Session> {
        let &idx = self.session_id_map.get(&id)?;
        let session = self.sessions.get_mut(idx)?;
        if session.id == id {
            Some(session)
        } else {
            None
        }
    }

    fn rebuild_session_id_map(&mut self) {
        self.session_id_map.clear();
        for (idx, session) in self.sessions.iter().enumerate() {
            self.session_id_map.insert(session.id, idx);
        }
    }

    pub(crate) fn move_sidebar_item(&mut self, from: usize, to: usize) {
        if from == to || from >= self.sidebar_items.len() || to >= self.sidebar_items.len() {
            return;
        }
        // When dragging a label, move the entire group (label + its sessions)
        if matches!(self.sidebar_items[from], SidebarItem::Label(_)) {
            let range = self.label_group_range(from);
            let group: Vec<_> = self.sidebar_items.drain(range.clone()).collect();
            let group_len = group.len();
            // Adjust target for the removal: if target was after the group, shift back
            let adjusted_to = if to > range.start {
                to.saturating_sub(group_len).min(self.sidebar_items.len())
            } else {
                to.min(self.sidebar_items.len())
            };
            // Splice the group back at the adjusted position
            for (i, item) in group.into_iter().enumerate() {
                self.sidebar_items.insert(adjusted_to + i, item);
            }
            self.list_state.select(Some(adjusted_to));
        } else {
            let item = self.sidebar_items.remove(from);
            self.sidebar_items.insert(to, item);
            self.list_state.select(Some(to));
        }
    }

    pub(crate) fn panel_size_or_default(&self) -> (u16, u16) {
        if self.layout.last_right_panel_size != (0, 0) {
            self.layout.last_right_panel_size
        } else {
            (24, 80)
        }
    }

    // ── Session creation ─────────────────────────────────

    pub(crate) fn create_session(
        &mut self,
        name: String,
        cli_type: CliType,
        rows: u16,
        cols: u16,
    ) {
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
        let escaped_prompt = config.prompt.replace('"', "\"\"").replace('\n', " ");
        let ralph_cmd = format!(
            "/ralph-loop {} --max-iterations {} --completion-promise \"{}\"",
            escaped_prompt, config.max_iterations, config.completion_promise
        );

        // Store pending command on the just-created session
        if let Some(session) = self.sessions.last_mut() {
            session.set_pending_ralph(ralph_cmd);
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
                let mut session = Session::new_failed(
                    id,
                    name,
                    cli_type,
                    parser,
                    (rows, cols),
                    PathBuf::new(),
                    is_ralph,
                );
                session.process_pty_output(
                    format!("Failed to open PTY: {}\r\n", e).as_bytes(),
                );
                self.push_session_to_sidebar(session);
                return id;
            }
        };

        let status_path = if cli_type == CliType::Console {
            PathBuf::new()
        } else {
            PathBuf::from(format!(
                "/tmp/neimar-{}-status-{}",
                std::process::id(),
                id
            ))
        };

        // Ralph sessions spawn interactive claude (no -p), regular sessions use the cli_type command
        let mut cmd = CommandBuilder::new(cli_type.command());
        for arg in cli_type.args() {
            cmd.arg(arg);
        }
        cmd.env("TERM", "xterm-256color");
        if !status_path.as_os_str().is_empty() {
            cmd.env("NEIMAR_STATUS_FILE", status_path.to_str().unwrap());
        }
        cmd.cwd(std::env::current_dir().unwrap());

        let child = match pair.slave.spawn_command(cmd) {
            Ok(child) => child,
            Err(e) => {
                let mut session = Session::new_failed(
                    id,
                    name,
                    cli_type,
                    parser,
                    (rows, cols),
                    status_path,
                    is_ralph,
                );
                session.process_pty_output(
                    format!("Failed to spawn {}: {}\r\n", cli_type.command(), e).as_bytes(),
                );
                self.push_session_to_sidebar(session);
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

        let session = Session::new(
            id,
            name,
            cli_type,
            SessionStatus::Running,
            parser,
            Some(pair.master),
            Some(writer),
            Some(child),
            (rows, cols),
            status_path,
            is_ralph,
        );

        self.push_session_to_sidebar(session);
        self.ui.focus = Focus::Terminal;
        id
    }

    /// Push a session to storage and insert it into the sidebar at the right position.
    fn push_session_to_sidebar(&mut self, session: Session) -> usize {
        let id = session.id;
        self.sessions.push(session);
        self.rebuild_session_id_map();
        let insert_pos = self.sidebar_insert_position();
        self.sidebar_items
            .insert(insert_pos, SidebarItem::Session(id));
        self.list_state.select(Some(insert_pos));
        insert_pos
    }

    // ── Sidebar & session queries ────────────────────────

    pub(crate) fn selected_sidebar_item(&self) -> Option<&SidebarItem> {
        self.list_state
            .selected()
            .and_then(|i| self.sidebar_items.get(i))
    }

    /// Map the current selection to a real session index (None if a label is selected).
    pub(crate) fn selected_real_index(&self) -> Option<usize> {
        match self.selected_sidebar_item()? {
            SidebarItem::Session(id) => self.session_id_map.get(id).copied(),
            SidebarItem::Label(_) => None,
        }
    }

    pub(crate) fn selected_session(&self) -> Option<&Session> {
        self.selected_real_index()
            .and_then(|i| self.sessions.get(i))
    }

    pub(crate) fn selected_session_mut(&mut self) -> Option<&mut Session> {
        let idx = self.selected_real_index();
        idx.and_then(|i| self.sessions.get_mut(i))
    }

    // ── Label management ─────────────────────────────────

    pub(crate) fn create_label(&mut self, name: String) {
        let id = self.next_label_id;
        self.next_label_id += 1;
        self.labels.insert(id, name);
        let insert_pos = self.sidebar_insert_position();
        self.sidebar_items.insert(insert_pos, SidebarItem::Label(id));
        self.list_state.select(Some(insert_pos));
    }

    /// Returns the range in sidebar_items for a label group (the label + its sessions).
    pub(crate) fn label_group_range(&self, label_pos: usize) -> std::ops::Range<usize> {
        let end = self.sidebar_items[label_pos + 1..]
            .iter()
            .position(|item| matches!(item, SidebarItem::Label(_)))
            .map(|p| label_pos + 1 + p)
            .unwrap_or(self.sidebar_items.len());
        label_pos..end
    }

    /// Where to insert a new sidebar item (after current selection, respecting label groups).
    fn sidebar_insert_position(&self) -> usize {
        if let Some(sel) = self.list_state.selected() {
            if sel < self.sidebar_items.len() {
                match &self.sidebar_items[sel] {
                    SidebarItem::Label(_) => self.label_group_range(sel).end,
                    SidebarItem::Session(_) => sel + 1,
                }
            } else {
                self.sidebar_items.len()
            }
        } else {
            self.sidebar_items.len()
        }
    }

    pub(crate) fn move_sidebar_item_up(&mut self) {
        let Some(sel) = self.list_state.selected() else {
            return;
        };
        if sel == 0 {
            return;
        }
        match &self.sidebar_items[sel] {
            SidebarItem::Session(_) => {
                self.sidebar_items.swap(sel, sel - 1);
                self.list_state.select(Some(sel - 1));
            }
            SidebarItem::Label(_) => {
                let range = self.label_group_range(sel);
                if range.start == 0 {
                    return;
                }
                // Swap the item above the group into the end of the group
                let above = self.sidebar_items.remove(range.start - 1);
                self.sidebar_items.insert(range.end - 1, above);
                self.list_state.select(Some(sel - 1));
            }
        }
    }

    pub(crate) fn move_sidebar_item_down(&mut self) {
        let Some(sel) = self.list_state.selected() else {
            return;
        };
        if sel >= self.sidebar_items.len() - 1 {
            return;
        }
        match &self.sidebar_items[sel] {
            SidebarItem::Session(_) => {
                self.sidebar_items.swap(sel, sel + 1);
                self.list_state.select(Some(sel + 1));
            }
            SidebarItem::Label(_) => {
                let range = self.label_group_range(sel);
                if range.end >= self.sidebar_items.len() {
                    return;
                }
                // Move the item below the group to just before the label
                let below = self.sidebar_items.remove(range.end);
                self.sidebar_items.insert(range.start, below);
                self.list_state.select(Some(sel + 1));
            }
        }
    }

    // ── Session removal & cleanup ────────────────────────

    pub(crate) fn remove_selected_sidebar_item(&mut self) {
        let Some(sel) = self.list_state.selected() else {
            return;
        };
        let Some(item) = self.sidebar_items.get(sel).cloned() else {
            return;
        };
        match item {
            SidebarItem::Session(id) => {
                if let Some(&real_idx) = self.session_id_map.get(&id) {
                    let status_path = self.sessions[real_idx].status_file_path().to_path_buf();
                    Self::cleanup_status_file(&status_path);
                    self.sessions.remove(real_idx);
                    self.rebuild_session_id_map();
                }
                self.sidebar_items.remove(sel);
            }
            SidebarItem::Label(label_id) => {
                // Remove just the label, sessions underneath stay (become unlabeled)
                self.labels.remove(&label_id);
                self.sidebar_items.remove(sel);
            }
        }
        if self.sidebar_items.is_empty() {
            self.list_state.select(None);
            self.ui.focus = Focus::Sessions;
        } else {
            let sel = self.list_state.selected().unwrap_or(0);
            if sel >= self.sidebar_items.len() {
                self.list_state.select(Some(self.sidebar_items.len() - 1));
            }
        }
    }

    fn cleanup_status_file(path: &PathBuf) {
        if !path.as_os_str().is_empty() {
            let _ = std::fs::remove_file(path);
        }
    }

    fn cleanup_all_status_files(&self) {
        for session in &self.sessions {
            let path = session.status_file_path().to_path_buf();
            Self::cleanup_status_file(&path);
        }
    }

    // ── Polling ──────────────────────────────────────────

    pub(crate) fn poll_status_files(&mut self) {
        if self.last_status_poll.elapsed() < STATUS_POLL_INTERVAL {
            return;
        }
        self.last_status_poll = Instant::now();

        for session in &mut self.sessions {
            session.poll_status_file();
            session.poll_transcript();
        }
    }

    pub(crate) fn check_pending_ralph_commands(&mut self) {
        for session in &mut self.sessions {
            session.try_inject_ralph_command(RALPH_MIN_WAIT, RALPH_MAX_WAIT);
        }
    }

    // ── Screen coords & clipboard ────────────────────────

    /// Convert absolute screen (column, row) to vt100 screen-space (row, col),
    /// returning None if the point is outside the inner right panel rect.
    pub(crate) fn screen_coords_from_mouse(&self, column: u16, row: u16) -> Option<(u16, u16)> {
        let inner = self.layout.last_right_panel_inner;
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
        let sel = match &self.drag.selection {
            Some(s) => s,
            None => return,
        };
        let idx = match self.selected_real_index() {
            Some(i) => i,
            None => return,
        };

        let (start_row, start_col, end_row, end_col) = sel.ordered();
        let scroll_offset = sel.scroll_offset;

        let text = self.sessions[idx].read_selection_text(
            start_row, start_col, end_row, end_col, scroll_offset,
        );

        if text.is_empty() {
            return;
        }

        if let Ok(mut clipboard) = arboard::Clipboard::new()
            && clipboard.set_text(text).is_ok()
        {
            self.ui.copied_at = Some(Instant::now());
        }
    }

    // ── Shutdown ─────────────────────────────────────────

    pub(crate) fn shutdown(&mut self) {
        for session in &mut self.sessions {
            session.send_shutdown_signals();
        }
        for session in &mut self.sessions {
            session.drop_pty();
        }
        self.cleanup_all_status_files();
    }
}
