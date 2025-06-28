use crossterm::event::{self, Event, KeyCode};
use color_eyre::eyre::Result;
use std::time::Duration;
use crate::app::{App, CurrentView};

pub fn handle_events(app: &mut App) -> Result<()> {
    if crossterm::event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            // If confirmation dialog is active, only handle confirmation keys
            if app.current_view == CurrentView::System && app.system_confirmation.is_some() {
                match key.code {
                    KeyCode::Char('y') => {
                        app.confirm_system_action();
                    }
                    KeyCode::Char('n') | KeyCode::Esc => {
                        app.cancel_system_action();
                    }
                    _ => {
                        // Block all other keys when confirmation dialog is active
                    }
                }
                return Ok(());
            }

            // Normal key handling when no confirmation dialog is active
            match key.code {
                KeyCode::Char('q') => {
                    app.should_quit = true;
                }
                // Tab navigation shortcuts
                KeyCode::Char('d') => {
                    app.current_view = CurrentView::Dashboard;
                }
                KeyCode::Char('v') => {
                    app.current_view = CurrentView::VMs;
                }
                KeyCode::Char('c') => {
                    app.current_view = CurrentView::Containers;
                }
                KeyCode::Char('l') => {
                    app.current_view = CurrentView::Logs;
                }
                KeyCode::Char('s') => {
                    app.current_view = CurrentView::System;
                }
                // Arrow key navigation
                KeyCode::Left => {
                    let current_index = app.current_view.to_index();
                    let new_index = if current_index == 0 {
                        CurrentView::titles().len() - 1
                    } else {
                        current_index - 1
                    };
                    app.current_view = CurrentView::from_index(new_index);
                }
                KeyCode::Right => {
                    let current_index = app.current_view.to_index();
                    let new_index = (current_index + 1) % CurrentView::titles().len();
                    app.current_view = CurrentView::from_index(new_index);
                }
                // Navigation in current view
                KeyCode::Up => {
                    match app.current_view {
                        CurrentView::VMs => {
                            app.select_previous_vm();
                        }
                        CurrentView::Containers => {
                            app.select_previous_container();
                        }
                        CurrentView::System => {
                            app.select_previous_system_action();
                        }
                        _ => {}
                    }
                }
                KeyCode::Down => {
                    match app.current_view {
                        CurrentView::VMs => {
                            app.select_next_vm();
                        }
                        CurrentView::Containers => {
                            app.select_next_container();
                        }
                        CurrentView::System => {
                            app.select_next_system_action();
                        }
                        _ => {}
                    }
                }
                // Enter key for actions
                KeyCode::Enter => {
                    if app.current_view == CurrentView::System {
                        app.trigger_system_confirmation();
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
} 