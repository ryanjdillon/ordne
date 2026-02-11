use crate::error::{OrdneError, Result};
use md5::{Digest, Md5};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

const BUFFER_SIZE: usize = 8192;

/// Computes MD5 hash of a file
///
/// Uses streaming implementation for memory efficiency with large files.
/// Reads the file in 8KB chunks to minimize memory usage.
pub fn hash_file_md5<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|_e| OrdneError::FileNotFound(path.to_path_buf()))?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = Md5::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Computes blake3 hash of a file
///
/// Uses streaming implementation for memory efficiency with large files.
/// Reads the file in 8KB chunks to minimize memory usage.
pub fn hash_file_blake3<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|_| OrdneError::FileNotFound(path.to_path_buf()))?;
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; BUFFER_SIZE];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Progress callback for hash operations
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send>;

/// Computes MD5 hash with progress reporting
///
/// The progress callback receives (bytes_processed, total_bytes)
pub fn hash_file_md5_with_progress<P: AsRef<Path>>(
    path: P,
    progress: ProgressCallback,
) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|_| OrdneError::FileNotFound(path.to_path_buf()))?;
    let total_size = file.metadata()?.len();
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, file);
    let mut hasher = Md5::new();
    let mut buffer = [0u8; BUFFER_SIZE];
    let mut bytes_processed = 0u64;

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        bytes_processed += bytes_read as u64;
        progress(bytes_processed, total_size);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

/// Verifies that a file matches an expected hash
///
/// Compares the computed hash against the expected value.
/// Returns Ok(()) if hashes match, Err with details if they don't.
pub fn verify_hash<P: AsRef<Path>>(path: P, expected_hash: &str) -> Result<()> {
    let path = path.as_ref();
    let actual_hash = if expected_hash.len() == 32 {
        hash_file_md5(path)?
    } else if expected_hash.len() == 64 {
        hash_file_blake3(path)?
    } else {
        return Err(OrdneError::Config(format!(
            "Invalid hash length: {}",
            expected_hash.len()
        )));
    };

    if actual_hash.eq_ignore_ascii_case(expected_hash) {
        Ok(())
    } else {
        Err(OrdneError::HashMismatch {
            expected: expected_hash.to_string(),
            actual: actual_hash,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hash_file_md5_empty() {
        let temp_file = NamedTempFile::new().unwrap();
        let hash = hash_file_md5(temp_file.path()).unwrap();
        assert_eq!(hash, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_hash_file_md5_known() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let hash = hash_file_md5(temp_file.path()).unwrap();
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }

    #[test]
    fn test_hash_file_blake3_known() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let hash = hash_file_blake3(temp_file.path()).unwrap();
        assert_eq!(
            hash,
            "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24"
        );
    }

    #[test]
    fn test_verify_hash_success() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        verify_hash(temp_file.path(), "5eb63bbbe01eeed093cb22bb8f5acdc3").unwrap();
    }

    #[test]
    fn test_verify_hash_mismatch() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        // Use a 32-character MD5 hash that doesn't match the actual content
        let result = verify_hash(temp_file.path(), "00000000000000000000000000000000");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), OrdneError::HashMismatch { .. }));
    }

    #[test]
    fn test_hash_large_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let large_data = vec![b'a'; 1024 * 1024];
        temp_file.write_all(&large_data).unwrap();
        temp_file.flush().unwrap();

        let hash = hash_file_md5(temp_file.path()).unwrap();
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_hash_with_progress() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"hello world").unwrap();
        temp_file.flush().unwrap();

        let hash = hash_file_md5_with_progress(temp_file.path(), Box::new(|_, _| {
            // Progress callback called during hashing
        }))
        .unwrap();

        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }
}
