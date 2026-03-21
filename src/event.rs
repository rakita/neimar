use crate::app::App;
use crate::types::AppEvent;

pub(crate) fn apply_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::PtyOutput(id, bytes) => {
            if let Some(session) = app.session_by_id_mut(id) {
                session.process_pty_output(&bytes);
            }
        }
        AppEvent::PtyExited(id) => {
            if let Some(session) = app.session_by_id_mut(id) {
                session.mark_exited();
            }
        }
    }
}
