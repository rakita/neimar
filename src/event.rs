use crate::app::App;
use crate::session::{AiState, PermissionMode, Session, SessionStatus};
use std::time::Instant;

pub(crate) enum AppEvent {
    PtyOutput(usize, Vec<u8>),
    PtyExited(usize),
    SummaryResult(usize, String),
}

pub(crate) fn apply_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::PtyOutput(id, bytes) => {
            if let Some(&idx) = app.session_id_map.get(&id)
                && let Some(session) = app.sessions.get_mut(idx)
                && session.id == id
            {
                // New output after idle: clear stale AI state so it gets re-classified
                if !session.is_actively_working() {
                    session.ai_state = None;
                    session.forced_summary_count = 0;
                }
                session.parser.process(&bytes);
                session.last_pty_output = Some(Instant::now());
                let detected = Session::detect_permission_mode_from_bytes(&bytes);
                if detected != PermissionMode::Unknown {
                    session.permission_mode = detected;
                }
            }
        }
        AppEvent::PtyExited(id) => {
            if let Some(&idx) = app.session_id_map.get(&id)
                && let Some(session) = app.sessions.get_mut(idx)
                && session.id == id
            {
                session.status = SessionStatus::Completed;
                session.pty_writer = None;
                session.pending_ralph_command = None;
            }
        }
        AppEvent::SummaryResult(id, text) => {
            if let Some(session) = app.sessions.iter_mut().find(|s| s.id == id) {
                session.summary_pending = false;
                if !text.is_empty() {
                    if let Some(space_idx) = text.find(' ') {
                        let first_word = &text[..space_idx];
                        if let Some(state) = AiState::parse(first_word) {
                            session.ai_state = Some(state);
                            session.summary = Some(text[space_idx + 1..].to_string());
                        } else {
                            session.summary = Some(text);
                        }
                    } else if let Some(state) = AiState::parse(&text) {
                        session.ai_state = Some(state);
                    } else {
                        session.summary = Some(text);
                    }
                }
            }
        }
    }
}
