mod app;
mod events;
mod mock_data;
mod terminal;
mod ui;

use color_eyre::{eyre::Result, install};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, panic};

use app::App;
use events::handle_events;
use terminal::{restore_terminal, setup_terminal};
use ui::render_ui;

fn main() -> Result<()> {
    // Install color_eyre for better error handling
    install()?;
    
    // Set up panic hook to restore terminal
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        let _ = restore_terminal();
        original_hook(panic);
    }));

    // Setup terminal for interactive mode
    setup_terminal()?;
    
    // Create app and run it
    let app = App::default();
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let result = run_app(&mut terminal, app);

    // Always restore terminal
    restore_terminal()?;
    
    result
}

fn run_app<B: ratatui::backend::Backend>(terminal: &mut Terminal<B>, mut app: App) -> Result<()> {
    loop {
        terminal.draw(|f| render_ui(f, &app))?;

        handle_events(&mut app)?;
        app.tick();

        if app.should_quit {
            break;
        }
    }
    Ok(())
} 