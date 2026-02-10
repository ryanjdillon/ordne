use crate::error::{PruneError, Result};
use std::path::{Path};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub device_id: Option<String>,
    pub device_path: Option<String>,
    pub uuid: Option<String>,
    pub mount_path: Option<String>,
    pub fs_type: Option<String>,
    pub total_bytes: Option<i64>,
    pub model: Option<String>,
    pub serial: Option<String>,
}

impl DeviceInfo {
    pub fn new() -> Self {
        Self {
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: None,
            fs_type: None,
            total_bytes: None,
            model: None,
            serial: None,
        }
    }
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// Discovers device information for a given mount path
///
/// This queries multiple sources:
/// - `/dev/disk/by-id/` for stable device identifiers
/// - `blkid` for filesystem UUID
/// - `findmnt` for mount point and filesystem type
/// - `lsblk` for size, model, serial number
pub fn discover_device<P: AsRef<Path>>(mount_path: P) -> Result<DeviceInfo> {
    let mount_path = mount_path.as_ref();
    let mut info = DeviceInfo::new();

    info.mount_path = Some(mount_path.to_string_lossy().to_string());

    if let Ok(output) = Command::new("findmnt")
        .arg("-n")
        .arg("-o")
        .arg("SOURCE,FSTYPE")
        .arg(mount_path)
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
            if parts.len() >= 2 {
                info.device_path = Some(parts[0].to_string());
                info.fs_type = Some(parts[1].to_string());
            }
        }
    }

    if let Some(device_path) = &info.device_path {
        if let Ok(output) = Command::new("blkid")
            .arg("-s")
            .arg("UUID")
            .arg("-o")
            .arg("value")
            .arg(device_path)
            .output()
        {
            if output.status.success() {
                let uuid = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !uuid.is_empty() {
                    info.uuid = Some(uuid);
                }
            }
        }

        if let Ok(output) = Command::new("lsblk")
            .arg("-n")
            .arg("-o")
            .arg("SIZE,MODEL,SERIAL")
            .arg(device_path)
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let parts: Vec<&str> = stdout.trim().split_whitespace().collect();
                if !parts.is_empty() {
                    if let Ok(size) = parse_size(parts[0]) {
                        info.total_bytes = Some(size);
                    }
                    if parts.len() > 1 {
                        info.model = Some(parts[1].to_string());
                    }
                    if parts.len() > 2 {
                        info.serial = Some(parts[2].to_string());
                    }
                }
            }
        }

        let device_name = device_path
            .trim_start_matches("/dev/")
            .replace('/', "-");
        let by_id_dir = Path::new("/dev/disk/by-id");
        if by_id_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(by_id_dir) {
                for entry in entries.flatten() {
                    if let Ok(target) = std::fs::read_link(entry.path()) {
                        let target_name = target
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        if target_name == device_name || target.to_string_lossy().contains(&device_name) {
                            info.device_id = Some(entry.file_name().to_string_lossy().to_string());
                            break;
                        }
                    }
                }
            }
        }
    }

    Ok(info)
}

/// Discovers rclone remote information
///
/// Queries `rclone about <remote>:` for backend information
pub fn discover_rclone_remote(remote: &str) -> Result<DeviceInfo> {
    let mut info = DeviceInfo::new();

    let output = Command::new("rclone")
        .arg("about")
        .arg(format!("{}:", remote))
        .arg("--json")
        .output()
        .map_err(|e| PruneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!("Failed to execute rclone: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PruneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!("rclone failed: {}", stderr),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
        if let Some(total) = json.get("total").and_then(|v| v.as_i64()) {
            info.total_bytes = Some(total);
        }
    }

    info.mount_path = Some(format!("{}:", remote));
    info.fs_type = Some("rclone".to_string());

    Ok(info)
}

fn parse_size(size_str: &str) -> Result<i64> {
    let size_str = size_str.trim().to_uppercase();
    let multiplier = if size_str.ends_with('K') {
        1024
    } else if size_str.ends_with('M') {
        1024 * 1024
    } else if size_str.ends_with('G') {
        1024 * 1024 * 1024
    } else if size_str.ends_with('T') {
        1024_i64 * 1024 * 1024 * 1024
    } else {
        1
    };

    let number_part = size_str
        .trim_end_matches(|c: char| !c.is_numeric() && c != '.')
        .parse::<f64>()
        .map_err(|e| PruneError::Config(format!("Invalid size format: {}", e)))?;

    Ok((number_part * multiplier as f64) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("1K").unwrap(), 1024);
        assert_eq!(parse_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("1.5G").unwrap(), (1.5 * 1024.0 * 1024.0 * 1024.0) as i64);
    }

    #[test]
    fn test_device_info_default() {
        let info = DeviceInfo::default();
        assert!(info.device_id.is_none());
        assert!(info.uuid.is_none());
    }
}
