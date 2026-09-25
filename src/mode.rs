use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Focus, Mode};
use crate::commands;

pub fn handle_key(app: &mut App, key: KeyEvent) {
    // Ctrl+W Ctrl+W (double-tap) toggles sidebar/canvas focus, mirroring
    // vim's own <C-w><C-w> window-cycle mapping. Only live in Normal mode -
    // Insert/Command treat it as an ordinary (unbound) key so it can't
    // yank focus away mid-edit.
    if app.mode == Mode::Normal && is_ctrl_w(&key) {
        if app.ctrl_w_pending {
            app.toggle_focus();
            app.ctrl_w_pending = false;
        } else {
            app.ctrl_w_pending = true;
        }
        return;
    }
    app.ctrl_w_pending = false;

    if key.code == KeyCode::Esc {
        handle_escape(app);
        return;
    }

    match app.mode.clone() {
        Mode::Insert => handle_insert(app, key),
        Mode::Command { buffer } => handle_command(app, key, buffer),
        Mode::Visual => handle_visual(app, key),
        Mode::Normal => handle_normal(app, key),
    }
}

fn is_ctrl_w(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('w')
}

fn handle_escape(app: &mut App) {
    if app.pending_connect.is_some() {
        app.cancel_connect();
        return;
    }
    match &app.mode {
        Mode::Insert => app.commit_label_edit(),
        Mode::Command { .. } => app.mode = Mode::Normal,
        Mode::Visual => app.mode = Mode::Normal,
        Mode::Normal => {}
    }
}

fn handle_normal(app: &mut App, key: KeyEvent) {
    if let KeyCode::Char(c) = key.code {
        if c == ':' {
            app.mode = Mode::Command { buffer: String::new() };
            return;
        }
        if c.is_ascii_digit() && (c != '0' || app.pending_count.is_some()) {
            let digit = c as u32 - '0' as u32;
            app.pending_count = Some(app.pending_count.unwrap_or(0) * 10 + digit);
            return;
        }
    }

    match app.focus {
        Focus::Sidebar => handle_sidebar_normal(app, key),
        Focus::Canvas => handle_canvas_normal(app, key),
    }
}

fn handle_sidebar_normal(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') => {
            let max = crate::model::ShapeKind::ALL.len() - 1;
            app.sidebar_index = (app.sidebar_index + 1).min(max);
        }
        KeyCode::Char('k') => {
            app.sidebar_index = app.sidebar_index.saturating_sub(1);
        }
        KeyCode::Enter => app.place_selected_shape(),
        _ => {}
    }
    app.pending_count = None;
}

fn handle_canvas_normal(app: &mut App, key: KeyEvent) {
    if app.pending_connect.is_some() {
        handle_connect_pending(app, key);
        return;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let count = app.take_count();
    match key.code {
        KeyCode::Char('h') if ctrl => app.move_focused(-1.0, 0.0),
        KeyCode::Char('l') if ctrl => app.move_focused(1.0, 0.0),
        KeyCode::Char('k') if ctrl => app.move_focused(0.0, -1.0),
        KeyCode::Char('j') if ctrl => app.move_focused(0.0, 1.0),
        KeyCode::Char('h') => (0..count).for_each(|_| app.navigate(-1.0, 0.0)),
        KeyCode::Char('l') => (0..count).for_each(|_| app.navigate(1.0, 0.0)),
        KeyCode::Char('k') => (0..count).for_each(|_| app.navigate(0.0, -1.0)),
        KeyCode::Char('j') => (0..count).for_each(|_| app.navigate(0.0, 1.0)),
        KeyCode::Char('c') if app.focused_node.is_some() => app.start_connect(),
        KeyCode::Char('i') if app.focused_node.is_some() => app.start_label_edit(),
        KeyCode::Char('x') if app.focused_node.is_some() => app.delete_focused(),
        KeyCode::Char('v') => app.mode = Mode::Visual,
        _ => {}
    }
}

fn handle_connect_pending(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') => app.move_connect_target(-1.0, 0.0),
        KeyCode::Char('l') => app.move_connect_target(1.0, 0.0),
        KeyCode::Char('k') => app.move_connect_target(0.0, -1.0),
        KeyCode::Char('j') => app.move_connect_target(0.0, 1.0),
        KeyCode::Enter => app.confirm_connect(),
        _ => {}
    }
}

fn handle_insert(app: &mut App, key: KeyEvent) {
    // Letters edit the label; arrow keys nudge the shape's edges (resize),
    // since hjkl must stay available as literal label text here.
    match key.code {
        KeyCode::Char(c) => app.insert_scratch.push(c),
        KeyCode::Backspace => {
            app.insert_scratch.pop();
        }
        KeyCode::Right => app.resize_focused(1.0, 0.0),
        KeyCode::Left => app.resize_focused(-1.0, 0.0),
        KeyCode::Down => app.resize_focused(0.0, 1.0),
        KeyCode::Up => app.resize_focused(0.0, -1.0),
        _ => {}
    }
}

fn handle_visual(app: &mut App, _key: KeyEvent) {
    // Mode exists and is enterable/exitable per the M1 vertical slice scope;
    // bulk operations are deferred to a later milestone.
    let _ = app;
}

fn handle_command(app: &mut App, key: KeyEvent, mut buffer: String) {
    match key.code {
        KeyCode::Char(c) => {
            buffer.push(c);
            app.mode = Mode::Command { buffer };
        }
        KeyCode::Backspace => {
            if buffer.is_empty() {
                app.mode = Mode::Normal;
            } else {
                buffer.pop();
                app.mode = Mode::Command { buffer };
            }
        }
        KeyCode::Enter => {
            app.mode = Mode::Normal;
            commands::execute(app, &buffer);
        }
        _ => {
            app.mode = Mode::Command { buffer };
        }
    }
}
