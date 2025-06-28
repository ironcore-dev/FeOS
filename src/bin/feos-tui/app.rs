use std::time::{Duration, Instant};
use crate::mock_data::*;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CurrentView {
    Dashboard,
    VMs,
    Containers,
    Logs,
    System,
}

impl CurrentView {
    pub fn titles() -> Vec<&'static str> {
        vec!["Dashboard", "VMs", "Containers", "Logs", "System"]
    }
    
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => CurrentView::Dashboard,
            1 => CurrentView::VMs,
            2 => CurrentView::Containers,
            3 => CurrentView::Logs,
            4 => CurrentView::System,
            _ => CurrentView::Dashboard,
        }
    }
    
    pub fn to_index(self) -> usize {
        match self {
            CurrentView::Dashboard => 0,
            CurrentView::VMs => 1,
            CurrentView::Containers => 2,
            CurrentView::Logs => 3,
            CurrentView::System => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemAction {
    Reboot,
    Shutdown,
}

impl SystemAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemAction::Reboot => "Reboot System",
            SystemAction::Shutdown => "Shutdown System",
        }
    }
}

pub struct App {
    pub should_quit: bool,
    pub current_view: CurrentView,
    pub host_info: HostInfo,
    pub vms: Vec<VmInfo>,
    pub containers: Vec<ContainerInfo>,
    pub cpu_history: Vec<u64>,
    pub memory_history: Vec<u64>,
    pub feos_logs: Vec<LogEntry>,
    pub kernel_logs: Vec<LogEntry>,
    pub selected_vm_index: usize,
    pub selected_container_index: usize,
    pub selected_system_action: usize,
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
            cpu_history: get_mock_cpu_history(),
            memory_history: get_mock_memory_history(),
            feos_logs: get_mock_feos_logs(),
            kernel_logs: get_mock_kernel_logs(),
            selected_vm_index: 0,
            selected_container_index: 0,
            selected_system_action: 0,
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
    
    pub fn select_next_system_action(&mut self) {
        self.selected_system_action = (self.selected_system_action + 1) % 2; // 2 actions: reboot, shutdown
    }
    
    pub fn select_previous_system_action(&mut self) {
        if self.selected_system_action == 0 {
            self.selected_system_action = 1;
        } else {
            self.selected_system_action -= 1;
        }
    }
    
    pub fn get_system_actions() -> Vec<SystemAction> {
        vec![SystemAction::Reboot, SystemAction::Shutdown]
    }
    
    pub fn trigger_system_confirmation(&mut self) {
        let actions = Self::get_system_actions();
        if let Some(action) = actions.get(self.selected_system_action) {
            self.system_confirmation = Some(*action);
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
            }
        }
        self.system_confirmation = None;
    }
    
    pub fn cancel_system_action(&mut self) {
        self.system_confirmation = None;
    }
} 