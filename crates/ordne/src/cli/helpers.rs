use ordne_lib::{Result, File, FileStatus, Priority, SqliteDatabase};
use chrono::{DateTime, Utc};

pub fn list_files_by_drive(db: &SqliteDatabase, drive_id: i64) -> Result<Vec<File>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, drive_id, path, abs_path, filename, extension, size_bytes,
                md5_hash, blake3_hash, created_at, modified_at, inode, device_num, nlinks,
                mime_type, is_symlink, symlink_target, git_remote_url,
                category, subcategory, target_path, target_drive_id,
                priority, duplicate_group, is_original, rmlint_type, status,
                migrated_to, migrated_to_drive, migrated_at, verified_hash, error, indexed_at
         FROM files WHERE drive_id = ?1 ORDER BY path",
    )?;

    let files = stmt
        .query_map([drive_id], |row| parse_file_row(row))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(files)
}

pub fn update_file_hash(db: &mut SqliteDatabase, id: i64, md5: Option<&str>, blake3: Option<&str>) -> Result<()> {
    let conn = db.conn_mut();
    conn.execute(
        "UPDATE files SET md5_hash = ?1, blake3_hash = ?2 WHERE id = ?3",
        (md5, blake3, id),
    )?;
    Ok(())
}

pub fn get_unclassified_files(db: &SqliteDatabase, limit: Option<usize>) -> Result<Vec<File>> {
    let conn = db.conn();
    ordne_lib::db::files::list_unclassified_files(
        conn,
        None,
        limit.map(|value| value as u32),
    )
}

#[derive(Debug, Clone)]
pub struct DriveStatistics {
    pub file_count: usize,
    pub total_bytes: i64,
    pub duplicate_groups: usize,
    pub duplicate_file_count: usize,
    pub duplicate_waste_bytes: i64,
}

pub fn get_drive_statistics(db: &SqliteDatabase, drive_id: i64) -> Result<DriveStatistics> {
    let conn = db.conn();

    let mut stmt = conn.prepare(
        "SELECT COUNT(*), SUM(size_bytes), COUNT(DISTINCT duplicate_group)
         FROM files WHERE drive_id = ?1 AND status != 'source_removed'",
    )?;

    let (file_count, total_bytes, duplicate_groups): (i64, Option<i64>, i64) =
        stmt.query_row([drive_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;

    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM files WHERE drive_id = ?1 AND duplicate_group IS NOT NULL AND is_original = 0",
    )?;
    let duplicate_file_count: i64 = stmt.query_row([drive_id], |row| row.get(0))?;

    let mut stmt = conn.prepare(
        "SELECT COALESCE(SUM(size_bytes), 0) FROM files WHERE drive_id = ?1 AND duplicate_group IS NOT NULL AND is_original = 0",
    )?;
    let duplicate_waste_bytes: i64 = stmt.query_row([drive_id], |row| row.get(0))?;

    Ok(DriveStatistics {
        file_count: file_count as usize,
        total_bytes: total_bytes.unwrap_or(0),
        duplicate_groups: duplicate_groups as usize,
        duplicate_file_count: duplicate_file_count as usize,
        duplicate_waste_bytes,
    })
}

fn parse_file_row(row: &rusqlite::Row) -> rusqlite::Result<File> {
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
            .get::<_, Option<String>>(32)?
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(chrono::Utc::now),
    })
}
