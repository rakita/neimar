use crate::app::{App, Focus, InputMode, LeftTab, RalphConfig, Selection};
use crate::mouse;
use crate::session::{CliType, SESSION_ITEM_HEIGHT};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::io::Write;

// ── Key to terminal bytes ───────────────────────────────

fn key_to_bytes(key: &KeyEvent) -> Vec<u8> {
    if key.modifiers.contains(KeyModifiers::CONTROL)
        && let KeyCode::Char(c) = key.code
    {
        let c_lower = c.to_ascii_lowercase();
        if c_lower.is_ascii_lowercase() {
            return vec![c_lower as u8 - b'a' + 1];
        }
        return vec![];
    }

    if key.modifiers.contains(KeyModifiers::ALT)
        && let KeyCode::Char(c) = key.code
    {
        let mut bytes = vec![0x1b];
        let mut buf = [0u8; 4];
        bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        return bytes;
    }

    match key.code {
        KeyCode::Char(c) => {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf);
            buf[..c.len_utf8()].to_vec()
        }
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(1) => b"\x1bOP".to_vec(),
        KeyCode::F(2) => b"\x1bOQ".to_vec(),
        KeyCode::F(3) => b"\x1bOR".to_vec(),
        KeyCode::F(4) => b"\x1bOS".to_vec(),
        KeyCode::F(5) => b"\x1b[15~".to_vec(),
        KeyCode::F(6) => b"\x1b[17~".to_vec(),
        KeyCode::F(7) => b"\x1b[18~".to_vec(),
        KeyCode::F(8) => b"\x1b[19~".to_vec(),
        KeyCode::F(9) => b"\x1b[20~".to_vec(),
        KeyCode::F(10) => b"\x1b[21~".to_vec(),
        KeyCode::F(11) => b"\x1b[23~".to_vec(),
        KeyCode::F(12) => b"\x1b[24~".to_vec(),
        _ => vec![],
    }
}

// ── Input handling ──────────────────────────────────────

impl App {
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        match self.input_mode {
            InputMode::SelectingSessionType => {
                self.handle_selecting_type_key(key);
                return;
            }
            InputMode::NamingSession => {
                self.handle_naming_key(key);
                return;
            }
            InputMode::RenamingSession => {
                self.handle_renaming_key(key);
                return;
            }
            InputMode::NamingRalph => {
                self.handle_naming_ralph_key(key);
                return;
            }
            InputMode::EnteringRalphPrompt => {
                self.handle_ralph_prompt_key(key);
                return;
            }
            InputMode::Normal => {}
        }

        // Shift+Arrow/Page keys work regardless of current panel
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            let panel_height = self.last_right_panel_size.0 as usize;
            match key.code {
                KeyCode::Left => {
                    self.focus = Focus::Sessions;
                    return;
                }
                KeyCode::Right => {
                    if self.left_tab == LeftTab::Sessions && self.selected_session().is_some() {
                        self.focus = Focus::Terminal;
                    }
                    return;
                }
                KeyCode::PageUp => {
                    self.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset += panel_height.saturating_sub(1).max(1);
                        session.clamp_scroll();
                    }
                    return;
                }
                KeyCode::PageDown => {
                    self.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset = session
                            .scroll_offset
                            .saturating_sub(panel_height.saturating_sub(1).max(1));
                    }
                    return;
                }
                KeyCode::Up => {
                    self.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset += 1;
                        session.clamp_scroll();
                    }
                    return;
                }
                KeyCode::Down => {
                    self.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset = session.scroll_offset.saturating_sub(1);
                    }
                    return;
                }
                _ => {}
            }
        }

        match self.focus {
            Focus::Sessions => match self.left_tab {
                LeftTab::Sessions => self.handle_sessions_key(key),
                LeftTab::Agents => self.handle_agents_key(key),
            },
            Focus::Terminal => self.handle_terminal_key(key),
        }
    }

    fn handle_sessions_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('n') => {
                self.input_mode = InputMode::SelectingSessionType;
                self.selected_cli_type = CliType::Claude;
            }
            KeyCode::Char('l') => {
                self.input_mode = InputMode::NamingRalph;
                self.input_buffer.clear();
            }
            KeyCode::Char('r') => {
                if let Some(real_idx) = self.selected_real_index() {
                    Self::cleanup_status_file(&self.sessions[real_idx].status_file);
                    self.sessions.remove(real_idx);
                    self.rebuild_session_id_map();
                    let vis = self.visible_sessions();
                    if vis.is_empty() {
                        self.list_state.select(None);
                        self.focus = Focus::Sessions;
                    } else {
                        let sel = self.list_state.selected().unwrap_or(0);
                        if sel >= vis.len() {
                            self.list_state.select(Some(vis.len() - 1));
                        }
                    }
                }
            }
            KeyCode::Char('e') => {
                if let Some(real_idx) = self.selected_real_index() {
                    self.input_mode = InputMode::RenamingSession;
                    self.input_buffer = self.sessions[real_idx].name.clone();
                }
            }
            KeyCode::Char('h') => {
                self.left_panel_half = !self.left_panel_half;
                if !self.left_panel_half {
                    self.left_panel_width = 42;
                }
            }
            KeyCode::Left | KeyCode::Right => {
                self.left_tab = LeftTab::Agents;
                self.agent_scroll_offset = 0;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selection = None;
                let vis = self.visible_sessions();
                if let Some(sel) = self.list_state.selected() {
                    if sel > 0 {
                        self.list_state.select(Some(sel - 1));
                    } else if !vis.is_empty() {
                        self.list_state.select(Some(vis.len() - 1));
                    }
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selection = None;
                let vis = self.visible_sessions();
                if let Some(sel) = self.list_state.selected() {
                    if sel + 1 < vis.len() {
                        self.list_state.select(Some(sel + 1));
                    } else {
                        self.list_state.select(Some(0));
                    }
                } else if !vis.is_empty() {
                    self.list_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    fn handle_agents_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left | KeyCode::Right => {
                self.left_tab = LeftTab::Sessions;
            }
            KeyCode::Char('n') => {
                self.left_tab = LeftTab::Sessions;
                self.input_mode = InputMode::SelectingSessionType;
                self.selected_cli_type = CliType::Claude;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(sel) = self.agent_list_state.selected() {
                    if sel > 0 {
                        self.agent_list_state.select(Some(sel - 1));
                    } else if !self.agents.is_empty() {
                        self.agent_list_state.select(Some(self.agents.len() - 1));
                    }
                    self.agent_scroll_offset = 0;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(sel) = self.agent_list_state.selected() {
                    if sel + 1 < self.agents.len() {
                        self.agent_list_state.select(Some(sel + 1));
                    } else {
                        self.agent_list_state.select(Some(0));
                    }
                    self.agent_scroll_offset = 0;
                } else if !self.agents.is_empty() {
                    self.agent_list_state.select(Some(0));
                    self.agent_scroll_offset = 0;
                }
            }
            _ => {}
        }
    }

    fn handle_naming_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    let name = self.input_buffer.clone();
                    let cli_type = self.selected_cli_type;
                    self.input_mode = InputMode::Normal;
                    self.input_buffer.clear();
                    let (rows, cols) = if self.last_right_panel_size != (0, 0) {
                        self.last_right_panel_size
                    } else {
                        (24, 80)
                    };
                    self.create_session(name, cli_type, rows, cols);
                } else {
                    self.input_mode = InputMode::Normal;
                    self.input_buffer.clear();
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_renaming_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    let new_name = self.input_buffer.clone();
                    if let Some(session) = self.selected_session_mut() {
                        session.name = new_name;
                    }
                }
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_selecting_type_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::Char('j')
            | KeyCode::Char('k')
            | KeyCode::Tab => {
                self.selected_cli_type = match self.selected_cli_type {
                    CliType::Claude => CliType::Amp,
                    CliType::Amp => CliType::Console,
                    CliType::Console => CliType::Claude,
                };
            }
            KeyCode::Enter => {
                self.input_mode = InputMode::NamingSession;
                self.input_buffer.clear();
            }
            KeyCode::Char('1') => {
                self.selected_cli_type = CliType::Claude;
                self.input_mode = InputMode::NamingSession;
                self.input_buffer.clear();
            }
            KeyCode::Char('2') => {
                self.selected_cli_type = CliType::Amp;
                self.input_mode = InputMode::NamingSession;
                self.input_buffer.clear();
            }
            KeyCode::Char('3') => {
                self.selected_cli_type = CliType::Console;
                self.input_mode = InputMode::NamingSession;
                self.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_naming_ralph_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.input_buffer.is_empty() {
                    self.pending_session_name = Some(self.input_buffer.clone());
                    self.input_buffer.clear();
                    self.input_mode = InputMode::EnteringRalphPrompt;
                } else {
                    self.input_mode = InputMode::Normal;
                    self.input_buffer.clear();
                }
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_ralph_prompt_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if self.input_buffer.is_empty() {
                    self.input_mode = InputMode::Normal;
                    self.pending_session_name = None;
                    return;
                }

                let prompt = self.input_buffer.clone();
                let name = self
                    .pending_session_name
                    .take()
                    .unwrap_or_else(|| "Ralph".to_string());
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();

                let (rows, cols) = if self.last_right_panel_size != (0, 0) {
                    self.last_right_panel_size
                } else {
                    (24, 80)
                };

                let config = RalphConfig {
                    prompt,
                    max_iterations: 50,
                    completion_promise: "COMPLETE".to_string(),
                };
                self.create_ralph_session(name, rows, cols, config);
            }
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.input_buffer.clear();
                self.pending_session_name = None;
            }
            KeyCode::Char(c) => self.input_buffer.push(c),
            KeyCode::Backspace => {
                self.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_terminal_key(&mut self, key: KeyEvent) {
        // Cmd+C with active selection: copy to clipboard instead of sending to PTY
        let is_copy = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::SUPER);
        if is_copy && self.selection.is_some() {
            self.copy_selection_to_clipboard();
            self.selection = None;
            return;
        }

        // Esc when scrolled: snap to bottom, consume the key
        if key.code == KeyCode::Esc
            && let Some(session) = self.selected_session_mut()
            && session.scroll_offset > 0
        {
            session.scroll_offset = 0;
            self.selection = None;
            return;
        }

        // Clear selection on any key forwarded to PTY
        self.selection = None;

        // Any other key forwarded to PTY: auto-snap to bottom
        if let Some(session) = self.selected_session_mut() {
            session.scroll_offset = 0;
            if let Some(writer) = &mut session.pty_writer {
                let bytes = key_to_bytes(&key);
                if !bytes.is_empty() {
                    let _ = writer.write_all(&bytes);
                    let _ = writer.flush();
                }
            }
        }
    }

    pub(crate) fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if clicking on the divider between panels
                let divider_x = self.last_right_panel_area.x;
                if divider_x > 0
                    && event.column >= divider_x.saturating_sub(1)
                    && event.column <= divider_x + 1
                    && event.row >= self.last_right_panel_area.y
                    && event.row < self.last_right_panel_area.y + self.last_right_panel_area.height
                {
                    self.dragging_divider = true;
                    return;
                }

                // Check if clicking on the scrollbar (rightmost column of inner panel)
                let inner = self.last_right_panel_inner;
                if inner.width > 0
                    && inner.height > 1
                    && event.column == inner.x + inner.width - 1
                    && event.row >= inner.y
                    && event.row < inner.y + inner.height
                    && let Some(session) = self.selected_session_mut()
                {
                    let max_scroll = session.max_scrollback();
                    if max_scroll > 0 {
                        let y_ratio =
                            (event.row - inner.y) as f64 / (inner.height - 1).max(1) as f64;
                        let y_ratio = y_ratio.clamp(0.0, 1.0);
                        session.scroll_offset =
                            ((1.0 - y_ratio) * max_scroll as f64).round() as usize;
                        self.dragging_scrollbar = true;
                        self.selection = None;
                        return;
                    }
                }

                // Clear any existing selection on new click
                self.selection = None;

                match self.left_tab {
                    LeftTab::Sessions => {
                        let vis_count = self.visible_sessions().len();
                        let scroll_offset = self.list_state.offset();
                        if let Some(index) = mouse::clicked_session_index(
                            event.column,
                            event.row,
                            self.last_sessions_area,
                            vis_count,
                            SESSION_ITEM_HEIGHT,
                            scroll_offset,
                        ) {
                            self.list_state.select(Some(index));
                            self.focus = Focus::Sessions;
                        }
                    }
                    LeftTab::Agents => {
                        let scroll_offset = self.agent_list_state.offset();
                        if let Some(index) = mouse::clicked_session_index(
                            event.column,
                            event.row,
                            self.last_sessions_area,
                            self.agents.len(),
                            1,
                            scroll_offset,
                        ) {
                            self.agent_list_state.select(Some(index));
                            self.agent_scroll_offset = 0;
                            self.focus = Focus::Sessions;
                        }
                    }
                }
                // Clicking anywhere on left panel focuses it
                let larea = self.last_sessions_area;
                if event.column >= larea.x
                    && event.column < larea.x + larea.width
                    && event.row >= larea.y
                    && event.row < larea.y + larea.height
                {
                    self.focus = Focus::Sessions;
                }
                // Clicking on right panel: start selection and switch focus
                if let Some((vt_row, vt_col)) =
                    self.screen_coords_from_mouse(event.column, event.row)
                {
                    let scroll_offset = self
                        .selected_session()
                        .map(|s| s.scroll_offset)
                        .unwrap_or(0);
                    self.selection = Some(Selection {
                        anchor_row: vt_row,
                        anchor_col: vt_col,
                        end_row: vt_row,
                        end_col: vt_col,
                        scroll_offset,
                    });
                    if self.left_tab == LeftTab::Sessions && self.selected_session().is_some() {
                        self.focus = Focus::Terminal;
                    }
                } else {
                    // Clicking on right panel border still switches focus
                    let area = self.last_right_panel_area;
                    if event.column >= area.x
                        && event.column < area.x + area.width
                        && event.row >= area.y
                        && event.row < area.y + area.height
                        && self.left_tab == LeftTab::Sessions
                        && self.selected_session().is_some()
                    {
                        self.focus = Focus::Terminal;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Handle divider dragging
                if self.dragging_divider {
                    self.left_panel_half = false;
                    let total_width =
                        self.last_sessions_area.width + self.last_right_panel_area.width;
                    let min_width: u16 = 15;
                    let max_width = total_width / 2;
                    self.left_panel_width = event.column.clamp(min_width, max_width);
                    return;
                }
                // Handle scrollbar dragging
                if self.dragging_scrollbar {
                    let inner = self.last_right_panel_inner;
                    if inner.height > 1 {
                        let clamped_row = event.row.max(inner.y).min(inner.y + inner.height - 1);
                        let y_ratio =
                            (clamped_row - inner.y) as f64 / (inner.height - 1).max(1) as f64;
                        let y_ratio = y_ratio.clamp(0.0, 1.0);
                        if let Some(session) = self.selected_session_mut() {
                            let max_scroll = session.max_scrollback();
                            session.scroll_offset =
                                ((1.0 - y_ratio) * max_scroll as f64).round() as usize;
                        }
                    }
                    return;
                }
                // Update selection end point, clamped to inner rect bounds
                if self.selection.is_some() {
                    let inner = self.last_right_panel_inner;
                    if inner.width > 0 && inner.height > 0 {
                        let clamped_col =
                            event.column.max(inner.x).min(inner.x + inner.width - 1) - inner.x;
                        let clamped_row =
                            event.row.max(inner.y).min(inner.y + inner.height - 1) - inner.y;
                        if let Some(sel) = &mut self.selection {
                            sel.end_row = clamped_row;
                            sel.end_col = clamped_col;
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.dragging_divider = false;
                self.dragging_scrollbar = false;
                // Auto-copy selection to clipboard on mouse-up if it's a real drag (not just a click)
                if let Some(sel) = &self.selection {
                    let (sr, sc, er, ec) = sel.ordered();
                    if sr != er || sc != ec {
                        self.copy_selection_to_clipboard();
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.selection = None;
                let area = self.last_right_panel_area;
                if event.column >= area.x
                    && event.column < area.x + area.width
                    && event.row >= area.y
                    && event.row < area.y + area.height
                {
                    if self.left_tab == LeftTab::Agents {
                        self.agent_scroll_offset = self.agent_scroll_offset.saturating_add(1);
                    } else if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset += 1;
                        session.clamp_scroll();
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                self.selection = None;
                let area = self.last_right_panel_area;
                if event.column >= area.x
                    && event.column < area.x + area.width
                    && event.row >= area.y
                    && event.row < area.y + area.height
                {
                    if self.left_tab == LeftTab::Agents {
                        self.agent_scroll_offset = self.agent_scroll_offset.saturating_sub(1);
                    } else if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset = session.scroll_offset.saturating_sub(1);
                    }
                }
            }
            _ => {}
        }
    }
}
