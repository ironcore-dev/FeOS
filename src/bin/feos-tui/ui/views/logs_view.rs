use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use crate::app::App;
use super::super::log_components::{self, LogConfig};

pub fn render_logs_view(f: &mut Frame, area: Rect, app: &App) {
    if app.global_logs_expanded {
        // In expanded logs mode, use the entire screen for logs
        render_full_screen_logs(f, area, app);
    } else {
        // Normal mode - side by side view
        render_side_by_side_logs(f, area, app);
    }
}

fn render_side_by_side_logs(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_feos_logs(f, chunks[0], &app.feos_logs);
    render_kernel_logs(f, chunks[1], &app.kernel_logs);
}

fn render_full_screen_logs(f: &mut Frame, area: Rect, app: &App) {
    // Determine the title based on the selected log tab
    let log_type = match app.selected_global_log_tab {
        0 => "FeOS Logs",
        1 => "Kernel Logs", 
        _ => "Logs",
    };

    // Add scroll and wrap info to title
    let wrap_status = if app.log_line_wrap { "ON" } else { "OFF" };
    let title = format!("{} (Scroll: {} | Line Wrap: {} | 'w' to toggle wrap)", log_type, app.log_scroll_offset, wrap_status);

    // Render logs based on selected tab with scrolling - use full screen
    match app.selected_global_log_tab {
        0 => render_full_screen_feos_logs(f, area, app, &title),
        1 => render_full_screen_kernel_logs(f, area, app, &title),
        _ => render_full_screen_feos_logs(f, area, app, &title),
    }
}

fn render_feos_logs(f: &mut Frame, area: Rect, logs: &[crate::mock_data::LogEntry]) {
    log_components::render_compact_log_view(
        f,
        area,
        logs,
        "FeOS Logs",
        false, // Not kernel mode
    );
}

fn render_kernel_logs(f: &mut Frame, area: Rect, logs: &[crate::mock_data::LogEntry]) {
    log_components::render_compact_log_view(
        f,
        area,
        logs,
        "Kernel Logs",
        true, // Kernel mode (blue INFO)
    );
}

fn render_full_screen_feos_logs(f: &mut Frame, area: Rect, app: &App, title: &str) {
    let config = LogConfig {
        title,
        scroll_offset: app.log_scroll_offset,
        line_wrap: app.log_line_wrap,
        kernel_mode: false, // Not kernel mode
    };
    
    log_components::render_log_view(f, area, &app.feos_logs, config);
}

fn render_full_screen_kernel_logs(f: &mut Frame, area: Rect, app: &App, title: &str) {
    let config = LogConfig {
        title,
        scroll_offset: app.log_scroll_offset,
        line_wrap: app.log_line_wrap,
        kernel_mode: true, // Kernel mode (blue INFO)
    };
    
    log_components::render_log_view(f, area, &app.kernel_logs, config);
} 