use crate::db::{Database, File, FileStatus, Priority};
use crate::error::{PruneError, Result};
use chrono::Utc;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path};
use walkdir::{DirEntry, WalkDir};

#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub dirs_scanned: usize,
    pub bytes_scanned: u64,
    pub symlinks_found: usize,
    pub git_repos_found: usize,
    pub errors: usize,
}

/// Options for filesystem scanning
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub max_depth: Option<usize>,
    pub include_hidden: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            follow_symlinks: false,
            max_depth: None,
            include_hidden: false,
        }
    }
}

/// Scans a directory and inserts file records into the database
///
/// This performs a recursive directory walk, collecting metadata for each file:
/// - Size, mtime, inode, permissions
/// - Symlink detection and target resolution
/// - Hardlink detection (nlinks > 1)
/// - Git remote URL extraction from `.git/config` files
///
/// Returns statistics about the scan operation.
pub fn scan_directory<P: AsRef<Path>>(
    db: &mut dyn Database,
    drive_id: i64,
    path: P,
    options: ScanOptions,
) -> Result<ScanStats> {
    let path = path.as_ref();
    let mut stats = ScanStats::default();

    if !path.exists() {
        return Err(PruneError::FileNotFound(path.to_path_buf()));
    }

    let mut walker = WalkDir::new(path).follow_links(options.follow_symlinks);

    if let Some(max_depth) = options.max_depth {
        walker = walker.max_depth(max_depth);
    }

    for entry in walker.into_iter() {
        match entry {
            Ok(entry) => {
                if let Err(e) = process_entry(db, drive_id, &entry, &mut stats, &options, path) {
                    eprintln!("Error processing {}: {}", entry.path().display(), e);
                    stats.errors += 1;
                }
            }
            Err(e) => {
                eprintln!("Walk error: {}", e);
                stats.errors += 1;
            }
        }
    }

    Ok(stats)
}

fn process_entry(
    db: &mut dyn Database,
    drive_id: i64,
    entry: &DirEntry,
    stats: &mut ScanStats,
    options: &ScanOptions,
    base_path: &Path,
) -> Result<()> {
    let path = entry.path();

    if !options.include_hidden {
        if let Some(name) = path.file_name() {
            if name.to_string_lossy().starts_with('.') && path != base_path {
                return Ok(());
            }
        }
    }

    if entry.file_type().is_dir() {
        stats.dirs_scanned += 1;
        return Ok(());
    }

    if !entry.file_type().is_file() && !entry.file_type().is_symlink() {
        return Ok(());
    }

    let metadata = match entry.metadata() {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to get metadata for {}: {}", path.display(), e);
            return Ok(());
        }
    };

    let is_symlink = metadata.is_symlink() || entry.path_is_symlink();
    let symlink_target = if is_symlink {
        stats.symlinks_found += 1;
        fs::read_link(path).ok().map(|p| p.to_string_lossy().to_string())
    } else {
        None
    };

    let relative_path = path
        .strip_prefix(base_path)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_string());

    let git_remote_url = if filename == "config" && path.parent().and_then(|p| p.file_name()).map(|n| n == ".git").unwrap_or(false) {
        extract_git_remote(path).ok()
    } else {
        None
    };

    if git_remote_url.is_some() {
        stats.git_repos_found += 1;
    }

    let size_bytes = if is_symlink { 0 } else { metadata.len() as i64 };

    let file = File {
        id: 0,
        drive_id,
        path: relative_path,
        abs_path: path.to_string_lossy().to_string(),
        filename,
        extension,
        size_bytes,
        md5_hash: None,
        blake3_hash: None,
        created_at: None,
        modified_at: metadata.modified().ok().map(|t| {
            let duration = t.duration_since(std::time::UNIX_EPOCH).unwrap();
            chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)
                .unwrap()
                .with_timezone(&Utc)
        }),
        inode: Some(metadata.ino() as i64),
        device_num: Some(metadata.dev() as i64),
        nlinks: Some(metadata.nlink() as i32),
        mime_type: None,
        is_symlink,
        symlink_target,
        git_remote_url,
        category: None,
        subcategory: None,
        target_path: None,
        target_drive_id: None,
        priority: Priority::Normal,
        duplicate_group: None,
        is_original: false,
        rmlint_type: None,
        status: FileStatus::Indexed,
        migrated_to: None,
        migrated_to_drive: None,
        migrated_at: None,
        verified_hash: None,
        error: None,
        indexed_at: Utc::now(),
    };

    db.add_file(&file)?;
    stats.files_scanned += 1;
    stats.bytes_scanned += size_bytes as u64;

    Ok(())
}

/// Extracts Git remote URL from a .git/config file
fn extract_git_remote<P: AsRef<Path>>(config_path: P) -> Result<String> {
    let content = fs::read_to_string(config_path)?;

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("url = ") {
            return Ok(line.trim_start_matches("url = ").to_string());
        }
    }

    Err(PruneError::Config("No remote URL found".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqliteDatabase;
    use std::fs::File as StdFile;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_db() -> SqliteDatabase {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn test_scan_empty_directory() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = create_test_db();

        let drive = crate::db::Drive {
            id: 0,
            label: "test".to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: None,
            fs_type: None,
            total_bytes: None,
            role: crate::db::DriveRole::Source,
            is_online: true,
            is_readonly: false,
            backend: crate::db::Backend::Local,
            rclone_remote: None,
            scanned_at: None,
            added_at: Utc::now(),
        };
        let drive_id = db.add_drive(&drive).unwrap();

        let stats = scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

        assert_eq!(stats.files_scanned, 0);
        assert_eq!(stats.dirs_scanned, 1);
    }

    #[test]
    fn test_scan_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = create_test_db();

        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");
        StdFile::create(&file1).unwrap().write_all(b"content1").unwrap();
        StdFile::create(&file2).unwrap().write_all(b"content2").unwrap();

        let drive = crate::db::Drive {
            id: 0,
            label: "test".to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: None,
            fs_type: None,
            total_bytes: None,
            role: crate::db::DriveRole::Source,
            is_online: true,
            is_readonly: false,
            backend: crate::db::Backend::Local,
            rclone_remote: None,
            scanned_at: None,
            added_at: Utc::now(),
        };
        let drive_id = db.add_drive(&drive).unwrap();

        let stats = scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

        assert_eq!(stats.files_scanned, 2);
        assert!(stats.bytes_scanned > 0);
    }

    #[test]
    fn test_scan_with_subdirectories() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = create_test_db();

        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();
        let file = subdir.join("file.txt");
        StdFile::create(&file).unwrap().write_all(b"content").unwrap();

        let drive = crate::db::Drive {
            id: 0,
            label: "test".to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: None,
            fs_type: None,
            total_bytes: None,
            role: crate::db::DriveRole::Source,
            is_online: true,
            is_readonly: false,
            backend: crate::db::Backend::Local,
            rclone_remote: None,
            scanned_at: None,
            added_at: Utc::now(),
        };
        let drive_id = db.add_drive(&drive).unwrap();

        let stats = scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

        assert_eq!(stats.files_scanned, 1);
        assert_eq!(stats.dirs_scanned, 2);
    }

    #[test]
    fn test_extract_git_remote() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config");
        let mut file = StdFile::create(&config_path).unwrap();
        file.write_all(b"[core]\n\trepositoryformatversion = 0\n[remote \"origin\"]\n\turl = https://github.com/user/repo.git\n\tfetch = +refs/heads/*:refs/remotes/origin/*\n").unwrap();

        let url = extract_git_remote(&config_path).unwrap();
        assert_eq!(url, "https://github.com/user/repo.git");
    }

    #[test]
    fn test_scan_options_max_depth() {
        let temp_dir = TempDir::new().unwrap();
        let mut db = create_test_db();

        let level1 = temp_dir.path().join("level1");
        let level2 = level1.join("level2");
        fs::create_dir_all(&level2).unwrap();
        StdFile::create(level1.join("file1.txt")).unwrap();
        StdFile::create(level2.join("file2.txt")).unwrap();

        let drive = crate::db::Drive {
            id: 0,
            label: "test".to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: None,
            fs_type: None,
            total_bytes: None,
            role: crate::db::DriveRole::Source,
            is_online: true,
            is_readonly: false,
            backend: crate::db::Backend::Local,
            rclone_remote: None,
            scanned_at: None,
            added_at: Utc::now(),
        };
        let drive_id = db.add_drive(&drive).unwrap();

        let options = ScanOptions {
            max_depth: Some(2),
            ..Default::default()
        };
        let stats = scan_directory(&mut db, drive_id, temp_dir.path(), options).unwrap();

        assert_eq!(stats.files_scanned, 1);
    }
}
