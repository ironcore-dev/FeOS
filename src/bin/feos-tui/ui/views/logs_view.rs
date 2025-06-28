use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::app::App;

pub fn render_logs_view(f: &mut Frame, area: Rect, app: &App) {
    render_log_content(f, area, app);
}

fn render_log_content(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_feos_logs(f, chunks[0], &app.feos_logs);
    render_kernel_logs(f, chunks[1], &app.kernel_logs);
}

fn render_feos_logs(f: &mut Frame, area: Rect, logs: &[crate::mock_data::LogEntry]) {
    let log_items: Vec<ListItem> = logs
        .iter()
        .rev() // Show newest first
        .take(20) // Limit display to prevent performance issues
        .map(|log| {
            let level_style = match log.level.as_str() {
                "ERROR" => Style::default().fg(Color::Red),
                "WARN" => Style::default().fg(Color::Yellow),
                "INFO" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::White),
            };

            ListItem::new(format!("[{}] {}", log.level, log.message))
                .style(level_style)
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("FeOS Logs ({} entries)", logs.len())),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

fn render_kernel_logs(f: &mut Frame, area: Rect, logs: &[crate::mock_data::LogEntry]) {
    let log_items: Vec<ListItem> = logs
        .iter()
        .rev() // Show newest first
        .take(20) // Limit display to prevent performance issues
        .map(|log| {
            let level_style = match log.level.as_str() {
                "ERROR" => Style::default().fg(Color::Red),
                "WARN" => Style::default().fg(Color::Yellow), 
                "INFO" => Style::default().fg(Color::Blue),
                _ => Style::default().fg(Color::White),
            };

            ListItem::new(format!("[{}] {}", log.level, log.message))
                .style(level_style)
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Kernel Logs ({} entries)", logs.len())),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
} 