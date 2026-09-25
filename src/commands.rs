use std::path::PathBuf;

use crate::app::App;
use crate::{drawio_export, persistence};

pub fn execute(app: &mut App, buffer: &str) {
    let buffer = buffer.trim();
    let (name, rest) = match buffer.split_once(' ') {
        Some((n, r)) => (n, r.trim()),
        None => (buffer, ""),
    };

    match name {
        "w" => {
            write_native(app, rest);
        }
        "q" => app.should_quit = true,
        "wq" => {
            if write_native(app, rest) {
                app.should_quit = true;
            }
        }
        "export" => export_drawio(app, rest),
        "" => {}
        other => app.status_message = Some(format!("unknown command: {other}")),
    }
}

fn write_native(app: &mut App, path_arg: &str) -> bool {
    let path = match resolve_path(app, path_arg) {
        Some(p) => p,
        None => {
            app.status_message = Some("no file path given (use :w <path>)".to_string());
            return false;
        }
    };

    match persistence::save(&app.document, &path) {
        Ok(()) => {
            app.status_message = Some(format!("wrote {}", path.display()));
            app.last_saved_path = Some(path);
            true
        }
        Err(e) => {
            app.status_message = Some(format!("write failed: {e}"));
            false
        }
    }
}

fn export_drawio(app: &mut App, path_arg: &str) {
    if path_arg.is_empty() {
        app.status_message = Some("usage: :export <path.drawio>".to_string());
        return;
    }
    let path = PathBuf::from(path_arg);
    match drawio_export::export(&app.document, &path) {
        Ok(()) => app.status_message = Some(format!("exported {}", path.display())),
        Err(e) => app.status_message = Some(format!("export failed: {e}")),
    }
}

fn resolve_path(app: &App, path_arg: &str) -> Option<PathBuf> {
    if !path_arg.is_empty() {
        Some(PathBuf::from(path_arg))
    } else {
        app.last_saved_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q_sets_should_quit() {
        let mut app = App::default();
        execute(&mut app, "q");
        assert!(app.should_quit);
    }

    #[test]
    fn unknown_command_sets_a_status_message_without_panicking() {
        let mut app = App::default();
        execute(&mut app, "bogus");
        assert!(app.status_message.unwrap().contains("unknown command"));
    }

    #[test]
    fn w_without_a_path_and_no_prior_save_reports_an_error() {
        let mut app = App::default();
        execute(&mut app, "w");
        assert!(app.status_message.unwrap().contains("no file path"));
    }

    #[test]
    fn w_then_wq_reuses_the_last_saved_path() {
        let mut app = App::default();
        let mut path = std::env::temp_dir();
        path.push(format!("vim-shapes-cmd-test-{}.json", std::process::id()));
        execute(&mut app, &format!("w {}", path.display()));
        assert!(app.last_saved_path.is_some());

        execute(&mut app, "wq");
        std::fs::remove_file(&path).ok();
        assert!(app.should_quit);
    }

    #[test]
    fn export_without_a_path_reports_usage_error() {
        let mut app = App::default();
        execute(&mut app, "export");
        assert!(app.status_message.unwrap().contains("usage"));
    }
}
