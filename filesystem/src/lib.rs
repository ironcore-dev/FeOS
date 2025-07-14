mod fsmount;
mod mount;
mod r#move; // Use r# to allow 'move' as a module name

// Re-export public functions
pub use mount::mount_virtual_filesystems;
pub use r#move::{get_root_fstype, move_root};