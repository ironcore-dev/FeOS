use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph, Table, Row, Cell},
    Frame,
};
use crate::app::App;
use crate::mock_data::VmStatus;

fn format_memory(bytes: u64) -> String {
    crate::mock_data::format_bytes(bytes)
}

pub fn render_vms_view(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_vm_table(f, chunks[0], app);
    render_vm_details(f, chunks[1], app);
}

fn render_vm_table(f: &mut Frame, area: Rect, app: &App) {
    let header = Row::new(vec!["UUID", "Status", "Image", "CPUs", "Memory"])
        .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    let rows: Vec<Row> = app.vms
        .iter()
        .enumerate()
        .map(|(index, vm)| {
            let status_style = match vm.status {
                VmStatus::Running => Style::default().fg(Color::Green),
                VmStatus::Stopped => Style::default().fg(Color::Red),
                VmStatus::Starting => Style::default().fg(Color::Yellow),
                VmStatus::Stopping => Style::default().fg(Color::Yellow),
                VmStatus::Error => Style::default().fg(Color::Magenta),
            };

            let row_style = if index == app.selected_vm_index {
                Style::default().bg(Color::Blue)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(vm.uuid.as_str()),
                Cell::from(vm.status.as_str()).style(status_style),
                Cell::from(vm.image_name.as_str()),
                Cell::from(vm.cpu_count.to_string()),
                Cell::from(format_memory(vm.memory_bytes)),
            ]).style(row_style)
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(12), // UUID
            Constraint::Length(10), // Status
            Constraint::Min(15),    // Image name
            Constraint::Length(6),  // CPUs
            Constraint::Length(8),  // Memory
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Virtual Machines"),
    )
    .style(Style::default().fg(Color::White));

    f.render_widget(table, area);
}

fn render_vm_details(f: &mut Frame, area: Rect, app: &App) {
    render_selected_vm_info(f, area, app);
}

fn render_selected_vm_info(f: &mut Frame, area: Rect, app: &App) {
    // Show details of the selected VM
    let selected_vm = app.get_selected_vm();

    let content = if let Some(vm) = selected_vm {
        format!(
            "UUID: {}\n\
             Status: {}\n\
             Image: {}\n\
             Image UUID: {}\n\
             CPU Cores: {}\n\
             Memory: {}\n\n\
             VM Configuration:\n\
             • Boot order: Disk, Network\n\
             • Graphics: VGA console\n\
             • Network: Bridge mode\n\
             • Storage: Virtio SCSI",
            vm.uuid,
            vm.status.as_str(),
            vm.image_name,
            vm.image_uuid,
            vm.cpu_count,
            format_memory(vm.memory_bytes)
        )
    } else {
        "No VMs available".to_string()
    };

    let details = Paragraph::new(content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("VM Details"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(details, area);
} 