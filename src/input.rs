use crate::app::App;
use crate::mouse;
use crate::types::{
    CliType, DraggingSession, Focus, InputMode, LeftTab, Selection, SidebarItem,
    SESSION_ITEM_HEIGHT,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use rand::RngExt;
use std::time::{Duration, Instant};

const DEFAULT_SESSION_NAMES: &[&str] = &[
    "turbo snail", "angry pickle", "cosmic potato", "dizzy llama", "funky moose",
    "grumpy waffle", "hyper sloth", "jazzy ferret", "karma yeti", "lazy rocket",
    "mighty noodle", "ninja turnip", "odd penguin", "plucky badger", "quirky otter",
    "rowdy puffin", "sneaky wombat", "tiny kraken", "ultra gremlin", "verbose clam",
    "wobbly cactus", "zippy narwhal", "bold pretzel", "crispy goblin", "dapper goose",
    "epic walrus", "fluffy gator", "groovy squid", "humble yak", "itchy parrot",
    "jolly muffin", "keen hamster", "lunar raccoon", "mellow panda", "noisy quokka",
    "peppy wizard", "quiet thunder", "rusty unicorn", "salty pigeon", "toasty cobra",
    "upbeat lemur", "vivid gecko", "witty falcon", "xenial donkey", "yappy coyote",
    "zesty beaver", "snazzy moth", "rapid turtle", "bouncy morel", "chief nugget",
];

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

    if key.modifiers.contains(KeyModifiers::ALT) {
        if let KeyCode::Char(c) = key.code {
            let mut bytes = vec![0x1b];
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            return bytes;
        }
        if key.code == KeyCode::Backspace {
            return vec![0x1b, 0x7f];
        }
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
        match self.ui.input_mode {
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
            InputMode::NamingLabel => {
                self.handle_naming_label_key(key);
                return;
            }
            InputMode::Normal => {}
        }

        // Shift+Arrow/Page keys work regardless of current panel
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            let panel_height = self.layout.last_right_panel_size.0 as usize;
            match key.code {
                KeyCode::Left => {
                    self.ui.focus = Focus::Sessions;
                    return;
                }
                KeyCode::Right => {
                    if self.ui.left_tab == LeftTab::Sessions && self.selected_session().is_some() {
                        self.ui.focus = Focus::Terminal;
                    }
                    return;
                }
                KeyCode::Up => {
                    if self.ui.left_tab == LeftTab::Sessions {
                        self.move_sidebar_item_up();
                    }
                    return;
                }
                KeyCode::Down => {
                    if self.ui.left_tab == LeftTab::Sessions {
                        self.move_sidebar_item_down();
                    }
                    return;
                }
                KeyCode::PageUp => {
                    self.drag.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset += panel_height.saturating_sub(1).max(1);
                        session.clamp_scroll();
                    }
                    return;
                }
                KeyCode::PageDown => {
                    self.drag.selection = None;
                    if let Some(session) = self.selected_session_mut() {
                        session.scroll_offset = session
                            .scroll_offset
                            .saturating_sub(panel_height.saturating_sub(1).max(1));
                    }
                    return;
                }
                _ => {}
            }
        }

        // Ctrl+key commands work regardless of current panel
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('n') => {
                    self.ui.input_mode = InputMode::SelectingSessionType;
                    self.ui.selected_cli_type = CliType::Claude;
                    return;
                }
                KeyCode::Char('e') => {
                    match self.selected_sidebar_item().cloned() {
                        Some(SidebarItem::Session(_)) => {
                            if let Some(real_idx) = self.selected_real_index() {
                                self.ui.input_mode = InputMode::RenamingSession;
                                self.ui.input_buffer = self.sessions[real_idx].name.clone();
                            }
                        }
                        Some(SidebarItem::Label(label_id)) => {
                            if let Some(name) = self.labels.get(&label_id) {
                                self.ui.input_mode = InputMode::RenamingSession;
                                self.ui.input_buffer = name.clone();
                            }
                        }
                        None => {}
                    }
                    return;
                }
                KeyCode::Char('g') => {
                    self.ui.input_mode = InputMode::NamingLabel;
                    self.ui.input_buffer.clear();
                    return;
                }
                KeyCode::Char('r') => {
                    self.remove_selected_sidebar_item();
                    return;
                }
                _ => {}
            }
        }


        match self.ui.focus {
            Focus::Sessions => match self.ui.left_tab {
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
                self.ui.input_mode = InputMode::SelectingSessionType;
                self.ui.selected_cli_type = CliType::Claude;
            }
            KeyCode::Char('g') => {
                self.ui.input_mode = InputMode::NamingLabel;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char('r') => {
                self.remove_selected_sidebar_item();
            }
            KeyCode::Char('e') => {
                match self.selected_sidebar_item().cloned() {
                    Some(SidebarItem::Session(_)) => {
                        if let Some(real_idx) = self.selected_real_index() {
                            self.ui.input_mode = InputMode::RenamingSession;
                            self.ui.input_buffer = self.sessions[real_idx].name.clone();
                        }
                    }
                    Some(SidebarItem::Label(label_id)) => {
                        if let Some(name) = self.labels.get(&label_id) {
                            self.ui.input_mode = InputMode::RenamingSession;
                            self.ui.input_buffer = name.clone();
                        }
                    }
                    None => {}
                }
            }
            KeyCode::Left | KeyCode::Right => {
                self.ui.left_tab = LeftTab::Agents;
                self.agent_scroll_offset = 0;
            }
            KeyCode::Up => {
                self.drag.selection = None;
                let count = self.sidebar_items.len();
                if let Some(sel) = self.list_state.selected() {
                    if sel > 0 {
                        self.list_state.select(Some(sel - 1));
                    } else if count > 0 {
                        self.list_state.select(Some(count - 1));
                    }
                }
            }
            KeyCode::Down => {
                self.drag.selection = None;
                let count = self.sidebar_items.len();
                if let Some(sel) = self.list_state.selected() {
                    if sel + 1 < count {
                        self.list_state.select(Some(sel + 1));
                    } else {
                        self.list_state.select(Some(0));
                    }
                } else if count > 0 {
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
                self.ui.left_tab = LeftTab::Sessions;
            }
            KeyCode::Char('n') => {
                self.ui.left_tab = LeftTab::Sessions;
                self.ui.input_mode = InputMode::SelectingSessionType;
                self.ui.selected_cli_type = CliType::Claude;
            }
            KeyCode::Up => {
                if let Some(sel) = self.agent_list_state.selected() {
                    if sel > 0 {
                        self.agent_list_state.select(Some(sel - 1));
                    } else if !self.agents.is_empty() {
                        self.agent_list_state.select(Some(self.agents.len() - 1));
                    }
                    self.agent_scroll_offset = 0;
                }
            }
            KeyCode::Down => {
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
                let name = if self.ui.input_buffer.is_empty() {
                    let idx = rand::rng().random_range(0..DEFAULT_SESSION_NAMES.len());
                    DEFAULT_SESSION_NAMES[idx].to_string()
                } else {
                    self.ui.input_buffer.clone()
                };
                let cli_type = self.ui.selected_cli_type;
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
                let (rows, cols) = self.panel_size_or_default();
                self.create_session(name, cli_type, rows, cols);
            }
            KeyCode::Esc => {
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char(c) => self.ui.input_buffer.push(c),
            KeyCode::Backspace => {
                self.ui.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_renaming_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.ui.input_buffer.is_empty() {
                    let new_name = self.ui.input_buffer.clone();
                    match self.selected_sidebar_item().cloned() {
                        Some(SidebarItem::Session(_)) => {
                            if let Some(session) = self.selected_session_mut() {
                                session.name = new_name;
                            }
                        }
                        Some(SidebarItem::Label(label_id)) => {
                            if let Some(name) = self.labels.get_mut(&label_id) {
                                *name = new_name;
                            }
                        }
                        None => {}
                    }
                }
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char(c) => self.ui.input_buffer.push(c),
            KeyCode::Backspace => {
                self.ui.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_selecting_type_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Left => {
                self.ui.selected_cli_type = match self.ui.selected_cli_type {
                    CliType::Claude => CliType::Console,
                    CliType::ClaudeDangerous => CliType::Claude,
                    CliType::Amp => CliType::ClaudeDangerous,
                    CliType::Console => CliType::Amp,
                };
            }
            KeyCode::Right | KeyCode::Tab => {
                self.ui.selected_cli_type = match self.ui.selected_cli_type {
                    CliType::Claude => CliType::ClaudeDangerous,
                    CliType::ClaudeDangerous => CliType::Amp,
                    CliType::Amp => CliType::Console,
                    CliType::Console => CliType::Claude,
                };
            }
            KeyCode::Enter => {
                self.ui.input_mode = InputMode::NamingSession;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char('1') => {
                self.ui.selected_cli_type = CliType::Claude;
                self.ui.input_mode = InputMode::NamingSession;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char('2') => {
                self.ui.selected_cli_type = CliType::ClaudeDangerous;
                self.ui.input_mode = InputMode::NamingSession;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char('3') => {
                self.ui.selected_cli_type = CliType::Amp;
                self.ui.input_mode = InputMode::NamingSession;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char('4') => {
                self.ui.selected_cli_type = CliType::Console;
                self.ui.input_mode = InputMode::NamingSession;
                self.ui.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.ui.input_mode = InputMode::Normal;
            }
            _ => {}
        }
    }

    fn handle_naming_label_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                if !self.ui.input_buffer.is_empty() {
                    let name = self.ui.input_buffer.clone();
                    self.create_label(name);
                }
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
            }
            KeyCode::Esc => {
                self.ui.input_mode = InputMode::Normal;
                self.ui.input_buffer.clear();
            }
            KeyCode::Char(c) => self.ui.input_buffer.push(c),
            KeyCode::Backspace => {
                self.ui.input_buffer.pop();
            }
            _ => {}
        }
    }

    fn handle_terminal_key(&mut self, key: KeyEvent) {
        // Cmd+C with active selection: copy to clipboard instead of sending to PTY
        let is_copy = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::SUPER);
        if is_copy && self.drag.selection.is_some() {
            self.copy_selection_to_clipboard();
            self.drag.selection = None;
            return;
        }

        // Esc when scrolled: snap to bottom, consume the key
        if key.code == KeyCode::Esc
            && let Some(session) = self.selected_session_mut()
            && session.scroll_offset > 0
        {
            session.scroll_offset = 0;
            self.drag.selection = None;
            return;
        }

        // Clear selection on any key forwarded to PTY
        self.drag.selection = None;

        // Any other key forwarded to PTY: auto-snap to bottom
        if let Some(session) = self.selected_session_mut() {
            session.scroll_offset = 0;
            let bytes = key_to_bytes(&key);
            if !bytes.is_empty() {
                session.write_to_pty(&bytes);
            }
        }
    }

    pub(crate) fn handle_mouse(&mut self, event: MouseEvent) {
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Check if clicking on the divider between panels
                let divider_x = self.layout.last_right_panel_area.x;
                if divider_x > 0
                    && event.column >= divider_x.saturating_sub(1)
                    && event.column <= divider_x + 1
                    && event.row >= self.layout.last_right_panel_area.y
                    && event.row < self.layout.last_right_panel_area.y + self.layout.last_right_panel_area.height
                {
                    self.drag.dragging_divider = true;
                    return;
                }

                // Check if clicking on the scrollbar (rightmost column of inner panel)
                let inner = self.layout.last_right_panel_inner;
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
                        self.drag.dragging_scrollbar = true;
                        self.drag.selection = None;
                        return;
                    }
                }

                // Check if clicking on the sessions scrollbar (rightmost column of sessions inner area)
                if self.ui.left_tab == LeftTab::Sessions {
                    let sess_inner = self.layout.last_sessions_area.inner(ratatui::layout::Margin::new(1, 1));
                    let item_count = self.sidebar_items.len();
                    if sess_inner.width > 0
                        && sess_inner.height > 1
                        && item_count > sess_inner.height as usize
                        && event.column == sess_inner.x + sess_inner.width - 1
                        && event.row >= sess_inner.y
                        && event.row < sess_inner.y + sess_inner.height
                    {
                        let y_ratio = (event.row - sess_inner.y) as f64
                            / (sess_inner.height - 1).max(1) as f64;
                        let y_ratio = y_ratio.clamp(0.0, 1.0);
                        let target = (y_ratio * (item_count - 1) as f64).round() as usize;
                        self.list_state.select(Some(target.min(item_count - 1)));
                        self.drag.dragging_sessions_scrollbar = true;
                        self.drag.selection = None;
                        return;
                    }
                }

                // Clear any existing selection on new click
                self.drag.selection = None;

                match self.ui.left_tab {
                    LeftTab::Sessions => {
                        let item_count = self.sidebar_items.len();
                        let scroll_offset = self.list_state.offset();
                        if let Some(index) = mouse::clicked_session_index(
                            event.column,
                            event.row,
                            self.layout.last_sessions_area,
                            item_count,
                            SESSION_ITEM_HEIGHT,
                            scroll_offset,
                        ) {
                            self.list_state.select(Some(index));
                            self.ui.focus = Focus::Sessions;
                            self.drag.dragging_session = Some(DraggingSession {
                                from_index: index,
                                target_index: index,
                            });
                        }
                    }
                    LeftTab::Agents => {
                        let scroll_offset = self.agent_list_state.offset();
                        if let Some(index) = mouse::clicked_session_index(
                            event.column,
                            event.row,
                            self.layout.last_sessions_area,
                            self.agents.len(),
                            1,
                            scroll_offset,
                        ) {
                            self.agent_list_state.select(Some(index));
                            self.agent_scroll_offset = 0;
                            self.ui.focus = Focus::Sessions;
                        }
                    }
                }
                // Clicking anywhere on left panel focuses it
                let larea = self.layout.last_sessions_area;
                if event.column >= larea.x
                    && event.column < larea.x + larea.width
                    && event.row >= larea.y
                    && event.row < larea.y + larea.height
                {
                    self.ui.focus = Focus::Sessions;
                }
                // Clicking on right panel: detect double-click or start selection
                if let Some((vt_row, vt_col)) =
                    self.screen_coords_from_mouse(event.column, event.row)
                {
                    let now = Instant::now();
                    let scroll_offset = self
                        .selected_session()
                        .map(|s| s.scroll_offset)
                        .unwrap_or(0);

                    // Double-click detection: same position within 400ms
                    let is_double_click = self
                        .drag
                        .last_click
                        .map(|(col, row, time)| {
                            col == event.column
                                && row == event.row
                                && now.duration_since(time) < Duration::from_millis(400)
                        })
                        .unwrap_or(false);

                    self.drag.last_click = Some((event.column, event.row, now));

                    if is_double_click {
                        self.drag.last_click = None; // reset so triple-click doesn't trigger
                        self.copy_word_at(vt_row, vt_col, scroll_offset);
                    } else {
                        self.drag.selection = Some(Selection {
                            anchor_row: vt_row,
                            anchor_col: vt_col,
                            end_row: vt_row,
                            end_col: vt_col,
                            scroll_offset,
                        });
                    }

                    if self.ui.left_tab == LeftTab::Sessions && self.selected_session().is_some() {
                        self.ui.focus = Focus::Terminal;
                    }
                } else {
                    // Clicking on right panel border still switches focus
                    let area = self.layout.last_right_panel_area;
                    if event.column >= area.x
                        && event.column < area.x + area.width
                        && event.row >= area.y
                        && event.row < area.y + area.height
                        && self.ui.left_tab == LeftTab::Sessions
                        && self.selected_session().is_some()
                    {
                        self.ui.focus = Focus::Terminal;
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // Handle divider dragging
                if self.drag.dragging_divider {
                    let total_width =
                        self.layout.last_sessions_area.width + self.layout.last_right_panel_area.width;
                    let min_width: u16 = 15;
                    let max_width = total_width / 2;
                    self.layout.left_panel_width = event.column.clamp(min_width, max_width);
                    return;
                }
                // Handle sessions scrollbar dragging
                if self.drag.dragging_sessions_scrollbar {
                    let sess_inner = self.layout.last_sessions_area.inner(ratatui::layout::Margin::new(1, 1));
                    let item_count = self.sidebar_items.len();
                    if sess_inner.height > 1 && item_count > 0 {
                        let clamped_row = event.row.max(sess_inner.y).min(sess_inner.y + sess_inner.height - 1);
                        let y_ratio = (clamped_row - sess_inner.y) as f64
                            / (sess_inner.height - 1).max(1) as f64;
                        let y_ratio = y_ratio.clamp(0.0, 1.0);
                        let target = (y_ratio * (item_count - 1) as f64).round() as usize;
                        self.list_state.select(Some(target.min(item_count - 1)));
                    }
                    return;
                }
                // Handle scrollbar dragging
                if self.drag.dragging_scrollbar {
                    let inner = self.layout.last_right_panel_inner;
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
                // Handle session dragging
                if self.drag.dragging_session.is_some() {
                    let item_count = self.sidebar_items.len();
                    let scroll_offset = self.list_state.offset();
                    let target = mouse::clicked_session_index(
                        event.column,
                        event.row,
                        self.layout.last_sessions_area,
                        item_count,
                        SESSION_ITEM_HEIGHT,
                        scroll_offset,
                    )
                    .unwrap_or_else(|| {
                        // If mouse is below items, clamp to last index
                        item_count.saturating_sub(1)
                    });
                    if let Some(ds) = &mut self.drag.dragging_session {
                        ds.target_index = target;
                    }
                    self.list_state.select(Some(target));
                    return;
                }
                // Update selection end point, clamped to inner rect bounds
                if self.drag.selection.is_some() {
                    let inner = self.layout.last_right_panel_inner;
                    if inner.width > 0 && inner.height > 0 {
                        let clamped_col =
                            event.column.max(inner.x).min(inner.x + inner.width - 1) - inner.x;
                        let clamped_row =
                            event.row.max(inner.y).min(inner.y + inner.height - 1) - inner.y;
                        if let Some(sel) = &mut self.drag.selection {
                            sel.end_row = clamped_row;
                            sel.end_col = clamped_col;
                        }
                    }
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.drag.dragging_divider = false;
                self.drag.dragging_scrollbar = false;
                self.drag.dragging_sessions_scrollbar = false;
                if let Some(ds) = self.drag.dragging_session.take() {
                    self.move_sidebar_item(ds.from_index, ds.target_index);
                }
                // Auto-copy selection to clipboard on mouse-up if it's a real drag (not just a click)
                if let Some(sel) = &self.drag.selection {
                    let (sr, sc, er, ec) = sel.ordered();
                    if sr != er || sc != ec {
                        self.copy_selection_to_clipboard();
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.drag.selection = None;
                let larea = self.layout.last_sessions_area;
                let rarea = self.layout.last_right_panel_area;
                if event.column >= larea.x
                    && event.column < larea.x + larea.width
                    && event.row >= larea.y
                    && event.row < larea.y + larea.height
                    && self.ui.left_tab == LeftTab::Sessions
                {
                    // Move selection up (which auto-scrolls the list viewport)
                    if let Some(sel) = self.list_state.selected() {
                        if sel > 0 {
                            self.list_state.select(Some(sel - 1));
                        }
                    }
                } else if event.column >= rarea.x
                    && event.column < rarea.x + rarea.width
                    && event.row >= rarea.y
                    && event.row < rarea.y + rarea.height
                {
                    if self.ui.left_tab == LeftTab::Agents {
                        self.agent_scroll_offset = self.agent_scroll_offset.saturating_add(1);
                    } else if let Some(session) = self.selected_session_mut() {
                        if session.screen().alternate_screen() {
                            // Pager/editor running — forward as Up arrow
                            session.write_to_pty(b"\x1b[A");
                        } else {
                            session.scroll_offset += 1;
                            session.clamp_scroll();
                        }
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                self.drag.selection = None;
                let larea = self.layout.last_sessions_area;
                let rarea = self.layout.last_right_panel_area;
                if event.column >= larea.x
                    && event.column < larea.x + larea.width
                    && event.row >= larea.y
                    && event.row < larea.y + larea.height
                    && self.ui.left_tab == LeftTab::Sessions
                {
                    // Move selection down (which auto-scrolls the list viewport)
                    let count = self.sidebar_items.len();
                    if let Some(sel) = self.list_state.selected() {
                        if sel + 1 < count {
                            self.list_state.select(Some(sel + 1));
                        }
                    }
                } else if event.column >= rarea.x
                    && event.column < rarea.x + rarea.width
                    && event.row >= rarea.y
                    && event.row < rarea.y + rarea.height
                {
                    if self.ui.left_tab == LeftTab::Agents {
                        self.agent_scroll_offset = self.agent_scroll_offset.saturating_sub(1);
                    } else if let Some(session) = self.selected_session_mut() {
                        if session.screen().alternate_screen() {
                            // Pager/editor running — forward as Down arrow
                            session.write_to_pty(b"\x1b[B");
                        } else {
                            session.scroll_offset = session.scroll_offset.saturating_sub(1);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}
