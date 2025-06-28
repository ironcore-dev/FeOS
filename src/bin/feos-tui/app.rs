use std::time::{Duration, Instant};
use crate::mock_data::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurrentView {
    Dashboard,
    VMs,
    Containers,
    IsolatedPods,
    Logs,
}

impl CurrentView {
    pub fn titles() -> Vec<&'static str> {
        vec!["Dashboard", "VMs", "Containers", "Isolated Pods", "Logs"]
    }
    
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => CurrentView::Dashboard,
            1 => CurrentView::VMs,
            2 => CurrentView::Containers,
            3 => CurrentView::IsolatedPods,
            4 => CurrentView::Logs,
            _ => CurrentView::Dashboard,
        }
    }
    
    pub fn to_index(self) -> usize {
        match self {
            CurrentView::Dashboard => 0,
            CurrentView::VMs => 1,
            CurrentView::Containers => 2,
            CurrentView::IsolatedPods => 3,
            CurrentView::Logs => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemAction {
    Reboot,
    Shutdown,
    Cancel,
}

impl SystemAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemAction::Reboot => "Reboot System",
            SystemAction::Shutdown => "Shutdown System",
            SystemAction::Cancel => "Cancel",
        }
    }
}

pub struct App {
    pub should_quit: bool,
    pub current_view: CurrentView,
    pub host_info: HostInfo,
    pub vms: Vec<VmInfo>,
    pub containers: Vec<ContainerInfo>,
    pub isolated_pods: Vec<IsolatedPodInfo>,
    pub cpu_history: Vec<u64>,
    pub memory_history: Vec<u64>,
    pub feos_logs: Vec<LogEntry>,
    pub kernel_logs: Vec<LogEntry>,
    pub selected_vm_index: usize,
    pub selected_container_index: usize,
    pub container_logs_expanded: bool, // Whether container logs are in expanded view
    pub selected_isolated_pod_index: usize,
    pub selected_pod_container_index: usize,
    pub selected_pod_log_tab: usize, // 0 = kernel logs, 1 = container logs
    pub logs_expanded: bool, // Whether logs are in expanded view
    pub selected_global_log_tab: usize, // 0 = feos logs, 1 = kernel logs
    pub global_logs_expanded: bool, // Whether global logs are in expanded view
    pub log_scroll_offset: usize, // For scrolling logs
    pub log_line_wrap: bool, // Whether to wrap long log lines
    pub help_modal_open: bool, // Whether help modal is open
    pub help_scroll_offset: usize, // For scrolling help content
    pub selected_system_action: usize,
    pub system_modal_open: bool,
    pub system_confirmation: Option<SystemAction>,
    last_update: Instant,
    last_log_update: Instant,
}

impl Default for App {
    fn default() -> App {
        App {
            should_quit: false,
            current_view: CurrentView::Dashboard,
            host_info: get_mock_host_info(),
            vms: get_mock_vms(),
            containers: get_mock_containers(),
            isolated_pods: get_mock_isolated_pods(),
            cpu_history: get_mock_cpu_history(),
            memory_history: get_mock_memory_history(),
            feos_logs: get_mock_feos_logs(),
            kernel_logs: get_mock_kernel_logs(),
            selected_vm_index: 0,
            selected_container_index: 0,
            container_logs_expanded: false,
            selected_isolated_pod_index: 0,
            selected_pod_container_index: 0,
            selected_pod_log_tab: 0,
            logs_expanded: false,
            selected_global_log_tab: 0,
            global_logs_expanded: false,
            log_scroll_offset: 0,
            log_line_wrap: true,
            help_modal_open: false,
            help_scroll_offset: 0,
            selected_system_action: 0,
            system_modal_open: false,
            system_confirmation: None,
            last_update: Instant::now(),
            last_log_update: Instant::now(),
        }
    }
}

impl App {
    pub fn tick(&mut self) {
        let now = Instant::now();
        
        // Update polling data every 2 seconds (simulates polling endpoints)
        if now.duration_since(self.last_update) >= Duration::from_secs(2) {
            tick_updates();
            self.host_info = get_mock_host_info();
            self.vms = get_mock_vms();
            self.containers = get_mock_containers();
            self.isolated_pods = get_mock_isolated_pods();
            self.cpu_history = get_mock_cpu_history();
            self.memory_history = get_mock_memory_history();
            
            // Ensure selected VM index is still valid after update
            if self.selected_vm_index >= self.vms.len() && !self.vms.is_empty() {
                self.selected_vm_index = self.vms.len() - 1;
            }
            
            // Ensure selected container index is still valid after update
            if self.selected_container_index >= self.containers.len() && !self.containers.is_empty() {
                self.selected_container_index = self.containers.len() - 1;
            }
            
            // Ensure selected isolated pod index is still valid after update
            if self.selected_isolated_pod_index >= self.isolated_pods.len() && !self.isolated_pods.is_empty() {
                self.selected_isolated_pod_index = self.isolated_pods.len() - 1;
            }
            
            // Ensure selected pod container index is still valid after update
            if let Some(pod) = self.isolated_pods.get(self.selected_isolated_pod_index) {
                if self.selected_pod_container_index >= pod.containers.len() && !pod.containers.is_empty() {
                    self.selected_pod_container_index = pod.containers.len() - 1;
                }
            }
            
            self.last_update = now;
        }
        
        // Update streaming logs every 3 seconds (simulates streaming endpoints)
        if now.duration_since(self.last_log_update) >= Duration::from_secs(3) {
            // Add new log entries (simulate streaming)
            let new_feos_logs = get_new_feos_log_entries();
            let new_kernel_logs = get_new_kernel_log_entries();
            
            // Keep only the last 50 log entries to prevent memory growth
            self.feos_logs.extend(new_feos_logs);
            if self.feos_logs.len() > 50 {
                self.feos_logs.drain(0..self.feos_logs.len() - 50);
            }
            
            self.kernel_logs.extend(new_kernel_logs);
            if self.kernel_logs.len() > 50 {
                self.kernel_logs.drain(0..self.kernel_logs.len() - 50);
            }
            
            self.last_log_update = now;
        }
    }
    
    pub fn select_next_vm(&mut self) {
        if !self.vms.is_empty() {
            self.selected_vm_index = (self.selected_vm_index + 1) % self.vms.len();
        }
    }
    
    pub fn select_previous_vm(&mut self) {
        if !self.vms.is_empty() {
            if self.selected_vm_index == 0 {
                self.selected_vm_index = self.vms.len() - 1;
            } else {
                self.selected_vm_index -= 1;
            }
        }
    }
    
    pub fn get_selected_vm(&self) -> Option<&VmInfo> {
        self.vms.get(self.selected_vm_index)
    }
    
    pub fn select_next_container(&mut self) {
        if !self.containers.is_empty() {
            self.selected_container_index = (self.selected_container_index + 1) % self.containers.len();
        }
    }
    
    pub fn select_previous_container(&mut self) {
        if !self.containers.is_empty() {
            if self.selected_container_index == 0 {
                self.selected_container_index = self.containers.len() - 1;
            } else {
                self.selected_container_index -= 1;
            }
        }
    }
    
    pub fn get_selected_container(&self) -> Option<&ContainerInfo> {
        self.containers.get(self.selected_container_index)
    }
    
    pub fn select_next_isolated_pod(&mut self) {
        if !self.isolated_pods.is_empty() {
            self.selected_isolated_pod_index = (self.selected_isolated_pod_index + 1) % self.isolated_pods.len();
            // Reset container selection when switching pods
            self.selected_pod_container_index = 0;
        }
    }
    
    pub fn select_previous_isolated_pod(&mut self) {
        if !self.isolated_pods.is_empty() {
            if self.selected_isolated_pod_index == 0 {
                self.selected_isolated_pod_index = self.isolated_pods.len() - 1;
            } else {
                self.selected_isolated_pod_index -= 1;
            }
            // Reset container selection when switching pods
            self.selected_pod_container_index = 0;
        }
    }
    
    pub fn get_selected_isolated_pod(&self) -> Option<&IsolatedPodInfo> {
        self.isolated_pods.get(self.selected_isolated_pod_index)
    }
    
    pub fn select_next_pod_container(&mut self) {
        if let Some(pod) = self.get_selected_isolated_pod() {
            if !pod.containers.is_empty() {
                self.selected_pod_container_index = (self.selected_pod_container_index + 1) % pod.containers.len();
            }
        }
    }
    
    pub fn select_previous_pod_container(&mut self) {
        if let Some(pod) = self.get_selected_isolated_pod() {
            if !pod.containers.is_empty() {
                if self.selected_pod_container_index == 0 {
                    self.selected_pod_container_index = pod.containers.len() - 1;
                } else {
                    self.selected_pod_container_index -= 1;
                }
            }
        }
    }
    
    pub fn get_selected_pod_container(&self) -> Option<&IsolatedPodContainer> {
        self.get_selected_isolated_pod()
            .and_then(|pod| pod.containers.get(self.selected_pod_container_index))
    }
    
    pub fn switch_pod_log_tab(&mut self) {
        self.selected_pod_log_tab = (self.selected_pod_log_tab + 1) % 2; // Toggle between 0 and 1
    }
    
    pub fn toggle_logs_expanded(&mut self) {
        self.logs_expanded = !self.logs_expanded;
        // Reset scroll when toggling expanded mode
        self.log_scroll_offset = 0;
    }
    
    pub fn switch_global_log_tab(&mut self) {
        self.selected_global_log_tab = (self.selected_global_log_tab + 1) % 2; // Toggle between 0 and 1
    }
    
    pub fn toggle_global_logs_expanded(&mut self) {
        self.global_logs_expanded = !self.global_logs_expanded;
        // Reset scroll when toggling expanded mode
        self.log_scroll_offset = 0;
    }

    pub fn toggle_container_logs_expanded(&mut self) {
        self.container_logs_expanded = !self.container_logs_expanded;
        // Reset scroll when toggling expanded mode
        self.log_scroll_offset = 0;
    }
    
    pub fn scroll_logs_up(&mut self) {
        if self.log_scroll_offset > 0 {
            self.log_scroll_offset -= 1;
        }
    }
    
    pub fn scroll_logs_down(&mut self) {
        self.log_scroll_offset += 1;
    }
    
    pub fn scroll_logs_page_up(&mut self) {
        self.log_scroll_offset = self.log_scroll_offset.saturating_sub(10);
    }
    
    pub fn scroll_logs_page_down(&mut self) {
        self.log_scroll_offset += 10;
    }
    
    pub fn toggle_log_line_wrap(&mut self) {
        self.log_line_wrap = !self.log_line_wrap;
    }
    
    pub fn open_help_modal(&mut self) {
        self.help_modal_open = true;
        self.help_scroll_offset = 0; // Reset scroll when opening
    }
    
    pub fn close_help_modal(&mut self) {
        self.help_modal_open = false;
        self.help_scroll_offset = 0;
    }
    
    pub fn scroll_help_up(&mut self) {
        if self.help_scroll_offset > 0 {
            self.help_scroll_offset -= 1;
        }
    }
    
    pub fn scroll_help_down(&mut self) {
        self.help_scroll_offset += 1;
    }
    
    pub fn select_next_system_action(&mut self) {
        self.selected_system_action = (self.selected_system_action + 1) % 3; // 3 actions: reboot, shutdown, cancel
    }
    
    pub fn select_previous_system_action(&mut self) {
        if self.selected_system_action == 0 {
            self.selected_system_action = 2;
        } else {
            self.selected_system_action -= 1;
        }
    }
    
    pub fn get_system_actions() -> Vec<SystemAction> {
        vec![SystemAction::Reboot, SystemAction::Shutdown, SystemAction::Cancel]
    }
    
    pub fn open_system_modal(&mut self) {
        self.system_modal_open = true;
        self.selected_system_action = 0; // Reset to first option
    }
    
    pub fn close_system_modal(&mut self) {
        self.system_modal_open = false;
        self.system_confirmation = None;
    }
    
    pub fn trigger_system_confirmation(&mut self) {
        let actions = Self::get_system_actions();
        if let Some(action) = actions.get(self.selected_system_action) {
            match action {
                SystemAction::Cancel => {
                    // Close modal immediately for Cancel
                    self.close_system_modal();
                }
                _ => {
                    // Show confirmation dialog for Reboot/Shutdown
                    self.system_confirmation = Some(*action);
                }
            }
        }
    }
    
    pub fn confirm_system_action(&mut self) {
        if let Some(action) = self.system_confirmation {
            // TODO: Implement actual system actions here
            match action {
                SystemAction::Reboot => {
                    // Placeholder for reboot logic
                }
                SystemAction::Shutdown => {
                    // Placeholder for shutdown logic
                }
                SystemAction::Cancel => {
                    // Just cancel, no action needed
                }
            }
        }
        self.close_system_modal();
    }
    
    pub fn cancel_system_action(&mut self) {
        self.close_system_modal();
    }
} 