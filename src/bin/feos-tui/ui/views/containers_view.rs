use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell},
    Frame,
};
use crate::app::App;
use crate::mock_data::{ContainerStatus, generate_container_logs};
use super::super::{log_components::{self, LogConfig}, utils};

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
                Cell::from(utils::format_uptime_from_created(container.created)),
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
            utils::format_uptime_from_created(container.created),
            utils::format_memory_limit(container.memory_limit),
            utils::format_cpu_limit(container.cpu_limit)
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
        let title = format!("Container '{}' Logs (Press 'e' to expand)", container.name);
        
        log_components::render_compact_log_view(
            f,
            area,
            &container_logs,
            &title,
            false, // Not kernel mode
        );
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
        
        let config = LogConfig {
            title: &title,
            scroll_offset: app.log_scroll_offset,
            line_wrap: app.log_line_wrap,
            kernel_mode: false, // Not kernel mode
        };
        
        log_components::render_log_view(f, area, &container_logs, config);
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

 