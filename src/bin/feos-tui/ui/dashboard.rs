use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph, Sparkline},
    Frame,
};
use chrono::{DateTime, Utc};
use crate::app::App;
use crate::mock_data::{HostInfo, VmInfo, LogEntry, format_uptime, get_ram_usage_percentage};

pub fn render_dashboard(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_left_column(f, chunks[0], app);
    render_right_column(f, chunks[1], app);
}

fn render_left_column(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Host info with IPv6 info and network interfaces
            Constraint::Min(0),    // VMs overview
        ])
        .split(area);

    render_host_info(f, chunks[0], &app.host_info);
    render_vms_overview(f, chunks[1], &app.vms);
}

fn render_right_column(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // CPU sparkline
            Constraint::Length(8), // Memory sparkline  
            Constraint::Min(0),    // FeOS logs
        ])
        .split(area);

    render_cpu_sparkline(f, chunks[0], &app.cpu_history);
    render_memory_sparkline(f, chunks[1], &app.memory_history, &app.host_info);
    render_feos_logs(f, chunks[2], &app.feos_logs);
}

fn render_host_info(f: &mut Frame, area: Rect, host_info: &HostInfo) {
    let mut info_lines = vec![
        format!("Uptime: {}", format_uptime(host_info.uptime)),
        format!("CPU Cores: {}", host_info.num_cores),
        format!("Total RAM: {}", crate::mock_data::format_bytes(host_info.ram_total)),
        format!("IPv6 Address: {}", host_info.ipv6_address),
        format!("Delegated Prefix: {}", host_info.delegated_prefix),
    ];

    // Add network interface information
    for interface in &host_info.net_interfaces {
        info_lines.push(format!("{}: {} ({})", interface.name, interface.ipv6_address, interface.mac_address));
    }

    let info_text = info_lines.join("\n");
    let paragraph = Paragraph::new(info_text)
        .block(Block::default().title("Host Information").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(paragraph, area);
}

fn render_vms_overview(f: &mut Frame, area: Rect, vms: &[VmInfo]) {
    // Calculate column widths for proper alignment
    let max_uuid_len = vms.iter().map(|vm| vm.uuid.len()).max().unwrap_or(0);
    let max_status_len = vms.iter().map(|vm| vm.status.as_str().len()).max().unwrap_or(0);
    
    let vm_items: Vec<ListItem> = vms
        .iter()
        .map(|vm| {
            let status_color = match vm.status {
                crate::mock_data::VmStatus::Running => Color::Green,
                crate::mock_data::VmStatus::Stopped => Color::Red,
                crate::mock_data::VmStatus::Starting => Color::Yellow,
                crate::mock_data::VmStatus::Stopping => Color::Yellow,
                crate::mock_data::VmStatus::Error => Color::Magenta,
            };

            // Format with proper column alignment
            let formatted_line = format!(
                "{:<width_uuid$} - {:<width_status$} - {}",
                vm.uuid,
                vm.status.as_str(),
                vm.image_name,
                width_uuid = max_uuid_len,
                width_status = max_status_len
            );

            ListItem::new(formatted_line)
                .style(Style::default().fg(status_color))
        })
        .collect();

    let vms_list = List::new(vm_items)
        .block(Block::default().title("Virtual Machines").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(vms_list, area);
}

fn render_cpu_sparkline(f: &mut Frame, area: Rect, cpu_history: &[u64]) {
    // Get current CPU usage (latest value from history)
    let current_cpu = cpu_history.last().unwrap_or(&0);
    let title = format!("CPU Usage: {}%", current_cpu);
    
    // Create block with border
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner_area = block.inner(area);
    
    // Render the block
    f.render_widget(block, area);
    
    // Right-align the sparkline by taking only the most recent values that fit
    let max_data_points = inner_area.width as usize;
    let data_to_show = if cpu_history.len() <= max_data_points {
        cpu_history
    } else {
        &cpu_history[cpu_history.len() - max_data_points..]
    };
    
    // Render sparkline in the inner area to maximize space usage
    // Note: mock data already includes 0-100% anchor points for proper scaling
    let sparkline = Sparkline::default()
        .data(data_to_show)
        .max(100)
        .style(Style::default().fg(Color::Blue));

    f.render_widget(sparkline, inner_area);
}

fn render_memory_sparkline(f: &mut Frame, area: Rect, memory_history: &[u64], host_info: &HostInfo) {
    // Get current memory usage percentage
    let current_memory = get_ram_usage_percentage(host_info) as u64;
    let used_memory = host_info.ram_total - host_info.ram_unused;
    let title = format!("Memory Usage: {}% ({} / {})", 
                       current_memory, 
                       crate::mock_data::format_bytes(used_memory),
                       crate::mock_data::format_bytes(host_info.ram_total));
    
    // Create block with border
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner_area = block.inner(area);
    
    // Render the block
    f.render_widget(block, area);
    
    // Right-align the sparkline by taking only the most recent values that fit
    let max_data_points = inner_area.width as usize;
    let data_to_show = if memory_history.len() <= max_data_points {
        memory_history
    } else {
        &memory_history[memory_history.len() - max_data_points..]
    };
    
    // Render sparkline in the inner area to maximize space usage
    // Note: mock data already includes 0-100% anchor points for proper scaling
    let sparkline = Sparkline::default()
        .data(data_to_show)
        .max(100)
        .style(Style::default().fg(Color::Green));

    f.render_widget(sparkline, inner_area);
}

fn render_feos_logs(f: &mut Frame, area: Rect, logs: &[LogEntry]) {
    let log_items: Vec<ListItem> = logs
        .iter()
        .rev() // Show newest logs first
        .take(area.height.saturating_sub(2) as usize) // Fit within the area
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
        .block(Block::default().title("Latest FeOS Logs").borders(Borders::ALL))
        .style(Style::default().fg(Color::White));

    f.render_widget(logs_list, area);
} 