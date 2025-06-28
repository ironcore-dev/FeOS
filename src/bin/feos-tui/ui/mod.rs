pub mod components;
pub mod dashboard;
pub mod views;

use ratatui::{
    layout::{Constraint, Direction, Layout},
    Frame,
};
use crate::app::{App, CurrentView};

pub fn render_ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // Main content
        ])
        .split(f.area());

    // Header
    components::render_header(f, chunks[0]);

    // Tabs
    components::render_tabs(f, chunks[1], app.current_view);

    // Main content based on current view
    match app.current_view {
        CurrentView::Dashboard => dashboard::render_dashboard(f, chunks[2], app),
        CurrentView::VMs => views::render_vms_view(f, chunks[2], app),
        CurrentView::Containers => views::render_containers_view(f, chunks[2], app),
        CurrentView::IsolatedPods => views::render_isolated_pods_view(f, chunks[2], app),
        CurrentView::Logs => views::render_logs_view(f, chunks[2], app),
    }
    
    // Render system modal if open
    if app.system_modal_open {
        components::render_system_modal(f, f.area(), app);
    }
    
    // Render help modal if open
    if app.help_modal_open {
        components::render_help_modal(f, f.area(), app);
    }
} 