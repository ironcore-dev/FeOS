// Re-export view functions from individual modules
mod vms_view;
mod containers_view;
mod isolated_pods_view;
mod logs_view;

pub use vms_view::render_vms_view;
pub use containers_view::render_containers_view;
pub use isolated_pods_view::render_isolated_pods_view;
pub use logs_view::render_logs_view; 