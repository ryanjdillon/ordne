use crate::error::{OrdneError, Result};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct RcloneOptions {
    pub checksum: bool,
    pub verbose: bool,
    pub progress: bool,
    pub transfers: Option<u32>,
    pub checkers: Option<u32>,
}

impl Default for RcloneOptions {
    fn default() -> Self {
        Self {
            checksum: true,
            verbose: true,
            progress: true,
            transfers: Some(4),
            checkers: Some(8),
        }
    }
}

pub struct RcloneResult {
    pub success: bool,
    pub output: String,
    pub exit_code: i32,
}

pub fn execute_rclone_copy(
    source: &str,
    dest: &str,
    options: &RcloneOptions,
) -> Result<RcloneResult> {
    let mut cmd = Command::new("rclone");
    cmd.arg("copy");

    if options.checksum {
        cmd.arg("--checksum");
    }

    if options.verbose {
        cmd.arg("--verbose");
    }

    if options.progress {
        cmd.arg("--progress");
    }

    if let Some(transfers) = options.transfers {
        cmd.arg("--transfers");
        cmd.arg(transfers.to_string());
    }

    if let Some(checkers) = options.checkers {
        cmd.arg("--checkers");
        cmd.arg(checkers.to_string());
    }

    cmd.arg(source);
    cmd.arg(dest);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    log::info!("Executing rclone: {:?}", cmd);

    let output = cmd.output().map_err(|e| OrdneError::ExternalTool {
        tool: "rclone".to_string(),
        message: format!("Failed to execute rclone: {}", e),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined_output = format!("{}\n{}", stdout, stderr);

    let exit_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();

    if !success {
        log::error!("rclone failed with exit code {}: {}", exit_code, stderr);
    }

    Ok(RcloneResult {
        success,
        output: combined_output,
        exit_code,
    })
}

pub fn execute_rclone_move(
    source: &str,
    dest: &str,
    options: &RcloneOptions,
) -> Result<RcloneResult> {
    let mut cmd = Command::new("rclone");
    cmd.arg("move");

    if options.checksum {
        cmd.arg("--checksum");
    }

    if options.verbose {
        cmd.arg("--verbose");
    }

    if options.progress {
        cmd.arg("--progress");
    }

    if let Some(transfers) = options.transfers {
        cmd.arg("--transfers");
        cmd.arg(transfers.to_string());
    }

    if let Some(checkers) = options.checkers {
        cmd.arg("--checkers");
        cmd.arg(checkers.to_string());
    }

    cmd.arg(source);
    cmd.arg(dest);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    log::info!("Executing rclone: {:?}", cmd);

    let output = cmd.output().map_err(|e| OrdneError::ExternalTool {
        tool: "rclone".to_string(),
        message: format!("Failed to execute rclone: {}", e),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined_output = format!("{}\n{}", stdout, stderr);

    let exit_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();

    if !success {
        log::error!("rclone failed with exit code {}: {}", exit_code, stderr);
    }

    Ok(RcloneResult {
        success,
        output: combined_output,
        exit_code,
    })
}

pub fn copy_to_remote(local_path: &Path, remote: &str, remote_path: &str) -> Result<()> {
    let source = local_path.to_str().ok_or_else(|| {
        OrdneError::Migration("Invalid local path encoding".to_string())
    })?;
    let dest = format!("{}:{}", remote, remote_path);

    let options = RcloneOptions::default();
    let result = execute_rclone_copy(source, &dest, &options)?;

    if !result.success {
        return Err(OrdneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!(
                "rclone copy failed with exit code {}: {}",
                result.exit_code, result.output
            ),
        });
    }

    Ok(())
}

pub fn copy_from_remote(remote: &str, remote_path: &str, local_path: &Path) -> Result<()> {
    let source = format!("{}:{}", remote, remote_path);
    let dest = local_path.to_str().ok_or_else(|| {
        OrdneError::Migration("Invalid local path encoding".to_string())
    })?;

    let options = RcloneOptions::default();
    let result = execute_rclone_copy(&source, dest, &options)?;

    if !result.success {
        return Err(OrdneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!(
                "rclone copy failed with exit code {}: {}",
                result.exit_code, result.output
            ),
        });
    }

    Ok(())
}

pub fn is_rclone_available() -> bool {
    Command::new("rclone")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn list_remotes() -> Result<Vec<String>> {
    let output = Command::new("rclone")
        .arg("listremotes")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| OrdneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!("Failed to list remotes: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OrdneError::ExternalTool {
            tool: "rclone".to_string(),
            message: format!("Failed to list remotes: {}", stderr),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let remotes: Vec<String> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim_end_matches(':').to_string())
        .collect();

    Ok(remotes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rclone_available() {
        let available = is_rclone_available();
        if available {
            println!("rclone is available");
        } else {
            println!("rclone is not available");
        }
    }

    #[test]
    fn test_rclone_options() {
        let options = RcloneOptions::default();
        assert!(options.checksum);
        assert!(options.verbose);
        assert_eq!(options.transfers, Some(4));
    }

    #[test]
    fn test_list_remotes() {
        if !is_rclone_available() {
            println!("Skipping test_list_remotes: rclone not available");
            return;
        }

        let result = list_remotes();
        match result {
            Ok(remotes) => {
                println!("Found {} rclone remotes", remotes.len());
                for remote in remotes {
                    println!("  - {}", remote);
                }
            }
            Err(e) => {
                println!("Failed to list remotes: {}", e);
            }
        }
    }
}
