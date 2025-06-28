use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use color_eyre::eyre::Result;
use std::time::Duration;
use crate::app::{App, CurrentView};

pub fn handle_events(app: &mut App) -> Result<()> {
    if crossterm::event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            // If help modal is open, only handle help-specific keys
            if app.help_modal_open {
                match key.code {
                    KeyCode::Char('h') | KeyCode::Esc => {
                        app.close_help_modal();
                    }
                    KeyCode::Up => {
                        app.scroll_help_up();
                    }
                    KeyCode::Down => {
                        app.scroll_help_down();
                    }
                    KeyCode::PageUp => {
                        // Scroll help up by multiple lines
                        for _ in 0..5 {
                            app.scroll_help_up();
                        }
                    }
                    KeyCode::PageDown => {
                        // Scroll help down by multiple lines
                        for _ in 0..5 {
                            app.scroll_help_down();
                        }
                    }
                    _ => {
                        // Block all other keys when help modal is active
                    }
                }
                return Ok(());
            }
            
            // If system confirmation dialog is active, only handle confirmation keys
            if app.system_confirmation.is_some() {
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
            
            // If system modal is open, handle modal navigation
            if app.system_modal_open {
                match key.code {
                    KeyCode::Up => {
                        app.select_previous_system_action();
                    }
                    KeyCode::Down => {
                        app.select_next_system_action();
                    }
                    KeyCode::Enter => {
                        app.trigger_system_confirmation();
                    }
                    KeyCode::Esc => {
                        app.close_system_modal();
                    }
                    _ => {}
                }
                return Ok(());
            }

            // Normal key handling when no modal is active
            match (key.code, key.modifiers) {
                // Ctrl+Q opens system modal
                (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                    app.open_system_modal();
                }
                // Regular q quits
                (KeyCode::Char('q'), KeyModifiers::NONE) => {
                    app.should_quit = true;
                }
                // Help modal toggle
                (KeyCode::Char('h'), KeyModifiers::NONE) => {
                    app.open_help_modal();
                }
                // Line wrap toggle (only in expanded logs mode)
                (KeyCode::Char('w'), KeyModifiers::NONE) => {
                    match app.current_view {
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.toggle_log_line_wrap();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.toggle_log_line_wrap();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.toggle_log_line_wrap();
                        }
                        _ => {}
                    }
                }
                // Tab navigation shortcuts
                (KeyCode::Char('d'), KeyModifiers::NONE) => {
                    app.current_view = CurrentView::Dashboard;
                }
                (KeyCode::Char('v'), KeyModifiers::NONE) => {
                    app.current_view = CurrentView::VMs;
                }
                (KeyCode::Char('c'), KeyModifiers::NONE) => {
                    app.current_view = CurrentView::Containers;
                }
                (KeyCode::Char('p'), KeyModifiers::NONE) => {
                    app.current_view = CurrentView::IsolatedPods;
                }
                (KeyCode::Char('l'), KeyModifiers::NONE) => {
                    app.current_view = CurrentView::Logs;
                }
                (KeyCode::Char('e'), KeyModifiers::NONE) => {
                    match app.current_view {
                        CurrentView::IsolatedPods => {
                            app.toggle_logs_expanded();
                        }
                        CurrentView::Containers => {
                            app.toggle_container_logs_expanded();
                        }
                        CurrentView::Logs => {
                            app.toggle_global_logs_expanded();
                        }
                        _ => {}
                    }
                }
                // Escape key to exit expanded logs mode
                (KeyCode::Esc, KeyModifiers::NONE) => {
                    match app.current_view {
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.toggle_logs_expanded();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.toggle_container_logs_expanded();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.toggle_global_logs_expanded();
                        }
                        _ => {}
                    }
                }
                // Page Up/Page Down for page-wise scrolling in expanded logs
                (KeyCode::PageUp, KeyModifiers::NONE) => {
                    match app.current_view {
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.scroll_logs_page_up();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.scroll_logs_page_up();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.scroll_logs_page_up();
                        }
                        _ => {}
                    }
                }
                (KeyCode::PageDown, KeyModifiers::NONE) => {
                    match app.current_view {
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.scroll_logs_page_down();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.scroll_logs_page_down();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.scroll_logs_page_down();
                        }
                        _ => {}
                    }
                }
                // Navigation in current view - handle Shift modifiers first
                (KeyCode::Up, KeyModifiers::SHIFT) => {
                    match app.current_view {
                        CurrentView::IsolatedPods => {
                            app.select_previous_pod_container();
                        }
                        _ => {}
                    }
                }
                (KeyCode::Down, KeyModifiers::SHIFT) => {
                    match app.current_view {
                        CurrentView::IsolatedPods => {
                            app.select_next_pod_container();
                        }
                        _ => {}
                    }
                }
                (KeyCode::Left, KeyModifiers::SHIFT) => {
                    match app.current_view {
                        CurrentView::IsolatedPods => {
                            app.switch_pod_log_tab();
                        }
                        CurrentView::Logs => {
                            app.switch_global_log_tab();
                        }
                        _ => {}
                    }
                }
                (KeyCode::Right, KeyModifiers::SHIFT) => {
                    match app.current_view {
                        CurrentView::IsolatedPods => {
                            app.switch_pod_log_tab();
                        }
                        CurrentView::Logs => {
                            app.switch_global_log_tab();
                        }
                        _ => {}
                    }
                }
                // Arrow key navigation (generic patterns - must come after specific modifier patterns)
                (KeyCode::Left, _) => {
                    let current_index = app.current_view.to_index();
                    let new_index = if current_index == 0 {
                        CurrentView::titles().len() - 1
                    } else {
                        current_index - 1
                    };
                    app.current_view = CurrentView::from_index(new_index);
                }
                (KeyCode::Right, _) => {
                    let current_index = app.current_view.to_index();
                    let new_index = (current_index + 1) % CurrentView::titles().len();
                    app.current_view = CurrentView::from_index(new_index);
                }
                (KeyCode::Up, _) => {
                    match app.current_view {
                        CurrentView::VMs => {
                            app.select_previous_vm();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.scroll_logs_up();
                        }
                        CurrentView::Containers => {
                            app.select_previous_container();
                        }
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.scroll_logs_up();
                        }
                        CurrentView::IsolatedPods => {
                            app.select_previous_isolated_pod();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.scroll_logs_up();
                        }
                        _ => {}
                    }
                }
                (KeyCode::Down, _) => {
                    match app.current_view {
                        CurrentView::VMs => {
                            app.select_next_vm();
                        }
                        CurrentView::Containers if app.container_logs_expanded => {
                            app.scroll_logs_down();
                        }
                        CurrentView::Containers => {
                            app.select_next_container();
                        }
                        CurrentView::IsolatedPods if app.logs_expanded => {
                            app.scroll_logs_down();
                        }
                        CurrentView::IsolatedPods => {
                            app.select_next_isolated_pod();
                        }
                        CurrentView::Logs if app.global_logs_expanded => {
                            app.scroll_logs_down();
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
} 