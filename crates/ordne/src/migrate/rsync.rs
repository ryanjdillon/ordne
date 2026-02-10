use crate::error::{OrdneError, Result};
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct RsyncOptions {
    pub checksum: bool,
    pub partial: bool,
    pub sparse: bool,
    pub archive: bool,
    pub verbose: bool,
    pub progress: bool,
    pub delete_after: bool,
}

impl Default for RsyncOptions {
    fn default() -> Self {
        Self {
            checksum: true,
            partial: true,
            sparse: true,
            archive: true,
            verbose: true,
            progress: true,
            delete_after: false,
        }
    }
}

pub struct RsyncResult {
    pub success: bool,
    pub output: String,
    pub exit_code: i32,
}

pub fn execute_rsync<S: AsRef<Path>, D: AsRef<Path>>(
    source: S,
    dest: D,
    options: &RsyncOptions,
) -> Result<RsyncResult> {
    let source = source.as_ref();
    let dest = dest.as_ref();

    if !source.exists() {
        return Err(OrdneError::FileNotFound(source.to_path_buf()));
    }

    let mut cmd = Command::new("rsync");

    if options.archive {
        cmd.arg("--archive");
    }

    if options.checksum {
        cmd.arg("--checksum");
    }

    if options.partial {
        cmd.arg("--partial");
    }

    if options.sparse {
        cmd.arg("--sparse");
    }

    if options.verbose {
        cmd.arg("--verbose");
    }

    if options.progress {
        cmd.arg("--progress");
    }

    if options.delete_after {
        cmd.arg("--delete-after");
    }

    cmd.arg(source.to_str().ok_or_else(|| {
        OrdneError::Migration("Invalid source path encoding".to_string())
    })?);
    cmd.arg(dest.to_str().ok_or_else(|| {
        OrdneError::Migration("Invalid destination path encoding".to_string())
    })?);

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    log::info!("Executing rsync: {:?}", cmd);

    let output = cmd.output().map_err(|e| OrdneError::ExternalTool {
        tool: "rsync".to_string(),
        message: format!("Failed to execute rsync: {}", e),
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined_output = format!("{}\n{}", stdout, stderr);

    let exit_code = output.status.code().unwrap_or(-1);
    let success = output.status.success();

    if !success {
        log::error!("rsync failed with exit code {}: {}", exit_code, stderr);
    }

    Ok(RsyncResult {
        success,
        output: combined_output,
        exit_code,
    })
}

pub fn copy_file<S: AsRef<Path>, D: AsRef<Path>>(source: S, dest: D) -> Result<()> {
    let options = RsyncOptions::default();
    let result = execute_rsync(source.as_ref(), dest.as_ref(), &options)?;

    if !result.success {
        return Err(OrdneError::ExternalTool {
            tool: "rsync".to_string(),
            message: format!(
                "rsync failed with exit code {}: {}",
                result.exit_code, result.output
            ),
        });
    }

    Ok(())
}

pub fn is_rsync_available() -> bool {
    Command::new("rsync")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_rsync_available() {
        let available = is_rsync_available();
        if available {
            println!("rsync is available");
        } else {
            println!("rsync is not available, skipping rsync tests");
        }
    }

    #[test]
    fn test_copy_file() {
        if !is_rsync_available() {
            println!("Skipping test_copy_file: rsync not available");
            return;
        }

        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("source.txt");
        let dest_path = temp_dir.path().join("dest.txt");

        fs::write(&source_path, b"test content").unwrap();

        let result = copy_file(&source_path, &dest_path);
        assert!(result.is_ok());
        assert!(dest_path.exists());

        let content = fs::read_to_string(&dest_path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_rsync_options() {
        let mut options = RsyncOptions::default();
        assert!(options.checksum);
        assert!(options.archive);
        assert!(options.partial);

        options.delete_after = true;
        assert!(options.delete_after);
    }

    #[test]
    fn test_copy_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let source_path = temp_dir.path().join("nonexistent.txt");
        let dest_path = temp_dir.path().join("dest.txt");

        let result = copy_file(&source_path, &dest_path);
        assert!(result.is_err());
    }
}
