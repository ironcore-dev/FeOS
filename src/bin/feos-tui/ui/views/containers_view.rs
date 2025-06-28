use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell},
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
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_container_table(f, chunks[0], app);
    render_container_details(f, chunks[1], app);
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