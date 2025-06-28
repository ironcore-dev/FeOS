use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell, Wrap},
    Frame,
};
use crate::app::App;
use crate::mock_data::{ContainerStatus, format_bytes, format_uptime};

fn format_memory(bytes: u64) -> String {
    if bytes == 0 {
        "No limit".to_string()
    } else {
        format_bytes(bytes)
    }
}

fn format_cpu_limit(limit: f64) -> String {
    if limit == 0.0 {
        "No limit".to_string()
    } else {
        format!("{:.1} cores", limit)
    }
}

fn format_uptime_from_created(created: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime_seconds = now.saturating_sub(created);
    format_uptime(uptime_seconds)
}

pub fn render_containers_view(f: &mut Frame, area: Rect, app: &App) {
    if app.container_logs_expanded {
        // In expanded mode, use the entire screen for logs
        render_full_screen_container_logs(f, area, app);
    } else {
        // Normal mode: top half for containers, bottom half for logs
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        render_containers_section(f, chunks[0], app);
        render_container_logs_section(f, chunks[1], app);
    }
}

fn render_container_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec!["Name", "Status", "Image", "Uptime"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app.containers
        .iter()
        .enumerate()
        .map(|(index, container)| {
            let status_style = match container.status {
                ContainerStatus::Running => Style::default().fg(Color::Green),
                ContainerStatus::Stopped => Style::default().fg(Color::Red),
                ContainerStatus::Starting => Style::default().fg(Color::Yellow),
                ContainerStatus::Stopping => Style::default().fg(Color::Yellow),
                ContainerStatus::Error => Style::default().fg(Color::Magenta),
                ContainerStatus::Exited => Style::default().fg(Color::Gray),
            };

            let row_style = if index == app.selected_container_index {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(container.name.as_str()),
                Cell::from(container.status.as_str()).style(status_style),
                Cell::from(container.image.as_str()),
                Cell::from(format_uptime_from_created(container.created)),
            ]).style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(20),    // Name
            Constraint::Length(10), // Status
            Constraint::Min(15),    // Image
            Constraint::Length(10), // Uptime
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Containers"),
    )
    .style(Style::default().fg(Color::White));

    f.render_widget(table, area);
}

fn render_containers_section(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_container_table(f, chunks[0], app);
    render_container_details(f, chunks[1], app);
}

fn render_container_details(f: &mut Frame, area: Rect, app: &App) {
    render_selected_container_info(f, area, app);
}

fn render_selected_container_info(f: &mut Frame, area: Rect, app: &App) {
    // Show details of the selected container
    let selected_container = app.get_selected_container();

    let content = if let Some(container) = selected_container {
        format!(
            "Name: {}\n\
             ID: {}\n\
             Status: {}\n\
             Image: {}\n\
             Created: {}\n\
             Memory Limit: {}\n\
             CPU Limit: {}\n\n\
             Container Configuration:\n\
             • Runtime: containerd/runc\n\
             • Network: Host mode\n\
             • Restart policy: Unless stopped\n\
             • Security: Non-privileged",
            container.name,
            container.id,
            container.status.as_str(),
            container.image,
            format_uptime_from_created(container.created),
            format_memory(container.memory_limit),
            format_cpu_limit(container.cpu_limit)
        )
    } else {
        "No containers available".to_string()
    };

    let details = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Container Details"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(details, area);
}

fn render_container_logs_section(f: &mut Frame, area: Rect, app: &App) {
    let selected_container = app.get_selected_container();
    
    if let Some(container) = selected_container {
        // Generate mock container logs for the selected container
        let container_logs = generate_container_logs(&container.name, &container.image);
        
        let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
        
        // Show most recent logs in chronological order (old to new)
        let logs_to_show = if container_logs.len() <= available_height {
            &container_logs[..]
        } else {
            // Show the most recent logs that fit in the area
            &container_logs[container_logs.len() - available_height..]
        };
        
        let log_items: Vec<ratatui::widgets::ListItem> = logs_to_show
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
                let datetime = chrono::DateTime::<chrono::Utc>::from(dt);
                let time_str = datetime.format("%H:%M:%S").to_string();
                
                ratatui::widgets::ListItem::new(format!("[{}] {}: {}", time_str, log.level, log.message))
                    .style(Style::default().fg(level_color))
            })
            .collect();

        let logs_list = ratatui::widgets::List::new(log_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!("Container '{}' Logs (Press 'e' to expand)", container.name)),
            )
            .style(Style::default().fg(Color::White));

        f.render_widget(logs_list, area);
    } else {
        let placeholder = Paragraph::new("No container selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Container Logs"),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn render_full_screen_container_logs(f: &mut Frame, area: Rect, app: &App) {
    let selected_container = app.get_selected_container();
    
    if let Some(container) = selected_container {
        let wrap_status = if app.log_line_wrap { "ON" } else { "OFF" };
        let title = format!("Container '{}' Logs (Scroll: {} | Line Wrap: {} | 'w' to toggle wrap, 'e' or Esc to collapse)", 
                           container.name, app.log_scroll_offset, wrap_status);
        
        // Generate mock container logs for the selected container
        let container_logs = generate_container_logs(&container.name, &container.image);
        
        if app.log_line_wrap {
            // Use Text with colored spans for proper line wrap support with colors
            let log_lines: Vec<Line> = container_logs
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
                    let datetime = chrono::DateTime::<chrono::Utc>::from(dt);
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
            let start_idx = app.log_scroll_offset.min(container_logs.len().saturating_sub(1));
            let end_idx = (start_idx + available_height).min(container_logs.len());
            
            let logs_to_show = if start_idx < end_idx {
                &container_logs[start_idx..end_idx]
            } else {
                &[]
            };
            
            let log_items: Vec<ratatui::widgets::ListItem> = logs_to_show
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
                    let datetime = chrono::DateTime::<chrono::Utc>::from(dt);
                    let time_str = datetime.format("%H:%M:%S").to_string();
                    
                    let line = format!("[{}] {}: {}", time_str, log.level, log.message);
                    
                    // Truncate long lines if wrapping is disabled
                    let max_width = area.width.saturating_sub(4) as usize; // Account for borders
                    let truncated = if line.len() > max_width {
                        format!("{}...", &line[..max_width.saturating_sub(3)])
                    } else {
                        line
                    };
                    ratatui::widgets::ListItem::new(truncated).style(Style::default().fg(level_color))
                })
                .collect();

            let logs_list = ratatui::widgets::List::new(log_items)
                .block(Block::default().title(title).borders(Borders::ALL))
                .style(Style::default().fg(Color::White));

            f.render_widget(logs_list, area);
        }
    } else {
        let placeholder = Paragraph::new("No container selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Full Screen Logs"),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn generate_container_logs(container_name: &str, image: &str) -> Vec<crate::mock_data::LogEntry> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    // Generate logs based on container type
    let logs = if image.contains("nginx") {
        vec![
            format!("{}: starting nginx", container_name),
            format!("{}: nginx configuration loaded", container_name),
            format!("{}: listening on port 80", container_name),
            format!("{}: GET /health 200", container_name),
            format!("{}: worker process started", container_name),
            format!("{}: GET /api/status 200", container_name),
            format!("{}: access log rotated", container_name),
            format!("{}: POST /api/data 201", container_name),
        ]
    } else if image.contains("postgres") {
        vec![
            format!("{}: PostgreSQL init process complete", container_name),
            format!("{}: database system is ready to accept connections", container_name),
            format!("{}: listening on port 5432", container_name),
            format!("{}: checkpoint starting", container_name),
            format!("{}: connection received: host=app port=34567", container_name),
            format!("{}: SELECT query executed in 2.4ms", container_name),
            format!("{}: transaction committed", container_name),
        ]
    } else if image.contains("redis") {
        vec![
            format!("{}: Redis server started", container_name),
            format!("{}: ready to accept connections", container_name),
            format!("{}: DB loaded from disk", container_name),
            format!("{}: RDB: 0 keys in 0 databases", container_name),
            format!("{}: client connected", container_name),
            format!("{}: SET key executed", container_name),
        ]
    } else if image.contains("node") {
        vec![
            format!("{}: npm start", container_name),
            format!("{}: server listening on port 3000", container_name),
            format!("{}: connected to database", container_name),
            format!("{}: middleware loaded", container_name),
            format!("{}: API routes configured", container_name),
            format!("{}: user authentication successful", container_name),
        ]
    } else if image.contains("python") {
        vec![
            format!("{}: starting Python application", container_name),
            format!("{}: loading configuration", container_name),
            format!("{}: connecting to data source", container_name),
            format!("{}: ETL pipeline initialized", container_name),
            format!("{}: processing batch job", container_name),
            format!("{}: data transformation complete", container_name),
        ]
    } else {
        vec![
            format!("{}: container started", container_name),
            format!("{}: application initialized", container_name),
            format!("{}: ready to serve requests", container_name),
            format!("{}: health check passed", container_name),
            format!("{}: processing request", container_name),
        ]
    };

    let logs_len = logs.len();
    logs.into_iter().enumerate().map(|(i, message)| {
        crate::mock_data::LogEntry {
            timestamp: now - (logs_len - i) as u64 * 10, // Space logs 10 seconds apart
            level: match i % 6 {
                0 => "INFO".to_string(),
                4 => "WARN".to_string(),
                5 => "ERROR".to_string(),
                _ => "INFO".to_string(),
            },
            message,
        }
    }).collect()
} 