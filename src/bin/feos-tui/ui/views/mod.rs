// Re-export view functions from individual modules
mod vms_view;
mod containers_view;
mod logs_view;
mod system_view;

pub use vms_view::render_vms_view;
pub use containers_view::render_containers_view;
pub use logs_view::render_logs_view;
pub use system_view::render_system_view; 