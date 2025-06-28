use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};
use crate::app::App;

pub fn render_system_view(f: &mut Frame, area: Rect, app: &App) {
    // Render the system menu in the full area
    render_system_menu(f, area, app);
    
    // Show modal confirmation dialog if needed
    if app.system_confirmation.is_some() {
        render_modal_confirmation_dialog(f, area, app);
    }
}

fn render_system_menu(f: &mut Frame, area: Rect, app: &App) {
    use crate::app::{App, SystemAction};
    
    let actions = App::get_system_actions();
    let action_items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let style = if i == app.selected_system_action {
                Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                match action {
                    SystemAction::Reboot => Style::default().fg(Color::Green),
                    SystemAction::Shutdown => Style::default().fg(Color::Red),
                }
            };
            
            ListItem::new(action.as_str()).style(style)
        })
        .collect();

    let menu = List::new(action_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("System Management"),
        )
        .style(Style::default().fg(Color::White));

    f.render_widget(menu, area);
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