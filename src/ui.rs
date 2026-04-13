use crate::app::App;
use crate::types::{CliType, Focus, InputMode, LeftTab, SidebarItem};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, List, ListItem, Padding, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState,
    },
};
use tui_term::widget::PseudoTerminal;
const PASTEL_CYAN: Color = Color::Rgb(150, 220, 235);
const PASTEL_YELLOW: Color = Color::Rgb(255, 255, 150);

/// Returns the display width of a string, correcting for emoji characters
/// that `unicode-width` underreports (e.g. ⏸ U+23F8, ⚡ U+26A1, ⏩ U+23E9).
fn display_width(s: &str) -> usize {
    s.chars().map(|c| char_width(c)).sum()
}

fn char_width(c: char) -> usize {
    let uw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
    if uw <= 1 && is_wide_emoji(c) {
        2
    } else {
        uw
    }
}

/// Emoji characters commonly reported as width 1 by unicode-width but
/// rendered as width 2 in most terminals.
fn is_wide_emoji(c: char) -> bool {
    matches!(c,
        '\u{23E9}'  // ⏩
        | '\u{23F8}' // ⏸
        | '\u{26A1}' // ⚡
        | '\u{2705}' // ✅
        | '\u{274C}' // ❌
        | '\u{2728}' // ✨
        | '\u{231B}' // ⌛
        | '\u{26D4}' // ⛔
        | '\u{2615}' // ☕
        | '\u{2B50}' // ⭐
        | '\u{26AA}' // ⚪
        | '\u{26AB}' // ⚫
        | '\u{2764}' // ❤
        | '\u{203C}' // ‼
        | '\u{2049}' // ⁉
        | '\u{1F916}' // 🤖 (Claude CLI type)
        | '\u{1F5A5}' // 🖥 (Console CLI type, base char)
        | '\u{1F9F1}' // 🧱 (Working state)
        | '\u{1F4AC}' // 💬 (Input state)
        | '\u{1F4CB}' // 📋 (Planned state)
        | '\u{1F7E2}' // 🟢 (Done state)
        | '\u{1F512}' // 🔒 (Closed state)
        | '\u{1F4A5}' // 💥 (Dangerous mode)
        | '\u{1F534}' // 🔴 (Failed state)
    )
}

// ── Rendering ───────────────────────────────────────────

impl App {
    pub(crate) fn render(&mut self, frame: &mut Frame) {
        let [main_area, status_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame.area());

        let [left_area, right_area] = Layout::horizontal([
            Constraint::Length(self.layout.left_panel_width),
            Constraint::Fill(1),
        ])
        .areas(main_area);

        self.layout.last_sessions_area = left_area;
        match self.ui.left_tab {
            LeftTab::Sessions => self.render_sessions(frame, left_area),
            LeftTab::Agents => self.render_agents(frame, left_area),
        }
        self.render_right_panel(frame, right_area);
        self.render_status(frame, status_area);
    }

    fn render_sessions(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.ui.focus == Focus::Sessions && matches!(self.ui.input_mode, InputMode::Normal);

        // Apply right margin by shrinking the area
        let area = Rect {
            width: area.width.saturating_sub(2),
            ..area
        };

        // Inner width available for content (panel width minus borders and highlight symbol)
        let inner_width = area.width.saturating_sub(4) as usize; // 2 border + 2 highlight

        // Precompute which sidebar indices are indented (sessions under a label)
        let indented: Vec<bool> = {
            let mut under_label = false;
            self.sidebar_items.iter().map(|item| match item {
                SidebarItem::Label(_) => { under_label = true; false }
                SidebarItem::Session(_) => under_label,
            }).collect()
        };
        const INDENT: usize = 2;

        let drag_info = self.drag.dragging_session.as_ref().map(|ds| (ds.from_index, ds.target_index));
        let items: Vec<ListItem> = self
            .sidebar_items
            .iter()
            .enumerate()
            .map(|(sidebar_idx, item)| match item {
                SidebarItem::Label(label_id) => {
                    let label_name = self
                        .labels
                        .get(label_id)
                        .map(|s| s.as_str())
                        .unwrap_or("?");
                    // Render centered: ── Label Name ──
                    let name_width = display_width(label_name);
                    let total_pad = inner_width.saturating_sub(name_width + 2); // 2 for spaces around name
                    let left_pad = total_pad / 2;
                    let right_pad = total_pad - left_pad;
                    let line = Line::from(vec![
                        Span::styled(
                            "─".repeat(left_pad),
                            Style::new().fg(Color::DarkGray),
                        ),
                        Span::styled(
                            format!(" {} ", label_name),
                            Style::new().fg(Color::White).bold(),
                        ),
                        Span::styled(
                            "─".repeat(right_pad),
                            Style::new().fg(Color::DarkGray),
                        ),
                    ]);
                    ListItem::new(vec![line])
                }
                SidebarItem::Session(session_id) => {
                    let Some(&idx) = self.session_id_map.get(session_id) else {
                        return ListItem::new(vec![Line::from("?")]);
                    };
                    let s: &crate::session::Session = &self.sessions[idx];
                    let state = s.inferred_state();
                    let is_indented = indented[sidebar_idx];
                    let effective_width = if is_indented { inner_width.saturating_sub(INDENT) } else { inner_width };

                    let state_emoji = state.label().to_string();
                    let name_style = Style::default();

                    let mode_emoji = s.permission_mode.emoji();
                    let left_prefix_width = if mode_emoji.is_empty() { 0 } else { display_width(mode_emoji) + 1 };
                    let display_name: String = format!("{} {}", s.cli_type.emoji(), s.name);

                    let ai_label = Some(format!("{} {}", state_emoji, state.text_label()));
                    let ai_color = state.color();
                    let metadata_text = {
                        let mode_label = s.permission_mode.label();
                        mode_label.to_string()
                    };

                    let ai_label_width = ai_label.as_ref().map(|s| display_width(s.as_str())).unwrap_or(0);
                    let right_width = match (&ai_label, metadata_text.is_empty()) {
                        (Some(_), false) => ai_label_width + 1 + metadata_text.len(),
                        (Some(_), true) => ai_label_width,
                        (None, false) => metadata_text.len(),
                        (None, true) => 0,
                    };

                    let name_max = effective_width
                        .saturating_sub(left_prefix_width)
                        .saturating_sub(if right_width == 0 { 0 } else { right_width + 1 });
                    let display_name: String = {
                        let mut w = 0;
                        display_name.chars().take_while(|c| {
                            w += char_width(*c);
                            w <= name_max
                        }).collect()
                    };
                    let used = left_prefix_width + display_width(display_name.as_str()) + right_width;
                    let pad1 = effective_width.saturating_sub(used).saturating_sub(1);
                    let mut line1_spans = vec![];
                    if is_indented {
                        line1_spans.push(Span::raw(" ".repeat(INDENT)));
                    }
                    if !mode_emoji.is_empty() {
                        line1_spans.push(Span::raw(format!("{} ", mode_emoji)));
                    }
                    line1_spans.extend([
                        Span::styled(display_name, name_style),
                        Span::raw(" ".repeat(pad1)),
                    ]);
                    if let Some(ref ai) = ai_label {
                        line1_spans.push(Span::styled(
                            ai.clone(),
                            Style::new().fg(ai_color).bold(),
                        ));
                        if !metadata_text.is_empty() {
                            line1_spans.push(Span::styled(
                                format!(" {}", metadata_text),
                                Style::new().fg(Color::DarkGray),
                            ));
                        }
                    } else if !metadata_text.is_empty() {
                        line1_spans.push(Span::styled(
                            metadata_text,
                            Style::new().fg(Color::DarkGray),
                        ));
                    }
                    let line1 = Line::from(line1_spans);

                    let mut item = ListItem::new(vec![line1]);
                    if let Some((from, _to)) = drag_info {
                        if sidebar_idx == from {
                            item = item.style(Style::new().fg(Color::DarkGray));
                        }
                    }
                    item
                }
            })
            .collect();

        let border_style = if focused {
            Style::new().fg(PASTEL_CYAN)
        } else {
            Style::new().fg(PASTEL_YELLOW)
        };

        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(" Sessions ")
                    .border_type(BorderType::Rounded)
                    .border_style(border_style),
            )
            .highlight_style(Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold())
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.list_state);

        // Render scrollbar when items overflow the visible area
        let inner_height = area.height.saturating_sub(2) as usize;
        let item_count = self.sidebar_items.len();
        if item_count > inner_height && inner_height > 0 {
            let inner = area.inner(ratatui::layout::Margin::new(1, 1));
            let mut scrollbar_state = ScrollbarState::new(item_count)
                .position(self.list_state.offset());
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(Color::DarkGray))
                .track_style(Style::new().fg(Color::Rgb(40, 40, 40)));
            frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
        }
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.ui.focus == Focus::Sessions && matches!(self.ui.input_mode, InputMode::Normal);

        let items: Vec<ListItem> = self
            .agents
            .iter()
            .map(|agent| ListItem::new(Line::from(Span::raw(&agent.name))))
            .collect();

        let border_style = if focused {
            Style::new().fg(PASTEL_CYAN)
        } else {
            Style::new().fg(PASTEL_YELLOW)
        };

        let list = List::new(items)
            .block(
                Block::bordered()
                    .title(" Agents ")
                    .border_type(BorderType::Rounded)
                    .border_style(border_style)
                    .padding(Padding::right(2)),
            )
            .highlight_style(Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold())
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.agent_list_state);
    }

    fn resize_all_sessions(&mut self, rows: u16, cols: u16) {
        for session in &mut self.sessions {
            session.resize(rows, cols);
        }
    }

    fn render_right_panel(&mut self, frame: &mut Frame, area: Rect) {
        self.layout.last_right_panel_area = area;

        if self.ui.left_tab == LeftTab::Agents {
            self.render_agent_content(frame, area);
            return;
        }

        let Some(idx) = self.selected_real_index() else {
            let p = Paragraph::new("Press 'n' to start a new agent session")
                .block(
                    Block::bordered()
                        .title(" Output ")
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().fg(PASTEL_YELLOW)),
                )
                .style(Style::new().dark_gray());
            frame.render_widget(p, area);
            return;
        };

        let border_style = if self.ui.focus == Focus::Terminal {
            Style::new().fg(PASTEL_CYAN)
        } else {
            Style::new().fg(PASTEL_YELLOW)
        };

        let scroll_offset = self.sessions[idx].scroll_offset;
        let max_title = area.width.saturating_sub(4) as usize;
        let mut title_text = if let Some(cs) = &self.sessions[idx].claude_status {
            let model = if cs.model.display_name.is_empty() {
                "?"
            } else {
                &cs.model.display_name
            };
            let turn = format!("T:{}", self.sessions[idx].turn_count);
            let lines_info = if cs.cost.total_lines_added > 0 || cs.cost.total_lines_removed > 0 {
                format!(
                    "+{}/-{}",
                    cs.cost.total_lines_added, cs.cost.total_lines_removed
                )
            } else {
                String::new()
            };
            let cost = format!("${:.2}", cs.cost.total_cost_usd);
            let ctx = format!("{}%ctx", cs.context_window.used_percentage as u32);
            let mode_label = self.sessions[idx].permission_mode.label();
            let mut parts = vec![self.sessions[idx].name.clone(), model.to_string()];
            if !mode_label.is_empty() {
                parts.push(mode_label.to_string());
            }
            parts.push(turn);
            if !lines_info.is_empty() {
                parts.push(lines_info);
            }
            parts.push(cost);
            parts.push(ctx);
            format!(" {} ", parts.join(" | "))
        } else {
            format!(" {} ", self.sessions[idx].name)
        };
        if scroll_offset > 0 {
            title_text.push_str(&format!("[+{}] ", scroll_offset));
        }
        let title = if title_text.len() > max_title {
            format!("{}...", &title_text[..max_title.saturating_sub(3)])
        } else {
            title_text
        };

        let block = Block::bordered()
            .title(title)
            .border_type(BorderType::Rounded)
            .border_style(border_style);
        let inner = block.inner(area);
        self.layout.last_right_panel_inner = inner;
        frame.render_widget(block, area);

        // Reserve 1 column on the right for the scrollbar so PTY text isn't clipped
        let pty_area = Rect {
            width: inner.width.saturating_sub(1),
            ..inner
        };

        // Resize ALL sessions when panel size changes (not just the visible one)
        let new_size = (pty_area.height, pty_area.width);
        if new_size != self.layout.last_right_panel_size && pty_area.width > 0 && pty_area.height > 0 {
            self.layout.last_right_panel_size = new_size;
            self.resize_all_sessions(pty_area.height, pty_area.width);
        }

        // Apply scrollback offset before rendering
        self.sessions[idx].set_scrollback(scroll_offset);

        let mut pseudo_term = PseudoTerminal::new(self.sessions[idx].screen());
        if scroll_offset > 0 {
            pseudo_term = pseudo_term.cursor(tui_term::widget::Cursor::default().visibility(false));
        }
        frame.render_widget(pseudo_term, pty_area);

        // Reset scrollback so parser operates normally
        self.sessions[idx].set_scrollback(0);

        // Render scrollbar when there's scrollback content
        let max_scroll = self.sessions[idx].max_scrollback();
        if max_scroll > 0 {
            let mut scrollbar_state =
                ScrollbarState::new(max_scroll).position(max_scroll.saturating_sub(scroll_offset));
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_style(Style::new().fg(Color::DarkGray))
                .track_style(Style::new().fg(Color::Rgb(40, 40, 40)));
            frame.render_stateful_widget(scrollbar, inner, &mut scrollbar_state);
        }

        // Render selection highlight by swapping fg/bg colors
        if let Some(sel) = &self.drag.selection {
            let (start_row, start_col, end_row, end_col) = sel.ordered();
            let buf = frame.buffer_mut();
            for vt_row in start_row..=end_row {
                let screen_y = inner.y + vt_row;
                if screen_y >= inner.y + inner.height {
                    break;
                }
                let col_start = if vt_row == start_row { start_col } else { 0 };
                let col_end = if vt_row == end_row {
                    end_col
                } else {
                    inner.width.saturating_sub(1)
                };
                for vt_col in col_start..=col_end {
                    let screen_x = inner.x + vt_col;
                    if screen_x >= inner.x + inner.width {
                        break;
                    }
                    let cell = &mut buf[(screen_x, screen_y)];
                    cell.set_bg(Color::Rgb(50, 50, 150));
                    if cell.fg == Color::Reset {
                        cell.set_fg(Color::White);
                    }
                }
            }
        }
    }

    fn render_agent_content(&mut self, frame: &mut Frame, area: Rect) {
        let Some(sel) = self.agent_list_state.selected() else {
            let p = Paragraph::new("No agents found in agents/ directory")
                .block(
                    Block::bordered()
                        .title(" Agent ")
                        .border_type(BorderType::Rounded)
                        .border_style(Style::new().fg(PASTEL_YELLOW)),
                )
                .style(Style::new().dark_gray());
            frame.render_widget(p, area);
            return;
        };

        let agent = &self.agents[sel];
        let title = format!(" {} ", agent.name);
        let block = Block::bordered()
            .title(title)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(PASTEL_CYAN));
        let inner = block.inner(area);

        // Clamp scroll offset to content
        let line_count = agent.content.lines().count() as u16;
        let max_scroll = line_count.saturating_sub(inner.height);
        if self.agent_scroll_offset > max_scroll {
            self.agent_scroll_offset = max_scroll;
        }

        let p = Paragraph::new(agent.content.as_str())
            .scroll((self.agent_scroll_offset, 0))
            .block(block);
        frame.render_widget(p, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let (title, border_style, content) = match &self.ui.input_mode {
            InputMode::NamingSession => (
                " Session name (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.ui.input_buffer),
                ]),
            ),
            InputMode::RenamingSession => (
                " Rename session (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.ui.input_buffer),
                ]),
            ),
            InputMode::NamingLabel => (
                " Label name (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.ui.input_buffer),
                ]),
            ),
            InputMode::SelectingSessionType => {
                let style_for = |t: CliType| {
                    if self.ui.selected_cli_type == t {
                        Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold()
                    } else {
                        Style::default()
                    }
                };
                (
                    " Select type (Esc to cancel) ".to_string(),
                    Style::new().fg(PASTEL_CYAN),
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(" 1: 🤖 claude ", style_for(CliType::Claude)),
                        Span::raw(" "),
                        Span::styled(" 2: 🤖💥 claude danger-accept-permissions ", style_for(CliType::ClaudeDangerous)),
                        Span::raw(" "),
                        Span::styled(" 3: ⚡ amp ", style_for(CliType::Amp)),
                        Span::raw(" "),
                        Span::styled(" 4: 🖥️ console ", style_for(CliType::Console)),
                        Span::styled(
                            "  ←/→: switch  Enter: confirm",
                            Style::new().dark_gray(),
                        ),
                    ]),
                )
            }
            _ => {
                let show_copied = self
                    .ui
                    .copied_at
                    .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(2));

                let content = if show_copied {
                    Line::from(Span::styled(
                        " Copied to clipboard!",
                        Style::new().fg(Color::Green).bold(),
                    ))
                } else {
                    Line::from(vec![
                        Span::raw(" Shift("),
                        Span::styled("⇧", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(") + "),
                        Span::styled("←/→", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": panel switch  "),
                        Span::styled("←/→", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": tab  "),
                        Span::styled("n", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": new  "),
                        Span::styled("e", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": rename  "),
                        Span::styled("g", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": label  "),
                        Span::styled("r", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": remove  "),
                        Span::styled("q", Style::new().fg(Color::Yellow).bold()),
                        Span::raw(": quit  "),
                        Span::styled("Control(⌃) + n : global new", Style::new().fg(Color::DarkGray)),
                    ])
                };
                (
                    " neimar ".to_string(),
                    Style::new().fg(PASTEL_YELLOW),
                    content,
                )
            }
        };

        let p = Paragraph::new(content).block(
            Block::bordered()
                .title(title)
                .border_type(BorderType::Rounded)
                .border_style(border_style),
        );
        frame.render_widget(p, area);

        if matches!(
            self.ui.input_mode,
            InputMode::NamingSession
                | InputMode::RenamingSession
                | InputMode::NamingLabel
        ) {
            let x = area.x + 4 + self.ui.input_buffer.len() as u16;
            let y = area.y + 1;
            frame.set_cursor_position(Position::new(x, y));
        }
    }
}
