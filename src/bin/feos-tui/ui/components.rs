use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};
use chrono::Utc;
use crate::app::CurrentView;

pub fn render_header(f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Left side - FeOS version
    let version = env!("CARGO_PKG_VERSION");
    let header_left = Paragraph::new(format!("[ FeOS v{} ]", version))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Left);
    f.render_widget(header_left, chunks[0]);

    // Right side - Current UTC date and time
    let now = Utc::now();
    let datetime_str = now.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let header_right = Paragraph::new(datetime_str)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Right);
    f.render_widget(header_right, chunks[1]);
}

pub fn render_footer(f: &mut Frame, area: Rect, current_view: CurrentView) {
    let help_text = match current_view {
        CurrentView::Dashboard => "Press 'q' to quit | ← → to navigate tabs",
        CurrentView::VMs => "Press 'q' to quit | ↑ ↓ to select VM | ← → to navigate tabs",
        CurrentView::Containers => "Press 'q' to quit | ↑ ↓ to select container | ← → to navigate tabs",
        CurrentView::Logs => "Press 'q' to quit | ← → to navigate tabs",
        CurrentView::System => "Press 'q' to quit | ↑ ↓ to select | Enter to confirm | Esc to cancel | ← → to navigate tabs",
    };
    
    let footer = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

pub fn render_tabs(f: &mut Frame, area: Rect, current_view: CurrentView) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(80), Constraint::Percentage(20)])
        .split(area);

    // Left side tabs: Dashboard, VMs, Containers, Logs
    let left_titles: Vec<Line> = vec!["Dashboard", "VMs", "Containers", "Logs"]
        .iter()
        .map(|t| Line::from(*t))
        .collect();
    
    let left_selected = match current_view {
        CurrentView::Dashboard => Some(0),
        CurrentView::VMs => Some(1),
        CurrentView::Containers => Some(2),
        CurrentView::Logs => Some(3),
        CurrentView::System => None,
    };
    
    let left_tabs = Tabs::new(left_titles)
        .block(Block::default().borders(Borders::ALL))
        .select(left_selected.unwrap_or(0))
        .style(Style::default().fg(Color::White))
        .highlight_style(
            if left_selected.is_some() {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        );
    f.render_widget(left_tabs, chunks[0]);

    // Right side tab: System
    let system_style = if current_view == CurrentView::System {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };
    
    let system_tab = Paragraph::new("System")
        .block(Block::default().borders(Borders::ALL))
        .style(system_style)
        .alignment(Alignment::Center);
    f.render_widget(system_tab, chunks[1]);
} 