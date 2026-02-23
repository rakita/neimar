mod app;
mod event;
mod input;
mod mouse;
mod session;
mod ui;

use app::App;
use event::apply_event;
use session::{CliType, MAX_PTY_EVENTS_PER_FRAME};

use crossterm::event::{Event, KeyEventKind};
use std::time::Duration;
use tokio::sync::mpsc;

/// Drain pending keys and PTY events, poll status files, render.
fn process_frame(
    app: &mut App,
    key_rx: &mut mpsc::UnboundedReceiver<Event>,
    rx: &mut mpsc::UnboundedReceiver<event::AppEvent>,
    terminal: &mut ratatui::DefaultTerminal,
) -> std::io::Result<()> {
    while let Ok(ev) = key_rx.try_recv() {
        match ev {
            Event::Key(key) => app.handle_key(key),
            Event::Mouse(mouse) => app.handle_mouse(mouse),
            _ => {}
        }
        if app.should_quit {
            break;
        }
    }
    for _ in 0..MAX_PTY_EVENTS_PER_FRAME {
        match rx.try_recv() {
            Ok(ev) => apply_event(app, ev),
            Err(_) => break,
        }
    }
    if !app.should_quit {
        app.poll_status_files();
        app.check_pending_ralph_commands();
        terminal.draw(|frame| app.render(frame))?;
    }
    Ok(())
}

// ── Main ────────────────────────────────────────────────

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let result = run(&mut terminal).await;
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
    ratatui::restore();
    result
}

async fn run(terminal: &mut ratatui::DefaultTerminal) -> std::io::Result<()> {
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;

    let (key_tx, mut key_rx) = mpsc::unbounded_channel::<Event>();
    let (tx, mut rx) = mpsc::unbounded_channel::<event::AppEvent>();

    // Dedicated OS thread for keyboard/mouse reading — never blocked by tokio scheduler
    std::thread::spawn(move || {
        loop {
            match crossterm::event::read() {
                Ok(Event::Key(key))
                    if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat =>
                {
                    if key_tx.send(Event::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(Event::Mouse(mouse)) => {
                    if key_tx.send(Event::Mouse(mouse)).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });

    let mut app = App::new(tx);
    terminal.draw(|frame| app.render(frame))?;
    let (rows, cols) = if app.last_right_panel_size.0 > 0 {
        app.last_right_panel_size
    } else {
        (24, 80)
    };
    app.create_session("Session 1".to_string(), CliType::Claude, rows, cols);
    let mut render_interval = tokio::time::interval(Duration::from_millis(33));
    render_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;

            // Priority 1: input events (key + mouse)
            Some(ev) = key_rx.recv() => {
                match ev {
                    Event::Key(key) => app.handle_key(key),
                    Event::Mouse(mouse) => app.handle_mouse(mouse),
                    _ => {}
                }
                process_frame(&mut app, &mut key_rx, &mut rx, terminal)?;
                render_interval.reset();
            }

            // Priority 2: render tick
            _ = render_interval.tick() => {
                process_frame(&mut app, &mut key_rx, &mut rx, terminal)?;
            }
        }

        if app.should_quit {
            break;
        }
    }

    app.shutdown();
    Ok(())
}
