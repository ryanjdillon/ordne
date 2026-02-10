use crate::db::{File, FileStatus, Priority};
use crate::error::{OrdneError, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, named_params};

pub fn add_file(conn: &Connection, file: &File) -> Result<i64> {
    conn.execute(
        "INSERT OR REPLACE INTO files (
            drive_id, path, abs_path, filename, extension, size_bytes,
            md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
            mime_type, is_symlink, symlink_target, git_remote_url,
            category, subcategory, target_path, target_drive_id,
            priority, duplicate_group, is_original, rmlint_type, status,
            migrated_to, migrated_to_drive, migrated_at, verified_hash, error
        ) VALUES (
            :drive_id, :path, :abs_path, :filename, :extension, :size_bytes,
            :md5_hash, :blake3_hash, :created_at, :modified_at, :inode, :device_num, :nlinks,
            :mime_type, :is_symlink, :symlink_target, :git_remote_url,
            :category, :subcategory, :target_path, :target_drive_id,
            :priority, :duplicate_group, :is_original, :rmlint_type, :status,
            :migrated_to, :migrated_to_drive, :migrated_at, :verified_hash, :error
        )",
        named_params! {
            ":drive_id": file.drive_id,
            ":path": &file.path,
            ":abs_path": &file.abs_path,
            ":filename": &file.filename,
            ":extension": &file.extension,
            ":size_bytes": file.size_bytes,
            ":md5_hash": &file.md5_hash,
            ":blake3_hash": &file.blake3_hash,
            ":created_at": file.created_at.as_ref().map(|dt| dt.to_rfc3339()),
            ":modified_at": file.modified_at.as_ref().map(|dt| dt.to_rfc3339()),
            ":inode": file.inode,
            ":device_num": file.device_num,
            ":nlinks": file.nlinks,
            ":mime_type": &file.mime_type,
            ":is_symlink": file.is_symlink,
            ":symlink_target": &file.symlink_target,
            ":git_remote_url": &file.git_remote_url,
            ":category": &file.category,
            ":subcategory": &file.subcategory,
            ":target_path": &file.target_path,
            ":target_drive_id": file.target_drive_id,
            ":priority": file.priority.as_str(),
            ":duplicate_group": file.duplicate_group,
            ":is_original": file.is_original,
            ":rmlint_type": &file.rmlint_type,
            ":status": file.status.as_str(),
            ":migrated_to": &file.migrated_to,
            ":migrated_to_drive": file.migrated_to_drive,
            ":migrated_at": file.migrated_at.as_ref().map(|dt| dt.to_rfc3339()),
            ":verified_hash": &file.verified_hash,
            ":error": &file.error,
        },
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn get_file(conn: &Connection, id: i64) -> Result<Option<File>> {
    let mut stmt = conn.prepare(
        "SELECT id, drive_id, path, abs_path, filename, extension, size_bytes,
                md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
                mime_type, is_symlink, symlink_target, git_remote_url,
                category, subcategory, target_path, target_drive_id,
                priority, duplicate_group, is_original, rmlint_type, status,
                migrated_to, migrated_to_drive, migrated_at, verified_hash, error, indexed_at
         FROM files WHERE id = ?1",
    )?;

    stmt.query_row([id], |row| {
        Ok(File {
            id: row.get(0)?,
            drive_id: row.get(1)?,
            path: row.get(2)?,
            abs_path: row.get(3)?,
            filename: row.get(4)?,
            extension: row.get(5)?,
            size_bytes: row.get(6)?,
            md5_hash: row.get(7)?,
            blake3_hash: row.get(8)?,
            created_at: row
                .get::<_, Option<String>>(9)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            modified_at: row
                .get::<_, Option<String>>(10)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            inode: row.get(11)?,
            device_num: row.get(12)?,
            nlinks: row.get(13)?,
            mime_type: row.get(14)?,
            is_symlink: row.get(15)?,
            symlink_target: row.get(16)?,
            git_remote_url: row.get(17)?,
            category: row.get(18)?,
            subcategory: row.get(19)?,
            target_path: row.get(20)?,
            target_drive_id: row.get(21)?,
            priority: Priority::from_str(&row.get::<_, String>(22)?).unwrap_or(Priority::Normal),
            duplicate_group: row.get(23)?,
            is_original: row.get(24)?,
            rmlint_type: row.get(25)?,
            status: FileStatus::from_str(&row.get::<_, String>(26)?).unwrap_or(FileStatus::Indexed),
            migrated_to: row.get(27)?,
            migrated_to_drive: row.get(28)?,
            migrated_at: row
                .get::<_, Option<String>>(29)?
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            verified_hash: row.get(30)?,
            error: row.get(31)?,
            indexed_at: row
                .get::<_, String>(32)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now),
        })
    })
    .optional()
    .map_err(Into::into)
}

pub fn update_file_status(conn: &Connection, id: i64, status: FileStatus) -> Result<()> {
    conn.execute(
        "UPDATE files SET status = ?1 WHERE id = ?2",
        (status.as_str(), id),
    )?;
    Ok(())
}

pub fn list_files_by_hash(conn: &Connection, hash: &str) -> Result<Vec<File>> {
    let mut stmt = conn.prepare(
        "SELECT id, drive_id, path, abs_path, filename, extension, size_bytes,
                md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
                mime_type, is_symlink, symlink_target, git_remote_url,
                category, subcategory, target_path, target_drive_id,
                priority, duplicate_group, is_original, rmlint_type, status,
                migrated_to, migrated_to_drive, migrated_at, verified_hash, error, indexed_at
         FROM files WHERE md5_hash = ?1 OR blake3_hash = ?1",
    )?;

    let files = stmt
        .query_map([hash], |row| {
            Ok(File {
                id: row.get(0)?,
                drive_id: row.get(1)?,
                path: row.get(2)?,
                abs_path: row.get(3)?,
                filename: row.get(4)?,
                extension: row.get(5)?,
                size_bytes: row.get(6)?,
                md5_hash: row.get(7)?,
                blake3_hash: row.get(8)?,
                created_at: row
                    .get::<_, Option<String>>(9)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                modified_at: row
                    .get::<_, Option<String>>(10)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                inode: row.get(11)?,
                device_num: row.get(12)?,
                nlinks: row.get(13)?,
                mime_type: row.get(14)?,
                is_symlink: row.get(15)?,
                symlink_target: row.get(16)?,
                git_remote_url: row.get(17)?,
                category: row.get(18)?,
                subcategory: row.get(19)?,
                target_path: row.get(20)?,
                target_drive_id: row.get(21)?,
                priority: Priority::from_str(&row.get::<_, String>(22)?).unwrap_or(Priority::Normal),
                duplicate_group: row.get(23)?,
                is_original: row.get(24)?,
                rmlint_type: row.get(25)?,
                status: FileStatus::from_str(&row.get::<_, String>(26)?).unwrap_or(FileStatus::Indexed),
                migrated_to: row.get(27)?,
                migrated_to_drive: row.get(28)?,
                migrated_at: row
                    .get::<_, Option<String>>(29)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                verified_hash: row.get(30)?,
                error: row.get(31)?,
                indexed_at: row
                    .get::<_, String>(32)
                    .ok()
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(files)
}

pub fn update_file_classification(
    conn: &Connection,
    id: i64,
    category: &str,
    subcategory: Option<&str>,
    priority: Priority,
) -> Result<()> {
    conn.execute(
        "UPDATE files SET category = ?1, subcategory = ?2, priority = ?3, status = ?4 WHERE id = ?5",
        (category, subcategory, priority.as_str(), FileStatus::Classified.as_str(), id),
    )?;
    Ok(())
}

pub fn get_files_by_category(conn: &Connection, category: &str) -> Result<Vec<File>> {
    let mut stmt = conn.prepare(
        "SELECT id, drive_id, path, abs_path, filename, extension, size_bytes,
                md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
                mime_type, is_symlink, symlink_target, git_remote_url,
                category, subcategory, target_path, target_drive_id,
                priority, duplicate_group, is_original, rmlint_type, status,
                migrated_to, migrated_to_drive, migrated_at, verified_hash, error, indexed_at
         FROM files WHERE category = ?1",
    )?;

    let files = stmt
        .query_map([category], |row| {
            Ok(File {
                id: row.get(0)?,
                drive_id: row.get(1)?,
                path: row.get(2)?,
                abs_path: row.get(3)?,
                filename: row.get(4)?,
                extension: row.get(5)?,
                size_bytes: row.get(6)?,
                md5_hash: row.get(7)?,
                blake3_hash: row.get(8)?,
                created_at: row
                    .get::<_, Option<String>>(9)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                modified_at: row
                    .get::<_, Option<String>>(10)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                inode: row.get(11)?,
                device_num: row.get(12)?,
                nlinks: row.get(13)?,
                mime_type: row.get(14)?,
                is_symlink: row.get(15)?,
                symlink_target: row.get(16)?,
                git_remote_url: row.get(17)?,
                category: row.get(18)?,
                subcategory: row.get(19)?,
                target_path: row.get(20)?,
                target_drive_id: row.get(21)?,
                priority: Priority::from_str(&row.get::<_, String>(22)?).unwrap_or(Priority::Normal),
                duplicate_group: row.get(23)?,
                is_original: row.get(24)?,
                rmlint_type: row.get(25)?,
                status: FileStatus::from_str(&row.get::<_, String>(26)?).unwrap_or(FileStatus::Indexed),
                migrated_to: row.get(27)?,
                migrated_to_drive: row.get(28)?,
                migrated_at: row
                    .get::<_, Option<String>>(29)?
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                verified_hash: row.get(30)?,
                error: row.get(31)?,
                indexed_at: row
                    .get::<_, String>(32)
                    .ok()
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(files)
}

#[derive(Debug)]
pub struct CategoryStats {
    pub category: String,
    pub file_count: i64,
    pub total_bytes: i64,
}

pub fn get_category_stats(conn: &Connection) -> Result<Vec<CategoryStats>> {
    let mut stmt = conn.prepare(
        "SELECT category, COUNT(*) as file_count, SUM(size_bytes) as total_bytes
         FROM files
         WHERE category IS NOT NULL
         GROUP BY category
         ORDER BY total_bytes DESC",
    )?;

    let stats = stmt
        .query_map([], |row| {
            Ok(CategoryStats {
                category: row.get(0)?,
                file_count: row.get(1)?,
                total_bytes: row.get(2)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(stats)
}

pub fn bulk_update_classification(
    conn: &Connection,
    file_ids: &[i64],
    category: &str,
    subcategory: Option<&str>,
    priority: Priority,
) -> Result<()> {
    let tx = conn.unchecked_transaction()?;

    for &id in file_ids {
        tx.execute(
            "UPDATE files SET category = ?1, subcategory = ?2, priority = ?3, status = ?4 WHERE id = ?5",
            (category, subcategory, priority.as_str(), FileStatus::Classified.as_str(), id),
        )?;
    }

    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_schema;
    use crate::db::{Backend, DriveRole};

    fn create_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn
    }

    fn create_test_drive(conn: &Connection, id: i64, label: &str) {
        conn.execute(
            "INSERT INTO drives (id, label, device_id, device_path, uuid, mount_path, fs_type,
                                total_bytes, role, is_online, is_readonly, backend, rclone_remote)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            (
                id,
                label,
                Some(format!("/dev/test_{}", label)),
                Some(format!("/dev/test_{}", label)),
                Some(format!("uuid-{}", label)),
                Some(format!("/mnt/{}", label)),
                Some("ext4"),
                Some(1_000_000_000_i64),
                DriveRole::Source.as_str(),
                true,
                false,
                Backend::Local.as_str(),
                None::<String>,
            ),
        ).unwrap();
    }

    #[test]
    fn test_add_and_get_file() {
        let conn = create_test_db();
        create_test_drive(&conn, 1, "test_drive");

        let file = File {
            id: 0,
            drive_id: 1,
            path: "test/file.txt".to_string(),
            abs_path: "/mnt/test/file.txt".to_string(),
            filename: "file.txt".to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 1024,
            md5_hash: Some("abc123".to_string()),
            blake3_hash: None,
            created_at: None,
            modified_at: Some(Utc::now()),
            inode: Some(12345),
            device_num: Some(1),
            nlinks: Some(1),
            mime_type: Some("text/plain".to_string()),
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let id = add_file(&conn, &file).unwrap();
        assert!(id > 0);

        let retrieved = get_file(&conn, id).unwrap();
        assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.filename, "file.txt");
        assert_eq!(retrieved.size_bytes, 1024);
        assert_eq!(retrieved.md5_hash, Some("abc123".to_string()));
    }

    #[test]
    fn test_update_file_status() {
        let conn = create_test_db();
        create_test_drive(&conn, 1, "test_drive");

        let file = File {
            id: 0,
            drive_id: 1,
            path: "test.txt".to_string(),
            abs_path: "/test.txt".to_string(),
            filename: "test.txt".to_string(),
            extension: None,
            size_bytes: 100,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let id = add_file(&conn, &file).unwrap();
        update_file_status(&conn, id, FileStatus::Classified).unwrap();

        let retrieved = get_file(&conn, id).unwrap().unwrap();
        assert_eq!(retrieved.status, FileStatus::Classified);
    }

    #[test]
    fn test_list_files_by_hash() {
        let conn = create_test_db();
        create_test_drive(&conn, 1, "drive1");
        create_test_drive(&conn, 2, "drive2");

        let file1 = File {
            id: 0,
            drive_id: 1,
            path: "file1.txt".to_string(),
            abs_path: "/file1.txt".to_string(),
            filename: "file1.txt".to_string(),
            extension: None,
            size_bytes: 100,
            md5_hash: Some("samehash".to_string()),
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let file2 = File {
            id: 0,
            drive_id: 2,
            path: "file2.txt".to_string(),
            abs_path: "/file2.txt".to_string(),
            filename: "file2.txt".to_string(),
            extension: None,
            size_bytes: 100,
            md5_hash: Some("samehash".to_string()),
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        add_file(&conn, &file1).unwrap();
        add_file(&conn, &file2).unwrap();

        let files = list_files_by_hash(&conn, "samehash").unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_classification_functions() {
        let conn = create_test_db();
        create_test_drive(&conn, 1, "test_drive");

        let file = File {
            id: 0,
            drive_id: 1,
            path: "photo.jpg".to_string(),
            abs_path: "/photo.jpg".to_string(),
            filename: "photo.jpg".to_string(),
            extension: Some("jpg".to_string()),
            size_bytes: 2048,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let id = add_file(&conn, &file).unwrap();

        update_file_classification(&conn, id, "photos", Some("2024"), Priority::Normal).unwrap();

        let updated = get_file(&conn, id).unwrap().unwrap();
        assert_eq!(updated.category, Some("photos".to_string()));
        assert_eq!(updated.subcategory, Some("2024".to_string()));
        assert_eq!(updated.status, FileStatus::Classified);

        let by_category = get_files_by_category(&conn, "photos").unwrap();
        assert_eq!(by_category.len(), 1);

        let stats = get_category_stats(&conn).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].category, "photos");
        assert_eq!(stats[0].file_count, 1);
    }

    #[test]
    fn test_bulk_classification() {
        let conn = create_test_db();
        create_test_drive(&conn, 1, "test_drive");

        let file1 = File {
            id: 0,
            drive_id: 1,
            path: "photo1.jpg".to_string(),
            abs_path: "/photo1.jpg".to_string(),
            filename: "photo1.jpg".to_string(),
            extension: Some("jpg".to_string()),
            size_bytes: 1024,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let file2 = File {
            id: 0,
            drive_id: 1,
            path: "photo2.jpg".to_string(),
            abs_path: "/photo2.jpg".to_string(),
            filename: "photo2.jpg".to_string(),
            extension: Some("jpg".to_string()),
            size_bytes: 2048,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: None,
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
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

        let id1 = add_file(&conn, &file1).unwrap();
        let id2 = add_file(&conn, &file2).unwrap();

        bulk_update_classification(&conn, &[id1, id2], "photos", Some("2024"), Priority::Critical).unwrap();

        let classified = get_files_by_category(&conn, "photos").unwrap();
        assert_eq!(classified.len(), 2);
        assert_eq!(classified[0].priority, Priority::Critical);
        assert_eq!(classified[0].status, FileStatus::Classified);
    }
}
