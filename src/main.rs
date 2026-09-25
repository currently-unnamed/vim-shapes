mod app;
mod commands;
mod drawio_export;
mod mode;
mod model;
mod persistence;
mod render;
mod shapes;

use std::io::{self, Stdout};

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> io::Result<()> {
    install_panic_hook();
    let mut terminal = init_terminal()?;

    let mut app = App::default();
    if let Some(path) = std::env::args().nth(1) {
        match persistence::load(std::path::Path::new(&path)) {
            Ok(doc) => {
                app.document = doc;
                app.last_saved_path = Some(std::path::PathBuf::from(path));
            }
            Err(e) => app.status_message = Some(format!("could not load: {e}")),
        }
    }

    let result = run(&mut terminal, &mut app);

    restore_terminal()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| render::draw(frame, app))?;

        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press {
                app.status_message = None;
                mode::handle_key(app, key);
            }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

/// Ensures the terminal is left in a sane state even if the app panics,
/// instead of leaving the user's shell stuck in raw mode / the alt screen.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));
}
