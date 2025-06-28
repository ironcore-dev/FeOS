# FeOS TUI - Terminal User Interface

A modern, interactive terminal user interface for monitoring and managing FeOS containers, virtual machines, and isolated pods. Built with Rust and [Ratatui](https://github.com/ratatui-org/ratatui).

## User Experience

The FeOS TUI is designed to be a responsive, keyboard-driven interface for managing a FeOS instance. The experience is centered around:

- **Modal Interaction**: Different views and pop-ups will have their own context-sensitive controls.
- **Immediate Feedback**: The UI will react to user input instantly.
- **Discoverability**: A persistent help/footer bar will display available keybindings for the current view.
- **Simplicity**: The layout is clean and prioritizes showing the most relevant information without clutter.

## Design and Layout

The TUI will be built using the [ratatui](https://ratatui.rs) library, making extensive use of its widget and layout system.

### Overall Structure

The main screen is divided into three parts:
1.  **Header**: A `Block` widget at the top displaying the "FeOS TUI" title.
2.  **Main Content**: A `Tabs` widget will allow switching between the main views: "Dashboard", "VMs", and "Logs". The content of this area will change based on the selected tab.
3.  **Footer**: A `Paragraph` at the bottom of the screen that displays the keybindings relevant to the currently active view or widget.

### Views

#### Dashboard (Default View)

The dashboard provides a high-level overview of the FeOS host. It will be composed of:
- A `Layout` manager splitting the view into several sections.
- **Host Information**: A `Block` containing a `Paragraph` with details like uptime, CPU count, and total/unused RAM.
- **Resource Utilization**: `Gauge` or `Sparkline` widgets will provide a live look at CPU and Memory usage.
- **VMs Overview**: A `List` widget showing currently running VMs for a quick status check.
- **Network Interfaces**: A `Table` will list the available network interfaces with their MAC and PCI addresses.

#### VMs View

This view is for managing Virtual Machines and is now fully implemented with:
- **VM Table**: A `Table` widget listing all VMs with columns for `UUID`, `Status`, `Image`, `CPUs`, and `Memory`. Status is color-coded (Green=Running, Red=Stopped, Yellow=Starting/Stopping, Magenta=Error).
- **VM Details Panel**: Shows detailed information for the selected VM including UUID, status, image details, CPU/memory allocation, and VM configuration.
- **Actions Panel**: Lists available VM management actions like Boot, Shutdown, Restart, Console Access, and Delete VM.
- **Dynamic Layout**: The view uses a 60/40 split between the VM table and details/actions panels for optimal information display.

#### Logs View

For monitoring the system, now fully implemented with:
- **Dual Log Display**: Side-by-side view showing both FeOS and Kernel logs simultaneously.
- **Color-coded Entries**: Log levels are color-coded (Red=ERROR, Yellow=WARN, Green/Blue=INFO) for easy identification.
- **Real-time Updates**: Logs are dynamically generated and updated, showing the newest entries first.
- **Performance Optimized**: Display is limited to 20 most recent entries per log source to maintain responsiveness.

#### System View

For server management operations, now fully implemented with:
- **Scrollable Action Menu**: Simple menu with two options:
  - **Reboot System** (Green) - Restart the entire system
  - **Shutdown System** (Red) - Power down the system
- **Selection Highlighting**: Selected action is highlighted with blue background
- **Confirmation Dialog**: When Enter is pressed, shows confirmation prompt
- **Safety Confirmation**: User must confirm with Enter or cancel with Esc
- **Right-aligned Tab**: System tab is positioned on the right side of the tab bar for visual separation
- **Keyboard Navigation**: ↑↓ to select, Enter to confirm, Esc to cancel

## Navigation

- **Tab Switching**: The user can switch between main views using `d` (Dashboard), `v` (VMs), `l` (Logs), and `s` (System), or with the arrow keys on the tab bar.
- **List/Table Navigation**: The `Up` and `Down` arrow keys are used to select items in lists and tables (e.g., VM selection in VMs view, system action selection in System view).
- **System Actions**: In the System view, use ↑↓ to select action, Enter to trigger confirmation dialog, Enter again to confirm, or Esc to cancel.
- **Actions**: The `Enter` key is used to trigger the primary action for a selected item. Other actions have dedicated character keybindings.
- **Exiting**: The `q` key is used to quit the application. `Esc` cancels confirmation dialogs.

## Architecture

The TUI is built with a modular architecture for maintainability and reusability:

### Core Components

- `main.rs` - Application entry point and setup
- `app.rs` - Main application state and logic  
- `events.rs` - Event handling and user input processing
- `terminal.rs` - Terminal setup and restoration

### UI Modules

- `ui/components.rs` - Reusable UI components (header, help modal, system actions)
- `ui/dashboard.rs` - Main dashboard view with system overview
- `ui/log_components.rs` - Shared log rendering components with scrolling and wrapping
- `ui/utils.rs` - Common formatting utilities
- `ui/views/` - Specific view implementations (VMs, containers, isolated pods, logs)

### Mock Data

- `mock_data.rs` - Centralized mock data generation with shared container log functions

### Key Design Principles

- **Modularity**: Shared components eliminate code duplication
- **Reusability**: Common log rendering and formatting utilities
- **Consistency**: Unified log display with chronological ordering (old to new)
- **Performance**: Bounded scrolling and efficient rendering

## Development

The TUI is built using [ratatui](https://ratatui.rs) with a focus on maintainable, modular code.

Mock data is used throughout for development, making it easy to test various scenarios and UI states without requiring a full FeOS environment.

### Building and Running

```bash
# Build the TUI
cargo build --bin feos-tui

# Run the TUI
cargo run --bin feos-tui

# Build in release mode
cargo build --release --bin feos-tui
```

### Features Status

- [x] **Dashboard view** - Complete with system overview, resource monitoring, VM status, and network interfaces
- [x] **Virtual Machine management** - Complete VM listing, details, selection
- [x] **System management** - Complete server control interface with restart/shutdown operations
- [x] **System information** - Host info, resource gauges, and real-time monitoring
- [x] **Logs viewer** - Dual-pane log display with color-coding and real-time updates
- [x] **Tab navigation** - Keyboard shortcuts and arrow key navigation (d/v/l/s keys)
- [x] **VM selection** - Interactive table selection with up/down arrow keys
- [x] **Dynamic mock data** - Realistic simulation with state changes and streaming updates
- [ ] **Backend integration** - gRPC API connection (planned)
- [ ] **System actions** - Actual server control operations (planned)