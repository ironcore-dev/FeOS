# FeOS TUI

This is the terminal user interface for FeOS.

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

## Development

The TUI is built using [ratatui](https://ratatui.rs).

At the moment, the TUI is in a very early stage of development and uses mocked data.
The goal is to provide a comprehensive and easy-to-use interface to manage and monitor a FeOS instance.

### Running the TUI

**Interactive Mode:**
```bash
cargo run --bin feos-tui
```

**Testing Mode:**
```bash
# Run comprehensive mock data tests
cargo run --bin feos-tui -- --test --verbose

# Test a specific view for 2 seconds (default)
cargo run --bin feos-tui -- --test-view dashboard
cargo run --bin feos-tui -- --test-view vms
cargo run --bin feos-tui -- --test-view logs
cargo run --bin feos-tui -- --test-view system

# Test with custom duration and verbose output
cargo run --bin feos-tui -- --test-view dashboard --duration 5 --verbose
```

**CLI Options:**
- `--test`: Run automated mock data tests and exit
- `--test-view <VIEW>`: Test a specific view (dashboard, vms, logs, system)
- `--duration <SECONDS>`: Duration to show test view (default: 2 seconds)
- `--verbose`: Enable verbose output in test modes
- `--help`: Show all available options

### Features Status

- [x] **Dashboard view** - Complete with system overview, resource monitoring, VM status, and network interfaces
- [x] **Virtual Machine management** - Complete VM listing, details, selection, and action interface
- [x] **System management** - Complete server control interface with restart/shutdown operations
- [x] **System information** - Host info, resource gauges, and real-time monitoring
- [x] **Logs viewer** - Dual-pane log display with color-coding and real-time updates
- [x] **Tab navigation** - Keyboard shortcuts and arrow key navigation (d/v/l/s keys)
- [x] **VM selection** - Interactive table selection with up/down arrow keys
- [x] **Dynamic mock data** - Realistic simulation with state changes and streaming updates
- [ ] **Backend integration** - gRPC API connection (planned)
- [ ] **System actions** - Actual server control operations (planned)
- [ ] **VM actions** - Actual VM control operations (planned) 