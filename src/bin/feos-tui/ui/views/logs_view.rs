use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};
use chrono::{DateTime, Utc};
use crate::app::App;

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
    let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
    
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
            let level_style = match log.level.as_str() {
                "ERROR" => Style::default().fg(Color::Red),
                "WARN" => Style::default().fg(Color::Yellow),
                "INFO" => Style::default().fg(Color::Green),
                _ => Style::default().fg(Color::White),
            };

            // Format timestamp as HH:MM:SS
            let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
            let datetime = DateTime::<Utc>::from(dt);
            let time_str = datetime.format("%H:%M:%S").to_string();

            ListItem::new(format!("[{}] {}: {}", time_str, log.level, log.message))
                .style(level_style)
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("FeOS Logs")),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

fn render_kernel_logs(f: &mut Frame, area: Rect, logs: &[crate::mock_data::LogEntry]) {
    let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
    
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
            let level_style = match log.level.as_str() {
                "ERROR" => Style::default().fg(Color::Red),
                "WARN" => Style::default().fg(Color::Yellow), 
                "INFO" => Style::default().fg(Color::Blue),
                _ => Style::default().fg(Color::White),
            };

            // Format timestamp as HH:MM:SS
            let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
            let datetime = DateTime::<Utc>::from(dt);
            let time_str = datetime.format("%H:%M:%S").to_string();

            ListItem::new(format!("[{}] {}: {}", time_str, log.level, log.message))
                .style(level_style)
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Kernel Logs")),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

fn render_full_screen_feos_logs(f: &mut Frame, area: Rect, app: &App, title: &str) {
    if app.log_line_wrap {
        // Use Text with colored spans for proper line wrap support with colors
        let log_lines: Vec<Line> = app.feos_logs
            .iter()
            .map(|log| {
                let level_color = match log.level.as_str() {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::Green,
                    _ => Color::White,
                };

                // Format timestamp as HH:MM:SS
                let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
                let datetime = DateTime::<Utc>::from(dt);
                let time_str = datetime.format("%H:%M:%S").to_string();
                
                Line::from(vec![
                    Span::styled(
                        format!("[{}] {}: {}", time_str, log.level, log.message),
                        Style::default().fg(level_color)
                    )
                ])
            })
            .collect();

        let text = Text::from(log_lines);
        
        let paragraph = Paragraph::new(text)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: true })
            .scroll((app.log_scroll_offset as u16, 0));

        f.render_widget(paragraph, area);
    } else {
        // Use List without wrapping (with truncation)
        let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
        
        // Calculate which logs to show with scrolling in chronological order
        let start_idx = app.log_scroll_offset.min(app.feos_logs.len().saturating_sub(1));
        let end_idx = (start_idx + available_height).min(app.feos_logs.len());
        
        let logs_to_show = if start_idx < end_idx {
            &app.feos_logs[start_idx..end_idx]
        } else {
            &[]
        };
        
        let log_items: Vec<ListItem> = logs_to_show
            .iter()
            .map(|log| {
                let level_color = match log.level.as_str() {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::Green,
                    _ => Color::White,
                };
                
                // Format timestamp as HH:MM:SS
                let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
                let datetime = DateTime::<Utc>::from(dt);
                let time_str = datetime.format("%H:%M:%S").to_string();
                
                let line = format!("[{}] {}: {}", time_str, log.level, log.message);
                
                // Truncate long lines if wrapping is disabled
                let max_width = area.width.saturating_sub(4) as usize; // Account for borders
                let truncated = if line.len() > max_width {
                    format!("{}...", &line[..max_width.saturating_sub(3)])
                } else {
                    line
                };
                ListItem::new(truncated).style(Style::default().fg(level_color))
            })
            .collect();

        let logs_list = List::new(log_items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

        f.render_widget(logs_list, area);
    }
}

fn render_full_screen_kernel_logs(f: &mut Frame, area: Rect, app: &App, title: &str) {
    if app.log_line_wrap {
        // Use Text with colored spans for proper line wrap support with colors
        let log_lines: Vec<Line> = app.kernel_logs
            .iter()
            .map(|log| {
                let level_color = match log.level.as_str() {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::Blue,
                    _ => Color::White,
                };

                // Format timestamp as HH:MM:SS
                let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
                let datetime = DateTime::<Utc>::from(dt);
                let time_str = datetime.format("%H:%M:%S").to_string();
                
                Line::from(vec![
                    Span::styled(
                        format!("[{}] {}: {}", time_str, log.level, log.message),
                        Style::default().fg(level_color)
                    )
                ])
            })
            .collect();

        let text = Text::from(log_lines);
        
        let paragraph = Paragraph::new(text)
            .block(Block::default().title(title).borders(Borders::ALL))
            .wrap(Wrap { trim: true })
            .scroll((app.log_scroll_offset as u16, 0));

        f.render_widget(paragraph, area);
    } else {
        // Use List without wrapping (with truncation)
        let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
        
        // Calculate which logs to show with scrolling in chronological order
        let start_idx = app.log_scroll_offset.min(app.kernel_logs.len().saturating_sub(1));
        let end_idx = (start_idx + available_height).min(app.kernel_logs.len());
        
        let logs_to_show = if start_idx < end_idx {
            &app.kernel_logs[start_idx..end_idx]
        } else {
            &[]
        };
        
        let log_items: Vec<ListItem> = logs_to_show
            .iter()
            .map(|log| {
                let level_color = match log.level.as_str() {
                    "ERROR" => Color::Red,
                    "WARN" => Color::Yellow,
                    "INFO" => Color::Blue,
                    _ => Color::White,
                };
                
                // Format timestamp as HH:MM:SS
                let dt = std::time::UNIX_EPOCH + std::time::Duration::from_secs(log.timestamp);
                let datetime = DateTime::<Utc>::from(dt);
                let time_str = datetime.format("%H:%M:%S").to_string();
                
                let line = format!("[{}] {}: {}", time_str, log.level, log.message);
                
                // Truncate long lines if wrapping is disabled
                let max_width = area.width.saturating_sub(4) as usize; // Account for borders
                let truncated = if line.len() > max_width {
                    format!("{}...", &line[..max_width.saturating_sub(3)])
                } else {
                    line
                };
                ListItem::new(truncated).style(Style::default().fg(level_color))
            })
            .collect();

        let logs_list = List::new(log_items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

        f.render_widget(logs_list, area);
    }
} 