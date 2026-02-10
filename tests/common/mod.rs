use ordne_lib::{
    Backend, Database, Drive, DriveRole, DuplicateGroup, File, FileStatus, Priority, Result,
    SqliteDatabase,
};
use chrono::Utc;
use rusqlite::Connection;
use std::path::PathBuf;
use tempfile::TempDir;

pub mod helpers;
pub use helpers::*;

pub struct TestFixture {
    pub temp_dir: TempDir,
    pub db_path: PathBuf,
    pub db: SqliteDatabase,
}

impl TestFixture {
    pub fn new() -> Result<Self> {
        let temp_dir = tempfile::tempdir()?;
        let db_path = temp_dir.path().join("test.db");
        let mut db = SqliteDatabase::open(&db_path)?;
        db.initialize()?;

        Ok(Self {
            temp_dir,
            db_path,
            db,
        })
    }

    pub fn db_mut(&mut self) -> &mut SqliteDatabase {
        &mut self.db
    }

    pub fn db(&self) -> &SqliteDatabase {
        &self.db
    }
}

pub fn create_temp_db() -> Result<(TempDir, SqliteDatabase)> {
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test.db");
    let mut db = SqliteDatabase::open(&db_path)?;
    db.initialize()?;
    Ok((temp_dir, db))
}

pub fn setup_test_fixture() -> Result<TestFixture> {
    TestFixture::new()
}

pub fn create_test_drive(conn: &Connection, label: &str, role: DriveRole) -> Result<i64> {
    conn.execute(
        "INSERT INTO drives (label, device_id, device_path, uuid, mount_path, fs_type,
                            total_bytes, role, is_online, is_readonly, backend, rclone_remote)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        (
            label,
            Some(format!("/dev/test_{}", label)),
            Some(format!("/dev/test_{}", label)),
            Some(format!("uuid-{}", label)),
            Some(format!("/mnt/{}", label)),
            Some("ext4"),
            Some(1_000_000_000_i64),
            role.as_str(),
            true,
            false,
            Backend::Local.as_str(),
            None::<String>,
        ),
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn create_test_file(conn: &Connection, drive_id: i64, path: &str) -> Result<i64> {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string());

    let abs_path = format!("/mnt/test/{}", path);

    conn.execute(
        "INSERT INTO files (
            drive_id, path, abs_path, filename, extension, size_bytes,
            md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
            mime_type, is_symlink, symlink_target, git_remote_url,
            category, subcategory, target_path, target_drive_id,
            priority, duplicate_group, is_original, rmlint_type, status,
            migrated_to, migrated_to_drive, migrated_at, verified_hash, error
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31
        )",
        (
            drive_id,
            path,
            abs_path,
            filename,
            extension,
            1024_i64,
            Some("abc123def456"),
            None::<String>,
            None::<String>,
            Some(Utc::now().to_rfc3339()),
            Some(12345_i64),
            Some(1_i64),
            Some(1_i64),
            Some("text/plain"),
            false,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<i64>,
            Priority::Normal.as_str(),
            None::<i64>,
            false,
            None::<String>,
            FileStatus::Indexed.as_str(),
            None::<String>,
            None::<i64>,
            None::<String>,
            None::<String>,
            None::<String>,
        ),
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn create_test_duplicate_group(
    conn: &Connection,
    hash: &str,
    total_size: i64,
    file_count: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO duplicate_groups (hash, total_waste_bytes, file_count)
         VALUES (?1, ?2, ?3)",
        (hash, total_size, file_count),
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn create_test_file_with_hash(
    conn: &Connection,
    drive_id: i64,
    path: &str,
    hash: &str,
    size: i64,
) -> Result<i64> {
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    let extension = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string());

    let abs_path = format!("/mnt/test/{}", path);

    conn.execute(
        "INSERT INTO files (
            drive_id, path, abs_path, filename, extension, size_bytes,
            md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
            mime_type, is_symlink, symlink_target, git_remote_url,
            category, subcategory, target_path, target_drive_id,
            priority, duplicate_group, is_original, rmlint_type, status,
            migrated_to, migrated_to_drive, migrated_at, verified_hash, error
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
            ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31
        )",
        (
            drive_id,
            path,
            abs_path,
            filename,
            extension,
            size,
            Some(hash),
            None::<String>,
            None::<String>,
            Some(Utc::now().to_rfc3339()),
            Some(12345_i64),
            Some(1_i64),
            Some(1_i64),
            Some("text/plain"),
            false,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<String>,
            None::<i64>,
            Priority::Normal.as_str(),
            None::<i64>,
            false,
            None::<String>,
            FileStatus::Indexed.as_str(),
            None::<String>,
            None::<i64>,
            None::<String>,
            None::<String>,
            None::<String>,
        ),
    )?;

    Ok(conn.last_insert_rowid())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixture_creation() {
        let fixture = TestFixture::new().unwrap();
        assert!(fixture.db_path.exists());
    }

    #[test]
    fn test_temp_db_creation() {
        let (_temp_dir, db) = create_temp_db().unwrap();
        let drives = db.list_drives().unwrap();
        assert_eq!(drives.len(), 0);
    }

    #[test]
    fn test_create_test_drive() {
        let (_temp_dir, db) = create_temp_db().unwrap();
        let drive_id = create_test_drive(db.conn(), "test_drive", DriveRole::Source).unwrap();
        assert!(drive_id > 0);

        let drives = db.list_drives().unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(drives[0].label, "test_drive");
    }

    #[test]
    fn test_create_test_file() {
        let (_temp_dir, db) = create_temp_db().unwrap();
        let drive_id = create_test_drive(db.conn(), "test_drive", DriveRole::Source).unwrap();
        let file_id = create_test_file(db.conn(), drive_id, "test/file.txt").unwrap();
        assert!(file_id > 0);
    }

    #[test]
    fn test_create_test_duplicate_group() {
        let (_temp_dir, db) = create_temp_db().unwrap();
        let group_id = create_test_duplicate_group(db.conn(), "abc123", 2048, 2).unwrap();
        assert!(group_id > 0);
    }
}
