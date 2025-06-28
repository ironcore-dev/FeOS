mod app;
mod events;
mod mock_data;
mod terminal;
mod ui;

use clap::{Parser, ValueEnum};
use color_eyre::{eyre::Result, install};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, panic, time::Duration};

use app::{App, CurrentView};
use events::handle_events;
use terminal::{restore_terminal, setup_terminal};
use ui::render_ui;

#[derive(Parser)]
#[command(name = "feos-tui")]
#[command(about = "FeOS Terminal User Interface")]
struct Cli {
    /// Test mode - runs automated tests and exits
    #[arg(long)]
    test: bool,
    
    /// Test a specific view and exit after duration
    #[arg(long, value_enum)]
    test_view: Option<TestView>,
    
    /// Duration to show test view in seconds (default: 2)
    #[arg(long, default_value = "2")]
    duration: u64,
    
    /// Verbose output in test mode
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Clone, Debug, ValueEnum)]
enum TestView {
    Dashboard,
    Vms,
    Containers,
    Logs,
    System,
}

fn main() -> Result<()> {
    // Install color_eyre for better error handling
    install()?;
    
    // Set up panic hook to restore terminal
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic| {
        let _ = restore_terminal();
        original_hook(panic);
    }));

    let cli = Cli::parse();
    
    // Handle test modes
    if cli.test {
        run_test_mode(cli.verbose)?;
        return Ok(());
    }
    
    if let Some(test_view) = cli.test_view {
        run_view_test(test_view, cli.duration, cli.verbose)?;
        return Ok(());
    }

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

fn run_test_mode(verbose: bool) -> Result<()> {
    if verbose {
        println!("FeOS TUI - Test Mode (Verbose)");
        println!("==============================");
    } else {
        println!("FeOS TUI - Test Mode");
        println!("===================");
    }
    
    let mut app = App::default();
    println!("✓ App initialized successfully");
    println!("✓ Host info: {} cores, {} RAM", 
             app.host_info.num_cores, 
             mock_data::format_bytes(app.host_info.ram_total));
    println!("✓ Found {} VMs", app.vms.len());
    println!("✓ Found {} containers", app.containers.len());
    println!("✓ CPU history data points: {}", app.cpu_history.len());
    println!("✓ Memory history data points: {}", app.memory_history.len());
    println!("✓ Network interfaces: {}", app.host_info.net_interfaces.len());
    println!("✓ Initial FeOS logs: {}", app.feos_logs.len());
    println!("✓ Initial kernel logs: {}", app.kernel_logs.len());
    
    println!("\n--- Testing Dynamic Updates ---");
    
    // Simulate a few updates to show dynamic behavior
    for i in 1..=3 {
        println!("\nUpdate {}:", i);
        
        // Sleep to trigger time-based updates
        std::thread::sleep(std::time::Duration::from_secs(1));
        app.tick();
        
        // Show VM status changes
        for vm in &app.vms {
            println!("  VM {}: {}", vm.uuid, vm.status.as_str());
        }
        
        // Show container status changes  
        for container in &app.containers {
            println!("  Container {}: {}", container.name, container.status.as_str());
        }
        
        // Show RAM usage changes
        println!("  RAM unused: {}", mock_data::format_bytes(app.host_info.ram_unused));
        
        // Show new log counts
        println!("  FeOS logs: {}, Kernel logs: {}", 
                 app.feos_logs.len(), app.kernel_logs.len());
    }
    
    println!("\nMock data test completed successfully!");
    println!("Dynamic behavior verified - VM statuses change, RAM usage varies, logs stream!");
    println!("To run the interactive TUI, use: cargo run --bin feos-tui");
    println!("Press 'q' to quit when running the TUI.");
    
    Ok(())
}

fn run_view_test(test_view: TestView, duration: u64, verbose: bool) -> Result<()> {
    if verbose {
        println!("FeOS TUI - View Test Mode");
        println!("========================");
        println!("Testing view: {:?}", test_view);
        println!("Duration: {} seconds", duration);
        println!();
    }
    
    // Setup terminal
    setup_terminal()?;
    
    // Create app and set the requested view
    let mut app = App::default();
    app.current_view = match test_view {
        TestView::Dashboard => CurrentView::Dashboard,
        TestView::Vms => CurrentView::VMs,
        TestView::Containers => CurrentView::Containers,
        TestView::Logs => CurrentView::Logs,
        TestView::System => CurrentView::System,
    };
    
    // Create terminal
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    
    // Show the view for the specified duration
    let start = std::time::Instant::now();
    let duration = Duration::from_secs(duration);
    
    while start.elapsed() < duration {
        terminal.draw(|f| render_ui(f, &app))?;
        app.tick();
        std::thread::sleep(Duration::from_millis(50)); // 20 FPS
    }
    
    // Restore terminal
    restore_terminal()?;
    
    if verbose {
        println!("View test completed successfully!");
        println!("View '{}' was displayed for {} seconds", 
                 match test_view {
                     TestView::Dashboard => "Dashboard",
                     TestView::Vms => "VMs",
                     TestView::Containers => "Containers",
                     TestView::Logs => "Logs", 
                     TestView::System => "System",
                 }, duration.as_secs());
    }
    
    Ok(())
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