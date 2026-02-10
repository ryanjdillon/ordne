use crate::error::{PruneError, Result};
use md5::Digest;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub fn compute_blake3_hash<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|e| {
        log::error!("Failed to open file for hashing: {}: {}", path.display(), e);
        e
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hash.to_hex().to_string())
}

pub fn compute_md5_hash<P: AsRef<Path>>(path: P) -> Result<String> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|e| {
        log::error!("Failed to open file for hashing: {}: {}", path.display(), e);
        e
    })?;

    let mut reader = BufReader::new(file);
    let mut hasher = md5::Md5::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(format!("{:x}", hash))
}

pub fn verify_hash<P: AsRef<Path>>(path: P, expected_hash: &str) -> Result<bool> {
    let path = path.as_ref();

    if !path.exists() {
        return Ok(false);
    }

    let computed = if expected_hash.len() == 64 {
        compute_blake3_hash(path)?
    } else if expected_hash.len() == 32 {
        compute_md5_hash(path)?
    } else {
        return Err(PruneError::Migration(format!(
            "Invalid hash length: {}",
            expected_hash.len()
        )));
    };

    Ok(computed.eq_ignore_ascii_case(expected_hash))
}

pub fn verify_source_unchanged<P: AsRef<Path>>(path: P, expected_hash: &str) -> Result<()> {
    let path = path.as_ref();

    if !verify_hash(path, expected_hash)? {
        let actual_hash = if expected_hash.len() == 64 {
            compute_blake3_hash(path)?
        } else {
            compute_md5_hash(path)?
        };

        return Err(PruneError::SourceChanged {
            expected: expected_hash.to_string(),
            actual: actual_hash,
        });
    }

    Ok(())
}

pub fn verify_destination<P: AsRef<Path>>(path: P, expected_hash: &str) -> Result<()> {
    let path = path.as_ref();

    if !verify_hash(path, expected_hash)? {
        return Err(PruneError::DestinationVerification {
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_compute_blake3_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let hash = compute_blake3_hash(&file_path).unwrap();
        assert_eq!(hash.len(), 64);

        let hash2 = compute_blake3_hash(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_compute_md5_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let hash = compute_md5_hash(&file_path).unwrap();
        assert_eq!(hash.len(), 32);

        let hash2 = compute_md5_hash(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_verify_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let hash = compute_blake3_hash(&file_path).unwrap();
        assert!(verify_hash(&file_path, &hash).unwrap());

        // Invalid hash length should return an error
        assert!(verify_hash(&file_path, "invalid_hash").is_err());

        // Wrong but properly-sized hash should return Ok(false)
        let wrong_hash = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(!verify_hash(&file_path, wrong_hash).unwrap());
    }

    #[test]
    fn test_verify_source_unchanged() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let hash = compute_blake3_hash(&file_path).unwrap();
        let result = verify_source_unchanged(&file_path, &hash);
        assert!(result.is_ok());

        fs::write(&file_path, b"modified content").unwrap();
        let result = verify_source_unchanged(&file_path, &hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_destination() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let hash = compute_blake3_hash(&file_path).unwrap();
        let result = verify_destination(&file_path, &hash);
        assert!(result.is_ok());

        fs::write(&file_path, b"modified content").unwrap();
        let result = verify_destination(&file_path, &hash);
        assert!(result.is_err());
    }
}
