use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell, List, ListItem, Tabs, Wrap},
    Frame,
};
use crate::app::App;
use crate::mock_data::{IsolatedPodStatus, ContainerStatus, format_bytes, format_uptime};
use chrono::{DateTime, Utc};



fn format_uptime_from_created(created: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime_seconds = now.saturating_sub(created);
    format_uptime(uptime_seconds)
}

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

pub fn render_isolated_pods_view(f: &mut Frame, area: Rect, app: &App) {
    if app.logs_expanded {
        // In expanded logs mode, use the entire screen for logs
        render_full_screen_logs(f, area, app);
    } else {
        // Normal mode
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        render_pods_table(f, chunks[0], app);
        render_pod_details(f, chunks[1], app);
    }
}

fn render_pods_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec!["Name", "Status", "Containers", "Memory", "Uptime"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app.isolated_pods
        .iter()
        .enumerate()
        .map(|(index, pod)| {
            let status_style = match pod.status {
                IsolatedPodStatus::Running => Style::default().fg(Color::Green),
                IsolatedPodStatus::Stopped => Style::default().fg(Color::Red),
                IsolatedPodStatus::Starting => Style::default().fg(Color::Yellow),
                IsolatedPodStatus::Stopping => Style::default().fg(Color::Yellow),
                IsolatedPodStatus::Error => Style::default().fg(Color::Magenta),
            };

            let row_style = if index == app.selected_isolated_pod_index {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(pod.name.as_str()),
                Cell::from(pod.status.as_str()).style(status_style),
                Cell::from(pod.containers.len().to_string()),
                Cell::from(format_bytes(pod.memory_bytes)),
                Cell::from(format_uptime_from_created(pod.created)),
            ]).style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(20),    // Name
            Constraint::Length(10), // Status
            Constraint::Length(10), // Containers
            Constraint::Length(8),  // Memory
            Constraint::Length(10), // Uptime
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Isolated Pods"),
    )
    .style(Style::default().fg(Color::White));

    f.render_widget(table, area);
}

fn render_pod_details(f: &mut Frame, area: Rect, app: &App) {
    let chunks = if app.logs_expanded {
        // Expanded logs mode: minimize other sections
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(4),  // Minimal pod info
                Constraint::Length(5),  // Minimal containers table (header + 2-3 rows)
                Constraint::Min(0),     // Expanded logs
            ])
            .split(area)
    } else {
        // Normal mode
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // Pod info
                Constraint::Length(10), // Containers table
                Constraint::Min(0),     // Logs
            ])
            .split(area)
    };

    render_pod_info(f, chunks[0], app);
    render_pod_containers(f, chunks[1], app);
    render_pod_logs(f, chunks[2], app);
}

fn render_pod_info(f: &mut Frame, area: Rect, app: &App) {
    let selected_pod = app.get_selected_isolated_pod();

    let content = if let Some(pod) = selected_pod {
        if app.logs_expanded {
            // Condensed format for expanded logs mode
            format!(
                "Pod: {} | Status: {} | MicroVM: {} | {}/{}",
                pod.name,
                pod.status.as_str(),
                pod.microvm_id,
                pod.cpu_count,
                format_bytes(pod.memory_bytes)
            )
        } else {
            // Full format for normal mode
            format!(
                "Pod: {}\n\
                 ID: {}\n\
                 Status: {}\n\
                 MicroVM: {}\n\
                 CPU Cores: {}\n\
                 Memory: {}\n\
                 Kernel: {}",
                pod.name,
                pod.id,
                pod.status.as_str(),
                pod.microvm_id,
                pod.cpu_count,
                format_bytes(pod.memory_bytes),
                pod.kernel_version
            )
        }
    } else {
        "No isolated pods available".to_string()
    };

    let title = if app.logs_expanded { "Pod (Press 'e' to collapse)" } else { "Pod Details" };
    let details = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(details, area);
}

fn render_pod_containers(f: &mut Frame, area: Rect, app: &App) {
    let selected_pod = app.get_selected_isolated_pod();

    if let Some(pod) = selected_pod {
        let header = Row::new(vec!["Name", "Status", "Image", "Memory", "CPU", "Restarts"])
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        let rows: Vec<Row> = pod.containers
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

                let row_style = if index == app.selected_pod_container_index {
                    Style::default().bg(Color::Blue)
                } else {
                    Style::default()
                };

                Row::new(vec![
                    Cell::from(container.name.as_str()),
                    Cell::from(container.status.as_str()).style(status_style),
                    Cell::from(container.image.as_str()),
                    Cell::from(format_memory(container.memory_limit)),
                    Cell::from(format_cpu_limit(container.cpu_limit)),
                    Cell::from(container.restart_count.to_string()),
                ]).style(row_style)
            })
            .collect();

        let table = Table::new(
            rows,
            [
                Constraint::Min(15),    // Name
                Constraint::Length(10), // Status
                Constraint::Min(20),    // Image
                Constraint::Length(8),  // Memory
                Constraint::Length(8),  // CPU
                Constraint::Length(8),  // Restarts
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Pod Containers"),
        )
        .style(Style::default().fg(Color::White));

        f.render_widget(table, area);
    } else {
        let placeholder = Paragraph::new("No pod selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Pod Containers"),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn render_pod_logs(f: &mut Frame, area: Rect, app: &App) {
    let selected_pod = app.get_selected_isolated_pod();

    if let Some(pod) = selected_pod {
        // Create tabs for kernel logs and container logs
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Tabs
                Constraint::Min(0),    // Log content
            ])
            .split(area);

        // Show tabs with current selection
        let tabs = Tabs::new(vec!["Kernel Logs", "Container Logs"])
            .block(Block::default().borders(Borders::ALL))
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(Color::Yellow))
            .select(app.selected_pod_log_tab);

        f.render_widget(tabs, chunks[0]);

        // Render logs based on selected tab
        match app.selected_pod_log_tab {
            0 => render_kernel_logs(f, chunks[1], pod),
            1 => render_container_logs(f, chunks[1], app),
            _ => render_kernel_logs(f, chunks[1], pod),
        }
    } else {
        let placeholder = Paragraph::new("No pod selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Logs"),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn render_kernel_logs(f: &mut Frame, area: Rect, pod: &crate::mock_data::IsolatedPodInfo) {
    let available_height = area.height.saturating_sub(2) as usize; // Fit within the area
    
    // Show most recent logs in chronological order (old to new)
    let logs_to_show = if pod.kernel_logs.len() <= available_height {
        &pod.kernel_logs[..]
    } else {
        // Show the most recent logs that fit in the area
        &pod.kernel_logs[pod.kernel_logs.len() - available_height..]
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
            
            ListItem::new(format!("[{}] {}: {}", time_str, log.level, log.message))
                .style(Style::default().fg(level_color))
        })
        .collect();

    let logs_list = List::new(log_items)
        .block(Block::default().title("Kernel Logs").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
}

fn render_container_logs(f: &mut Frame, area: Rect, app: &App) {
    let selected_container = app.get_selected_pod_container();
    
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
                
                ListItem::new(format!("[{}] {}: {}", time_str, log.level, log.message))
                    .style(Style::default().fg(level_color))
            })
            .collect();

        let title = format!("Container Logs: {}", container.name);
        let logs_list = List::new(log_items)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(Color::White));

        f.render_widget(logs_list, area);
    } else {
        let placeholder = Paragraph::new("No container selected")
            .block(Block::default().title("Container Logs").borders(Borders::ALL))
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn render_full_screen_logs(f: &mut Frame, area: Rect, app: &App) {
    let selected_pod = app.get_selected_isolated_pod();

    if let Some(pod) = selected_pod {
        // Determine the title based on the selected log tab and container
        let title = match app.selected_pod_log_tab {
            0 => {
                // Kernel logs
                format!("Isolated Pod \"{}\" - Kernel Logs", pod.name)
            }
            1 => {
                // Container logs
                if let Some(container) = app.get_selected_pod_container() {
                    format!("Isolated Pod \"{}\" - Container \"{}\" Logs", pod.name, container.name)
                } else {
                    format!("Isolated Pod \"{}\" - Container Logs (No container selected)", pod.name)
                }
            }
            _ => {
                format!("Isolated Pod \"{}\" - Logs", pod.name)
            }
        };

        // Add scroll and wrap info to title
        let _wrap_status = if app.log_line_wrap { "ON" } else { "OFF" };

        // Render logs based on selected tab with scrolling - use full screen
        match app.selected_pod_log_tab {
            0 => render_full_screen_kernel_logs(f, area, pod, app, &title),
            1 => render_full_screen_container_logs(f, area, app, &title),
            _ => render_full_screen_kernel_logs(f, area, pod, app, &title),
        }
    } else {
        let placeholder = Paragraph::new("No pod selected")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Full Screen Logs"),
            )
            .style(Style::default().fg(Color::Gray));

        f.render_widget(placeholder, area);
    }
}

fn render_full_screen_kernel_logs(f: &mut Frame, area: Rect, pod: &crate::mock_data::IsolatedPodInfo, app: &App, title: &str) {
    if app.log_line_wrap {
        // Use Text with colored spans for proper line wrap support with colors
        let log_lines: Vec<Line> = pod.kernel_logs
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
        let start_idx = app.log_scroll_offset.min(pod.kernel_logs.len().saturating_sub(1));
        let end_idx = (start_idx + available_height).min(pod.kernel_logs.len());
        
        let logs_to_show = if start_idx < end_idx {
            &pod.kernel_logs[start_idx..end_idx]
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

fn render_full_screen_container_logs(f: &mut Frame, area: Rect, app: &App, title: &str) {
    let selected_container = app.get_selected_pod_container();
    
    if let Some(container) = selected_container {
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
            let start_idx = app.log_scroll_offset.min(container_logs.len().saturating_sub(1));
            let end_idx = (start_idx + available_height).min(container_logs.len());
            
            let logs_to_show = if start_idx < end_idx {
                &container_logs[start_idx..end_idx]
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
    } else {
        let placeholder = Paragraph::new("No container selected")
            .block(Block::default().title(title).borders(Borders::ALL))
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
        ]
    } else if image.contains("postgres") {
        vec![
            format!("{}: PostgreSQL init process complete", container_name),
            format!("{}: database system is ready to accept connections", container_name),
            format!("{}: listening on port 5432", container_name),
            format!("{}: checkpoint starting", container_name),
            format!("{}: connection received: host=pod port=34567", container_name),
        ]
    } else if image.contains("redis") {
        vec![
            format!("{}: Redis server started", container_name),
            format!("{}: ready to accept connections", container_name),
            format!("{}: DB loaded from disk", container_name),
            format!("{}: RDB: 0 keys in 0 databases", container_name),
        ]
    } else if image.contains("node") {
        vec![
            format!("{}: npm start", container_name),
            format!("{}: server listening on port 3000", container_name),
            format!("{}: connected to database", container_name),
            format!("{}: middleware loaded", container_name),
            format!("{}: API routes configured", container_name),
        ]
    } else if image.contains("python") {
        vec![
            format!("{}: starting Python application", container_name),
            format!("{}: loading configuration", container_name),
            format!("{}: connecting to data source", container_name),
            format!("{}: ETL pipeline initialized", container_name),
            format!("{}: processing batch job", container_name),
        ]
    } else if image.contains("jupyter") {
        vec![
            format!("{}: starting Jupyter server", container_name),
            format!("{}: notebook server is running", container_name),
            format!("{}: kernel started", container_name),
            format!("{}: loading ML libraries", container_name),
        ]
    } else if image.contains("tensorflow") {
        vec![
            format!("{}: TensorFlow serving started", container_name),
            format!("{}: model loaded successfully", container_name),
            format!("{}: serving on port 8501", container_name),
            format!("{}: prediction request processed", container_name),
        ]
    } else {
        vec![
            format!("{}: container started", container_name),
            format!("{}: application initialized", container_name),
            format!("{}: ready to serve requests", container_name),
        ]
    };

    let logs_len = logs.len();
    logs.into_iter().enumerate().map(|(i, message)| {
        crate::mock_data::LogEntry {
            timestamp: now - (logs_len - i) as u64 * 10, // Space logs 10 seconds apart
            level: match i % 5 {
                0 => "INFO".to_string(),
                4 => "WARN".to_string(),
                _ => "INFO".to_string(),
            },
            message,
        }
    }).collect()
} 