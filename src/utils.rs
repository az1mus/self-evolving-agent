/// Utility functions for SEA (Self-Evolved Agent)
use std::path::PathBuf;

/// Set up logging for the application
pub fn setup_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
}

/// Get the configuration directory path
#[allow(dead_code)]
pub fn get_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".sea")
}

/// Ensure the configuration directory exists, creating it if necessary
#[allow(dead_code)]
pub fn ensure_config_dir_exists() -> std::io::Result<PathBuf> {
    let config_dir = get_config_dir();
    std::fs::create_dir_all(&config_dir)?;
    Ok(config_dir)
}

/// Format bytes as human-readable string
#[allow(dead_code)]
pub fn format_bytes(size_bytes: u64) -> String {
    if size_bytes == 0 {
        return "0 B".to_string();
    }

    let size_names = ["B", "KB", "MB", "GB", "TB"];
    let mut size = size_bytes as f64;
    let mut i = 0;

    while size >= 1024.0 && i < size_names.len() - 1 {
        size /= 1024.0;
        i += 1;
    }

    format!("{:.1} {}", size, size_names[i])
}

/// Truncate text to a maximum length
#[allow(dead_code)]
pub fn truncate_text(text: &str, max_length: usize, suffix: &str) -> String {
    if text.len() <= max_length {
        text.to_string()
    } else {
        let max = max_length.saturating_sub(suffix.len());
        format!("{}{}", &text[..max], suffix)
    }
}

/// Mask a sensitive string (like API keys) for display
pub fn mask_sensitive(value: &str, visible_chars: usize) -> String {
    if value.len() <= visible_chars {
        "*".repeat(value.len())
    } else {
        "*".repeat(value.len() - visible_chars) + &value[value.len() - visible_chars..]
    }
}
