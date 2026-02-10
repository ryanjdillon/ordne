use chrono::{DateTime, Local, Utc};
use std::time::Duration;

pub fn format_bytes(bytes: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;
    const TB: i64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

pub fn format_timestamp(dt: &DateTime<Utc>) -> String {
    let local: DateTime<Local> = DateTime::from(dt.clone());
    local.format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn parse_size_string(s: &str) -> Result<i64, String> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if let Some(stripped) = s.strip_suffix("TB") {
        (stripped, 1024_i64.pow(4))
    } else if let Some(stripped) = s.strip_suffix("GB") {
        (stripped, 1024_i64.pow(3))
    } else if let Some(stripped) = s.strip_suffix("MB") {
        (stripped, 1024_i64.pow(2))
    } else if let Some(stripped) = s.strip_suffix("KB") {
        (stripped, 1024)
    } else if let Some(stripped) = s.strip_suffix("B") {
        (stripped, 1)
    } else {
        (&*s, 1)
    };

    let num: f64 = num_str
        .trim()
        .parse()
        .map_err(|_| format!("Invalid size value: {}", s))?;

    Ok((num * multiplier as f64) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1024_i64.pow(4)), "1.00 TB");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(3665)), "1h 1m 5s");
    }

    #[test]
    fn test_parse_size_string() {
        assert_eq!(parse_size_string("512B").unwrap(), 512);
        assert_eq!(parse_size_string("1KB").unwrap(), 1024);
        assert_eq!(parse_size_string("1.5 MB").unwrap(), 1_572_864);
        assert_eq!(parse_size_string("2GB").unwrap(), 2_147_483_648);
        assert_eq!(parse_size_string("100").unwrap(), 100);
    }
}
