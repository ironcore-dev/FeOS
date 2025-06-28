use crate::mock_data::{format_bytes, format_uptime};

/// Format memory in a human-readable way, showing "No limit" for zero values
pub fn format_memory_limit(bytes: u64) -> String {
    if bytes == 0 {
        "No limit".to_string()
    } else {
        format_bytes(bytes)
    }
}

/// Format CPU limit in a human-readable way, showing "No limit" for zero values
pub fn format_cpu_limit(limit: f64) -> String {
    if limit == 0.0 {
        "No limit".to_string()
    } else {
        format!("{:.1} cores", limit)
    }
}

/// Format uptime from a creation timestamp
pub fn format_uptime_from_created(created: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let uptime_seconds = now.saturating_sub(created);
    format_uptime(uptime_seconds)
} 