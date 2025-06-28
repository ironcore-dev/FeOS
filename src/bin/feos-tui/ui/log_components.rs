use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use chrono::{DateTime, Utc};
use crate::mock_data::LogEntry;

/// Configuration for log rendering
pub struct LogConfig<'a> {
    pub title: &'a str,
    pub scroll_offset: usize,
    pub line_wrap: bool,
    pub kernel_mode: bool, // true for kernel logs (blue INFO), false for regular logs (green INFO)
}

/// Renders a log list with proper chronological ordering, scrolling, and wrapping support
pub fn render_log_view(
    f: &mut Frame,
    area: Rect,
    logs: &[LogEntry],
    config: LogConfig,
) {
    if config.line_wrap {
        render_wrapped_logs(f, area, logs, &config);
    } else {
        render_list_logs(f, area, logs, &config);
    }
}

/// Renders logs using List widget (no wrapping, with truncation)
fn render_list_logs(
    f: &mut Frame,
    area: Rect,
    logs: &[LogEntry],
    config: &LogConfig,
) {
    let available_height = area.height.saturating_sub(2) as usize; // Account for borders
    
    // Calculate which logs to show with scrolling in chronological order
    let start_idx = config.scroll_offset.min(logs.len().saturating_sub(1));
    let end_idx = (start_idx + available_height).min(logs.len());
    
    let logs_to_show = if start_idx < end_idx {
        &logs[start_idx..end_idx]
    } else {
        &[]
    };
    
    let log_items: Vec<ListItem> = logs_to_show
        .iter()
        .map(|log| {
            let level_color = get_log_level_color(&log.level, config.kernel_mode);
            let formatted_line = format_log_line(log);
            
            // Truncate long lines
            let max_width = area.width.saturating_sub(4) as usize; // Account for borders
            let truncated = if formatted_line.len() > max_width {
                format!("{}...", &formatted_line[..max_width.saturating_sub(3)])
            } else {
                formatted_line
            };
            
            ListItem::new(truncated).style(Style::default().fg(level_color))
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(Block::default().title(config.title).borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

/// Renders logs using Paragraph widget (with line wrapping and color preservation)
fn render_wrapped_logs(
    f: &mut Frame,
    area: Rect,
    logs: &[LogEntry],
    config: &LogConfig,
) {
    let log_lines: Vec<Line> = logs
        .iter()
        .map(|log| {
            let level_color = get_log_level_color(&log.level, config.kernel_mode);
            let formatted_line = format_log_line(log);
            
            Line::from(vec![
                Span::styled(formatted_line, Style::default().fg(level_color))
            ])
        })
        .collect();

    let text = Text::from(log_lines);
    
    let paragraph = Paragraph::new(text)
        .block(Block::default().title(config.title).borders(Borders::ALL))
        .wrap(Wrap { trim: true })
        .scroll((config.scroll_offset as u16, 0));

    f.render_widget(paragraph, area);
}

/// Renders logs in a compact view (shows most recent logs that fit)
pub fn render_compact_log_view(
    f: &mut Frame,
    area: Rect,
    logs: &[LogEntry],
    title: &str,
    kernel_mode: bool,
) {
    let available_height = area.height.saturating_sub(2) as usize; // Account for borders
    
    // Show most recent logs in chronological order (old to new)
    let logs_to_show = if logs.len() <= available_height {
        logs
    } else {
        // Show the most recent logs that fit in the area
        &logs[logs.len() - available_height..]
    };
    
    let log_items: Vec<ListItem> = logs_to_show
        .iter()
        .map(|log| {
            let level_color = get_log_level_color(&log.level, kernel_mode);
            let formatted_line = format_log_line(log);
            
            ListItem::new(formatted_line)
                .style(Style::default().fg(level_color))
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

/// Get color for log level based on context
fn get_log_level_color(level: &str, kernel_mode: bool) -> Color {
    match level {
        "ERROR" => Color::Red,
        "WARN" => Color::Yellow,
        "INFO" => if kernel_mode { Color::Blue } else { Color::Green },
        _ => Color::White,
    }
}

/// Format a log entry with timestamp
fn format_log_line(log: &LogEntry) -> String {
    // Format timestamp as HH:MM:SS
    let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
    let datetime = DateTime::<Utc>::from(dt);
    let time_str = datetime.format("%H:%M:%S").to_string();
    
    format!("[{}] {}: {}", time_str, log.level, log.message)
} 