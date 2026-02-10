use crate::error::{PruneError, Result};
use std::path::Path;

const SAFETY_HEADROOM_PERCENT: f64 = 0.50;

#[derive(Debug, Clone)]
pub struct SpaceInfo {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

impl SpaceInfo {
    pub fn max_safe_write_bytes(&self) -> u64 {
        let max_use = (self.free_bytes as f64 * SAFETY_HEADROOM_PERCENT) as u64;
        max_use.min(self.available_bytes)
    }

    pub fn can_safely_write(&self, bytes: u64) -> bool {
        bytes <= self.max_safe_write_bytes()
    }
}

pub fn get_free_space<P: AsRef<Path>>(path: P) -> Result<SpaceInfo> {

    let path = path.as_ref();
    if !path.exists() {
        return Err(PruneError::FileNotFound(path.to_path_buf()));
    }

    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        use std::mem;

        let path_cstr = CString::new(path.to_str().ok_or_else(|| {
            PruneError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid path encoding",
            ))
        })?)
        .map_err(|_| {
            PruneError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path contains null byte",
            ))
        })?;

        let mut stat: libc::statvfs = unsafe { mem::zeroed() };
        let result = unsafe { libc::statvfs(path_cstr.as_ptr(), &mut stat) };

        if result != 0 {
            return Err(PruneError::Io(std::io::Error::last_os_error()));
        }

        let block_size = stat.f_frsize as u64;
        let total_bytes = stat.f_blocks * block_size;
        let free_bytes = stat.f_bfree * block_size;
        let available_bytes = stat.f_bavail * block_size;
        let used_bytes = total_bytes.saturating_sub(free_bytes);

        Ok(SpaceInfo {
            total_bytes,
            free_bytes,
            used_bytes,
            available_bytes,
        })
    }

    #[cfg(not(target_os = "linux"))]
    {
        Err(PruneError::Config(
            "Space checking not implemented for this platform".to_string(),
        ))
    }
}

pub fn calculate_batch_size(files: &[(i64, u64)], max_bytes: u64) -> Vec<i64> {
    let mut batch = Vec::new();
    let mut total_bytes = 0u64;

    for (file_id, size) in files {
        if total_bytes + size <= max_bytes {
            batch.push(*file_id);
            total_bytes += size;
        } else {
            break;
        }
    }

    batch
}

pub fn verify_sufficient_space<P: AsRef<Path>>(path: P, required_bytes: u64) -> Result<()> {
    let space_info = get_free_space(path)?;

    if !space_info.can_safely_write(required_bytes) {
        return Err(PruneError::InsufficientSpace {
            available: space_info.max_safe_write_bytes(),
            required: required_bytes,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_get_free_space() {
        let temp_dir = TempDir::new().unwrap();
        let space_info = get_free_space(temp_dir.path()).unwrap();

        assert!(space_info.total_bytes > 0);
        assert!(space_info.free_bytes > 0);
        assert!(space_info.available_bytes > 0);
        assert!(space_info.used_bytes < space_info.total_bytes);
    }

    #[test]
    fn test_max_safe_write_bytes() {
        let space_info = SpaceInfo {
            total_bytes: 1_000_000_000,
            free_bytes: 500_000_000,
            used_bytes: 500_000_000,
            available_bytes: 500_000_000,
        };

        let max_safe = space_info.max_safe_write_bytes();
        assert_eq!(max_safe, 250_000_000);
        assert!(space_info.can_safely_write(200_000_000));
        assert!(!space_info.can_safely_write(300_000_000));
    }

    #[test]
    fn test_calculate_batch_size() {
        let files = vec![
            (1, 100_000),
            (2, 200_000),
            (3, 300_000),
            (4, 400_000),
            (5, 500_000),
        ];

        let batch = calculate_batch_size(&files, 600_000);
        assert_eq!(batch, vec![1, 2, 3]);

        let batch_exact = calculate_batch_size(&files, 600_000);
        assert_eq!(batch_exact, vec![1, 2, 3]);

        let batch_single = calculate_batch_size(&files, 150_000);
        assert_eq!(batch_single, vec![1]);
    }

    #[test]
    fn test_verify_sufficient_space() {
        let temp_dir = TempDir::new().unwrap();
        let space_info = get_free_space(temp_dir.path()).unwrap();
        let safe_amount = space_info.max_safe_write_bytes() / 2;

        let result = verify_sufficient_space(temp_dir.path(), safe_amount);
        assert!(result.is_ok());

        let unsafe_amount = space_info.max_safe_write_bytes() * 2;
        let result = verify_sufficient_space(temp_dir.path(), unsafe_amount);
        assert!(result.is_err());
    }
}
