use crate::app::App;
use crate::session::SessionStatus;
use std::time::Instant;

pub(crate) enum AppEvent {
    PtyOutput(usize, Vec<u8>),
    PtyExited(usize),
    SummaryResult(usize, String),
}

pub(crate) fn apply_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::PtyOutput(id, bytes) => {
            if let Some(&idx) = app.session_id_map.get(&id) {
                if let Some(session) = app.sessions.get_mut(idx) {
                    session.parser.process(&bytes);
                    session.last_pty_output = Some(Instant::now());
                }
            }
        }
        AppEvent::PtyExited(id) => {
            if let Some(&idx) = app.session_id_map.get(&id)
                && let Some(session) = app.sessions.get_mut(idx)
            {
                session.status = SessionStatus::Completed;
                session.pty_writer = None;
                session.pending_ralph_command = None;
            }
        }
        AppEvent::SummaryResult(id, text) => {
            if let Some(&idx) = app.session_id_map.get(&id)
                && let Some(session) = app.sessions.get_mut(idx)
            {
                session.summary_pending = false;
                if !text.is_empty() {
                    session.summary = Some(text);
                }
            }
        }
    }
}
