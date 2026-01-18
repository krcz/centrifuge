mod app;
mod input;
mod ui;

use std::io;
use std::time::Duration;

use cid::Cid;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use silane_openrouter::OpenRouterClient;

pub use app::ChatApp;

use crate::error::SihError;
use crate::store::AppContext;

pub async fn run(
    ctx: AppContext,
    client: OpenRouterClient,
    model: String,
    reasoning_effort: Option<String>,
    continue_from: Option<Cid>,
) -> Result<(), SihError> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = ChatApp::new(ctx, client, model, reasoning_effort, continue_from)?;

    // Run event loop
    let result = run_loop(&mut terminal, &mut app).await;

    // Get final conversation CID before restoring terminal
    let final_cid = app.conversation_cid();

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Print final conversation CID
    if let Some(cid) = final_cid {
        println!("Conversation CID: {}", cid);
        println!("To continue: sih chat --continue-from {}", cid);
    }

    result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app: &mut ChatApp,
) -> Result<(), SihError> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // Poll for events with timeout to allow checking async responses
        if event::poll(Duration::from_millis(50))? {
            let event = event::read()?;
            input::handle_event(app, event);
        }

        // Check for async response
        app.poll_response();

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
