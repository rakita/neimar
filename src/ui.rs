use crate::app::{App, Focus, InputMode, LeftTab};
use crate::session::{CliType, SessionState, PREVIEW_LINE_COUNT};
use portable_pty::PtySize;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, List, ListItem, Padding, Paragraph},
};
use tui_term::widget::PseudoTerminal;

const PASTEL_CYAN: Color = Color::Rgb(150, 220, 235);
const PASTEL_YELLOW: Color = Color::Rgb(255, 255, 150);

// ── Rendering ───────────────────────────────────────────

impl App {
    pub(crate) fn render(&mut self, frame: &mut Frame) {
        let [main_area, status_area] =
            Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(frame.area());

        let [left_area, right_area] =
            Layout::horizontal([Constraint::Length(38), Constraint::Fill(1)]).areas(main_area);

        self.last_sessions_area = left_area;
        match self.left_tab {
            LeftTab::Sessions => self.render_sessions(frame, left_area),
            LeftTab::Agents => self.render_agents(frame, left_area),
        }
        self.render_right_panel(frame, right_area);
        self.render_status(frame, status_area);
    }

    fn render_sessions(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sessions && matches!(self.input_mode, InputMode::Normal);

        // Inner width available for content (panel width minus borders and highlight symbol)
        let inner_width = area.width.saturating_sub(6) as usize; // 2 border + 2 highlight + 2 padding

        let vis = self.visible_sessions();
        let items: Vec<ListItem> = vis
            .iter()
            .map(|&idx| {
                let s = &self.sessions[idx];
                let state = s.inferred_state();

                let icon = if s.ralph_loop {
                    "🔁"
                } else {
                    match state {
                        SessionState::Working => "🔥",
                        SessionState::Prompt => "☕",
                        SessionState::Starting => "⏳",
                        SessionState::Done => "💯",
                        SessionState::Failed => "🫠",
                        SessionState::Archived => "📦",
                    }
                };

                let name_style = if state == SessionState::Archived {
                    Style::new().dark_gray()
                } else {
                    Style::default()
                };

                // Line 1: icon + name (left) + lines changed (right-aligned, cyan)
                let icon_name = format!("{} {}", icon, s.name);
                let lines_text = s.lines_changed_display().unwrap_or_default();
                let pad1 = inner_width.saturating_sub(icon_name.chars().count() + lines_text.len());
                let mut line1_spans = vec![
                    Span::styled(icon_name, name_style),
                    Span::raw(" ".repeat(pad1)),
                ];
                if !lines_text.is_empty() {
                    line1_spans.push(Span::styled(lines_text, Style::new().fg(Color::Cyan)));
                }
                let line1 = Line::from(line1_spans);

                // Line 2: context bar + pct + turn count
                // Preview lines: last output (dimmed, wrapped to fit width)
                let previews = s.last_output_lines(PREVIEW_LINE_COUNT);
                let full_text = previews.join(" ");
                let max_len = inner_width.saturating_sub(1); // 1 char left indent
                let mut preview_lines: Vec<Line> = Vec::new();
                if !full_text.is_empty() && max_len > 0 {
                    let chars: Vec<char> = full_text.chars().collect();
                    let mut pos = 0;
                    while pos < chars.len() && preview_lines.len() < PREVIEW_LINE_COUNT {
                        let end = (pos + max_len).min(chars.len());
                        let chunk: String = chars[pos..end].iter().collect();
                        preview_lines.push(Line::from(Span::styled(
                            format!(" {}", chunk),
                            Style::new().dark_gray(),
                        )));
                        pos = end;
                    }
                }
                // Pad to PREVIEW_LINE_COUNT for consistent item height
                while preview_lines.len() < PREVIEW_LINE_COUNT {
                    preview_lines.push(Line::from(Span::raw("")));
                }

                let line2 = if s.claude_status.is_some() {
                    let bar_width = 10;
                    let (bar, bar_color) = s.context_bar(bar_width);
                    let pct = s
                        .claude_status
                        .as_ref()
                        .unwrap()
                        .context_window
                        .used_percentage as u32;
                    let pct_str = format!("{}%", pct);
                    let turn_str = format!("T:{}", s.turn_count);
                    let mode_label = s.permission_mode.label();
                    let left_part = format!(" {} {:>3}  {:<5}", bar, pct_str, turn_str);
                    let mut line2_spans = vec![
                        Span::raw(" "),
                        Span::styled(bar, Style::new().fg(bar_color)),
                        Span::raw(format!(" {:>3}  {:<5}", pct_str, turn_str)),
                    ];
                    if !mode_label.is_empty() {
                        let pad2 = inner_width.saturating_sub(left_part.chars().count() + mode_label.len());
                        line2_spans.push(Span::raw(" ".repeat(pad2)));
                        line2_spans.push(Span::styled(
                            mode_label,
                            Style::new().fg(s.permission_mode.color()),
                        ));
                    }
                    Line::from(line2_spans)
                } else {
                    Line::from(Span::raw(""))
                };

                let mut lines = vec![line1, line2];
                lines.extend(preview_lines);
                ListItem::new(lines)
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
                    .border_style(border_style)
                    .padding(Padding::right(2)),
            )
            .highlight_style(Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold())
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_agents(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sessions && matches!(self.input_mode, InputMode::Normal);

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
            if (rows, cols) != session.last_size {
                session.parser.screen_mut().set_size(rows, cols);
                if let Some(master) = &session.pty_master {
                    let _ = master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    });
                }
                session.last_size = (rows, cols);
            }
        }
    }

    fn render_right_panel(&mut self, frame: &mut Frame, area: Rect) {
        self.last_right_panel_area = area;

        if self.left_tab == LeftTab::Agents {
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

        let border_style = if self.focus == Focus::Terminal {
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
        self.last_right_panel_inner = inner;
        frame.render_widget(block, area);

        // Resize ALL sessions when panel size changes (not just the visible one)
        let new_size = (inner.height, inner.width);
        if new_size != self.last_right_panel_size && inner.width > 0 && inner.height > 0 {
            self.last_right_panel_size = new_size;
            self.resize_all_sessions(inner.height, inner.width);
        }

        // Apply scrollback offset before rendering
        self.sessions[idx]
            .parser
            .screen_mut()
            .set_scrollback(scroll_offset);

        let mut pseudo_term = PseudoTerminal::new(self.sessions[idx].parser.screen());
        if scroll_offset > 0 {
            pseudo_term = pseudo_term.cursor(tui_term::widget::Cursor::default().visibility(false));
        }
        frame.render_widget(pseudo_term, inner);

        // Reset scrollback so parser operates normally
        self.sessions[idx].parser.screen_mut().set_scrollback(0);

        // Render selection highlight by swapping fg/bg colors
        if let Some(sel) = &self.selection {
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
                    let fg = cell.fg;
                    let bg = cell.bg;
                    cell.set_fg(bg);
                    cell.set_bg(fg);
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
        let (title, border_style, content) = match &self.input_mode {
            InputMode::NamingSession => (
                " Session name (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.input_buffer),
                ]),
            ),
            InputMode::RenamingSession => (
                " Rename session (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.input_buffer),
                ]),
            ),
            InputMode::NamingRalph => (
                " Ralph loop name (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.input_buffer),
                ]),
            ),
            InputMode::EnteringRalphPrompt => (
                " Ralph prompt (Esc to cancel) ".to_string(),
                Style::new().fg(PASTEL_CYAN),
                Line::from(vec![
                    Span::styled(" > ", Style::new().fg(PASTEL_CYAN).bold()),
                    Span::raw(&self.input_buffer),
                ]),
            ),
            InputMode::SelectingSessionType => {
                let claude_style = if self.selected_cli_type == CliType::Claude {
                    Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold()
                } else {
                    Style::default()
                };
                let amp_style = if self.selected_cli_type == CliType::Amp {
                    Style::new().bg(PASTEL_CYAN).fg(Color::Black).bold()
                } else {
                    Style::default()
                };
                (
                    " Select type (Esc to cancel) ".to_string(),
                    Style::new().fg(PASTEL_CYAN),
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(" 1: 🤖 claude ", claude_style),
                        Span::raw("  "),
                        Span::styled(" 2: ⚡ amp ", amp_style),
                        Span::styled(
                            "    ↑↓/Tab: switch  Enter: confirm",
                            Style::new().dark_gray(),
                        ),
                    ]),
                )
            }
            _ => {
                let mut spans = vec![
                    Span::styled(
                        " Shift(⇧)+Left(←)/Right(→)",
                        Style::new().fg(Color::Yellow).bold(),
                    ),
                    Span::raw(": panel  "),
                    Span::styled("←/→", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": tab  "),
                    Span::styled("n", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": new  "),
                    Span::styled("l", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": ralph  "),
                    Span::styled("e", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": rename  "),
                    Span::styled("r", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": remove  "),
                    Span::styled("x", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": archive  "),
                    Span::styled("a", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": toggle archived  "),
                    Span::styled("j/k", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": navigate  "),
                    Span::styled("q", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": quit  "),
                    Span::styled("Shift+PgUp/PgDn", Style::new().fg(Color::Yellow).bold()),
                    Span::raw(": scroll"),
                ];
                if self.show_archived {
                    spans.push(Span::styled(
                        "  [showing archived]",
                        Style::new().dark_gray(),
                    ));
                }
                (
                    " neimar ".to_string(),
                    Style::new().fg(PASTEL_YELLOW),
                    Line::from(spans),
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
            self.input_mode,
            InputMode::NamingSession
                | InputMode::RenamingSession
                | InputMode::NamingRalph
                | InputMode::EnteringRalphPrompt
        ) {
            let x = area.x + 4 + self.input_buffer.len() as u16;
            let y = area.y + 1;
            frame.set_cursor_position(Position::new(x, y));
        }
    }
}
