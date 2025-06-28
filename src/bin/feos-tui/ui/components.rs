use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
    Frame,
};
use chrono::Utc;
use crate::app::{App, CurrentView, SystemAction};

pub fn render_header(f: &mut Frame, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25), Constraint::Percentage(50), Constraint::Percentage(25)])
        .split(area);

    // Left side - FeOS version
    let version = env!("CARGO_PKG_VERSION");
    let header_left = Paragraph::new(format!("[ FeOS v{} ]", version))
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Left);
    f.render_widget(header_left, chunks[0]);

    // Center - Help text
    let help_text = Paragraph::new("Press 'h' for help")
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);
    f.render_widget(help_text, chunks[1]);

    // Right side - Current UTC date and time
    let now = Utc::now();
    let datetime_str = now.format("%Y-%m-%d %H:%M:%S UTC").to_string();
    let header_right = Paragraph::new(datetime_str)
        .style(Style::default().fg(Color::Gray))
        .alignment(Alignment::Right);
    f.render_widget(header_right, chunks[2]);
}

pub fn render_help_modal(f: &mut Frame, area: Rect, app: &App) {
    // Create a centered popup area
    let popup_area = centered_rect(70, 80, area);
    
    // Clear the background
    f.render_widget(Clear, popup_area);
    
    // Generate context-aware help content
    let mut help_text = vec![
        "FeOS TUI - Help",
        "",
        "GENERAL CONTROLS:",
        "  q              - Quit application",
        "  h              - Show/hide this help",
        "  ← →            - Navigate between tabs",
        "  Ctrl+Q         - System actions (reboot/shutdown)",
        "",
    ];

    // Add view-specific help based on current view and context
    match app.current_view {
        CurrentView::Dashboard => {
            help_text.extend(vec![
                "DASHBOARD VIEW:",
                "  (No specific controls)",
                "",
            ]);
        }
        CurrentView::VMs => {
            help_text.extend(vec![
                "VMS VIEW:",
                "  ↑ ↓            - Select VM",
                "",
            ]);
        }
        CurrentView::Containers => {
            if app.container_logs_expanded {
                help_text.extend(vec![
                    "CONTAINERS - EXPANDED LOGS MODE:",
                    "  ↑ ↓            - Scroll logs line by line",
                    "  Page Up/Down   - Scroll logs page by page",
                    "  w              - Toggle line wrapping",
                    "  e / Esc        - Exit expanded logs mode",
                    "",
                ]);
            } else {
                help_text.extend(vec![
                    "CONTAINERS VIEW:",
                    "  ↑ ↓            - Select container",
                    "  e              - Enter expanded logs mode",
                    "",
                ]);
            }
        }
        CurrentView::IsolatedPods => {
            if app.logs_expanded {
                help_text.extend(vec![
                    "ISOLATED PODS - EXPANDED LOGS MODE:",
                    "  ↑ ↓            - Scroll logs line by line",
                    "  Page Up/Down   - Scroll logs page by page",
                    "  w              - Toggle line wrapping",
                    "  e / Esc        - Exit expanded logs mode",
                    "",
                                         "NOTE:",
                     "  Use Shift+← → (before entering expanded mode) to switch log types",
                     "  Use Shift+↑ ↓ (before entering expanded mode) to select container",
                    "",
                ]);
            } else {
                help_text.extend(vec![
                    "ISOLATED PODS VIEW:",
                    "  ↑ ↓            - Select pod",
                    "  Shift+↑ ↓      - Select container within pod",
                    "  Shift+← →      - Switch between kernel/container log tabs",
                    "  e              - Enter expanded logs mode",
                    "",
                ]);
            }
        }
        CurrentView::Logs => {
            if app.global_logs_expanded {
                help_text.extend(vec![
                    "LOGS - EXPANDED MODE:",
                    "  ↑ ↓            - Scroll logs line by line",
                    "  Page Up/Down   - Scroll logs page by page",
                    "  w              - Toggle line wrapping",
                    "  e / Esc        - Exit expanded logs mode",
                    "",
                    "LOG SELECTION:",
                    "  Shift+← →      - Switch between FeOS/Kernel logs",
                    "",
                ]);
            } else {
                help_text.extend(vec![
                    "LOGS VIEW:",
                    "  Shift+← →      - Switch between FeOS/Kernel log tabs",
                    "  e              - Enter expanded logs mode",
                    "",
                ]);
            }
        }
    }

    help_text.extend(vec![
        "HELP NAVIGATION:",
        "  ↑ ↓            - Scroll help content",
        "  Page Up/Down   - Fast scroll help content",
        "",
        "Press 'h' or Esc to close this help."
    ]);
    
    // Apply scrolling - only show visible lines
    let visible_height = popup_area.height.saturating_sub(2) as usize; // Account for borders
    let start_index = app.help_scroll_offset;
    let end_index = (start_index + visible_height).min(help_text.len());
    
    let visible_help_text = if start_index < help_text.len() {
        &help_text[start_index..end_index]
    } else {
        &[]
    };
    
    let help_content = visible_help_text.join("\n");
    
    // Show scroll indicator in title
    let scroll_indicator = if help_text.len() > visible_height {
        format!(" (Scroll: {}/{})", app.help_scroll_offset + 1, help_text.len().saturating_sub(visible_height) + 1)
    } else {
        String::new()
    };
    
    let title = format!("Help{}", scroll_indicator);
    
    let help_paragraph = Paragraph::new(help_content)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(help_paragraph, popup_area);
}

pub fn render_tabs(f: &mut Frame, area: Rect, current_view: CurrentView) {
    // Simple tabs layout across the full width
    let titles: Vec<Line> = vec!["Dashboard", "VMs", "Containers", "Isolated Pods", "Logs"]
        .iter()
        .map(|t| Line::from(*t))
        .collect();
    
    let selected = match current_view {
        CurrentView::Dashboard => 0,
        CurrentView::VMs => 1,
        CurrentView::Containers => 2,
        CurrentView::IsolatedPods => 3,
        CurrentView::Logs => 4,
    };
    
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL))
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

pub fn render_system_modal(f: &mut Frame, area: Rect, app: &App) {
    // Show modal menu or confirmation dialog
    if app.system_confirmation.is_some() {
        render_modal_confirmation_dialog(f, area, app);
    } else {
        render_system_action_modal(f, area, app);
    }
}

fn render_system_action_modal(f: &mut Frame, area: Rect, app: &App) {
    // Create a centered popup area
    let popup_area = centered_rect(40, 30, area);
    
    // Clear the background
    f.render_widget(Clear, popup_area);
    
    let actions = App::get_system_actions();
    let action_items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let style = if i == app.selected_system_action {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                match action {
                    SystemAction::Reboot => Style::default(),
                    SystemAction::Shutdown => Style::default(),
                    SystemAction::Cancel => Style::default(),
                }
            };
            
            ListItem::new(action.as_str()).style(style)
        })
        .collect();

    let menu = List::new(action_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("System Actions"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(menu, popup_area);
}

fn render_modal_confirmation_dialog(f: &mut Frame, area: Rect, app: &App) {
    if let Some(action) = app.system_confirmation {
        let message = format!(
            "Are you sure you want to {}?\n\nPress [y] to confirm or [n]/[Esc] to cancel.",
            action.as_str().to_lowercase()
        );
        
        // Create a centered popup area
        let popup_area = centered_rect(50, 30, area);
        
        // Clear the background
        f.render_widget(Clear, popup_area);
        
        let dialog = Paragraph::new(message)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Confirmation Required")
                    .style(Style::default().fg(Color::Red)),
            )
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

        f.render_widget(dialog, popup_area);
    }
}

/// helper function to create a centered rect using up certain percentage of the available rect `r`
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
} 