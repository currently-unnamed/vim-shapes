mod archimate_import;
mod clean;
mod config;
mod drawio_export;
mod drawio_import;
mod export;
mod fonts;
mod foundry_import;
mod layout;
mod model;
mod ontology;
mod persistence;
mod registry;
mod render;
mod shapes;
mod ui;
mod workbench;

use std::io::{self, Stdout};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen};
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the terminal was asked for the kitty keyboard protocol — so the teardown, which
/// the panic hook also runs, knows whether there is anything to pop.
static ENHANCED: AtomicBool = AtomicBool::new(false);
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use ui::App;

const USAGE: &str = "\
vim-shapes — architecture diagrams, driven like vim

  vim-shapes [file.json]        open the app (empty, or on a file)
  vim-shapes --light | --dark   the palette for this run; :theme sets and keeps one
  vim-shapes --check <file>     lint a file and exit
  vim-shapes --ontology         the whole ontology, as JSON, and exit
  vim-shapes --render <file> <out.png> [--tab N] [--px 20] [--font <path>]
                                render one tab to a PNG, through a monospace font, and exit
";

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--help" | "-h") => {
            print!("{USAGE}");
            return Ok(());
        }
        Some("--ontology") => {
            println!("{}", serde_json::to_string_pretty(&ontology::emit::spec()).expect("the spec serializes"));
            return Ok(());
        }
        Some("--render") => {
            let (Some(file), Some(out)) = (args.get(1), args.get(2)) else {
                eprintln!("usage: vim-shapes --render <file> <out.png> [--tab N] [--px 20] [--font <path>]");
                std::process::exit(2);
            };
            let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
            let ws = match persistence::load(std::path::Path::new(file)) {
                Ok(ws) => ws,
                Err(e) => {
                    eprintln!("{file}: {e}");
                    std::process::exit(1);
                }
            };
            let tab = flag("--tab").and_then(|n| n.parse::<usize>().ok()).map(|n| n.saturating_sub(1)).unwrap_or(ws.current);
            let Some(t) = ws.tabs.get(tab) else {
                eprintln!("no tab {} — the file has {}", tab + 1, ws.tabs.len());
                std::process::exit(1);
            };
            let px: f32 = flag("--px").and_then(|p| p.parse().ok()).unwrap_or(20.0);
            let o = export::Options { grid: ws.grid, font: flag("--font"), ..export::Options::default() };
            return match render::to_png(&t.diagram, &o, px, std::path::Path::new(out)) {
                Ok((cw, ch, w, h)) => {
                    println!("{}: {cw}x{ch} cells → {w}x{h} px → {out}", t.name);
                    Ok(())
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            };
        }
        Some("--check") => {
            let Some(path) = args.get(1) else {
                eprintln!("usage: vim-shapes --check <file>");
                std::process::exit(2);
            };
            return match persistence::load(std::path::Path::new(path)) {
                Ok(ws) => {
                    let mut clean = true;
                    for t in &ws.tabs {
                        let report = ontology::emit::report(&t.diagram);
                        if ws.tabs.len() > 1 {
                            println!("tab {}:", t.name);
                        }
                        print!("{report}");
                        clean &= report.starts_with("ok:");
                    }
                    if clean { Ok(()) } else { std::process::exit(1) }
                }
                Err(e) => {
                    eprintln!("{path}: {e}");
                    std::process::exit(1);
                }
            };
        }
        _ => {}
    }

    // The palette: a flag, the config file, the terminal's hint, or dark.
    let flag = args.iter().find_map(|a| match a.as_str() {
        "--light" => Some(ui::theme::Mode::Light),
        "--dark" => Some(ui::theme::Mode::Dark),
        _ => None,
    });
    let config = config::load();
    let (mode, from) = ui::theme::choose(flag, config.theme.as_deref(), std::env::var("COLORFGBG").ok().as_deref());
    ui::theme::set_mode(mode, from);
    if let Some(i) = config.ink.as_deref().and_then(ui::wire::Ink::parse) {
        ui::wire::set_ink(i);
    }
    let args: Vec<String> = args.into_iter().filter(|a| a != "--light" && a != "--dark").collect();

    install_panic_hook();
    let mut terminal = init_terminal()?;

    let mut app = App::new();
    app.enhanced_keys = ENHANCED.load(Ordering::Relaxed);
    if let Some(path) = args.first() {
        // A file on the command line skips the title screen and the start dialog: you asked
        // for the diagram.
        app.loading = false;
        if let Err(e) = app.open_path(std::path::PathBuf::from(path)) {
            restore_terminal()?;
            eprintln!("{path}: {e}");
            std::process::exit(1);
        }
    }

    let result = run(&mut terminal, &mut app);
    restore_terminal()?;
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.on_key(key),
            Event::Mouse(m) => app.on_mouse(m),
            _ => {}
        }
        if app.should_quit {
            app.persist_workbench_session();
            return Ok(());
        }
    }
}

fn init_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    // A plain terminal sends shift+ctrl+h as the same byte as ctrl+h, and the app has a
    // move on one and a linked shape on the other. The kitty keyboard protocol tells them
    // apart; ask for it where the terminal has it, and the app knows either way.
    if supports_keyboard_enhancement().unwrap_or(false)
        && execute!(stdout, PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)).is_ok()
    {
        ENHANCED.store(true, Ordering::Relaxed);
    }
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal() -> io::Result<()> {
    if ENHANCED.swap(false, Ordering::Relaxed) {
        execute!(io::stdout(), PopKeyboardEnhancementFlags)?;
    }
    disable_raw_mode()?;
    execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen)?;
    Ok(())
}

/// Leaves the terminal sane even if the app panics, instead of leaving the shell stuck in raw
/// mode on the alternate screen.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));
}
