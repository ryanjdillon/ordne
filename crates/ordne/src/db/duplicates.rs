use crate::db::DuplicateGroup;
use crate::error::Result;
use rusqlite::{Connection, OptionalExtension};

pub fn create_duplicate_group(
    conn: &Connection,
    hash: &str,
    file_count: i32,
    total_waste_bytes: i64,
    original_id: Option<i64>,
    drives_involved: &[i64],
    cross_drive: bool,
) -> Result<i64> {
    let drives_json = serde_json::to_string(drives_involved)?;

    conn.execute(
        "INSERT INTO duplicate_groups (hash, file_count, total_waste_bytes, original_id, drives_involved, cross_drive)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (hash, file_count, total_waste_bytes, original_id, drives_json, cross_drive),
    )?;

    Ok(conn.last_insert_rowid())
}

pub fn get_duplicate_group(conn: &Connection, group_id: i64) -> Result<Option<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT group_id, hash, file_count, total_waste_bytes, original_id, drives_involved, cross_drive, resolution
         FROM duplicate_groups WHERE group_id = ?1",
    )?;

    let group = stmt
        .query_row([group_id], |row| parse_duplicate_group_row(row))
        .optional()?;

    Ok(group)
}

pub fn list_duplicate_groups(conn: &Connection) -> Result<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT group_id, hash, file_count, total_waste_bytes, original_id, drives_involved, cross_drive, resolution
         FROM duplicate_groups ORDER BY total_waste_bytes DESC",
    )?;

    let groups = stmt
        .query_map([], |row| parse_duplicate_group_row(row))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(groups)
}

pub fn list_cross_drive_duplicates(conn: &Connection) -> Result<Vec<DuplicateGroup>> {
    let mut stmt = conn.prepare(
        "SELECT group_id, hash, file_count, total_waste_bytes, original_id, drives_involved, cross_drive, resolution
         FROM duplicate_groups WHERE cross_drive = 1 ORDER BY total_waste_bytes DESC",
    )?;

    let groups = stmt
        .query_map([], |row| parse_duplicate_group_row(row))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(groups)
}

pub fn update_duplicate_group_resolution(
    conn: &Connection,
    group_id: i64,
    resolution: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE duplicate_groups SET resolution = ?1 WHERE group_id = ?2",
        (resolution, group_id),
    )?;
    Ok(())
}

pub fn assign_files_to_duplicate_group(
    conn: &Connection,
    file_ids: &[i64],
    group_id: i64,
    original_id: Option<i64>,
) -> Result<()> {
    for &file_id in file_ids {
        let is_original = Some(file_id) == original_id;
        conn.execute(
            "UPDATE files SET duplicate_group = ?1, is_original = ?2 WHERE id = ?3",
            (group_id, is_original, file_id),
        )?;
    }
    Ok(())
}

pub fn get_duplicate_statistics(conn: &Connection) -> Result<DuplicateStatistics> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*), COALESCE(SUM(file_count), 0), COALESCE(SUM(total_waste_bytes), 0)
         FROM duplicate_groups",
    )?;

    let (group_count, total_duplicates, total_waste): (i64, i64, i64) =
        stmt.query_row([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;

    let mut stmt = conn.prepare(
        "SELECT COUNT(*) FROM duplicate_groups WHERE cross_drive = 1",
    )?;
    let cross_drive_groups: i64 = stmt.query_row([], |row| row.get(0))?;

    Ok(DuplicateStatistics {
        group_count: group_count as usize,
        total_duplicate_files: total_duplicates as usize,
        total_waste_bytes: total_waste,
        cross_drive_groups: cross_drive_groups as usize,
    })
}

#[derive(Debug, Clone)]
pub struct DuplicateStatistics {
    pub group_count: usize,
    pub total_duplicate_files: usize,
    pub total_waste_bytes: i64,
    pub cross_drive_groups: usize,
}

fn parse_duplicate_group_row(row: &rusqlite::Row) -> rusqlite::Result<DuplicateGroup> {
    let drives_json: String = row.get(5)?;
    let drives_involved: Vec<i64> = serde_json::from_str(&drives_json).unwrap_or_default();

    Ok(DuplicateGroup {
        group_id: row.get(0)?,
        hash: row.get(1)?,
        file_count: row.get(2)?,
        total_waste_bytes: row.get(3)?,
        original_id: row.get(4)?,
        drives_involved,
        cross_drive: row.get(6)?,
        resolution: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_schema;

    fn create_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_create_and_get_duplicate_group() {
        let conn = create_test_db();

        // Create required drives
        conn.execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'source', 1, 'local')",
            rusqlite::params![1, "drive1"],
        ).unwrap();
        conn.execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'source', 1, 'local')",
            rusqlite::params![2, "drive2"],
        ).unwrap();

        // Create file with id=1 for original_id reference
        conn.execute(
            "INSERT INTO files (id, drive_id, path, abs_path, filename, size_bytes, status, indexed_at)
             VALUES (1, 1, '/test.txt', '/mnt/drive1/test.txt', 'test.txt', 1024, 'indexed', datetime('now'))",
            [],
        ).unwrap();

        let drives = vec![1, 2];
        let group_id = create_duplicate_group(
            &conn,
            "abc123",
            3,
            2048,
            Some(1),
            &drives,
            true,
        )
        .unwrap();

        assert!(group_id > 0);

        let group = get_duplicate_group(&conn, group_id).unwrap();
        assert!(group.is_some());

        let group = group.unwrap();
        assert_eq!(group.hash, "abc123");
        assert_eq!(group.file_count, 3);
        assert_eq!(group.total_waste_bytes, 2048);
        assert_eq!(group.drives_involved, vec![1, 2]);
        assert!(group.cross_drive);
    }

    #[test]
    fn test_list_cross_drive_duplicates() {
        let conn = create_test_db();

        create_duplicate_group(&conn, "hash1", 2, 1024, None, &[1], false).unwrap();
        create_duplicate_group(&conn, "hash2", 3, 2048, None, &[1, 2], true).unwrap();
        create_duplicate_group(&conn, "hash3", 2, 512, None, &[2, 3], true).unwrap();

        let cross_drive = list_cross_drive_duplicates(&conn).unwrap();
        assert_eq!(cross_drive.len(), 2);
    }

    #[test]
    fn test_duplicate_statistics() {
        let conn = create_test_db();

        create_duplicate_group(&conn, "hash1", 2, 1024, None, &[1], false).unwrap();
        create_duplicate_group(&conn, "hash2", 3, 2048, None, &[1, 2], true).unwrap();

        let stats = get_duplicate_statistics(&conn).unwrap();
        assert_eq!(stats.group_count, 2);
        assert_eq!(stats.total_duplicate_files, 5);
        assert_eq!(stats.total_waste_bytes, 3072);
        assert_eq!(stats.cross_drive_groups, 1);
    }

    #[test]
    fn test_update_resolution() {
        let conn = create_test_db();

        let group_id = create_duplicate_group(&conn, "hash1", 2, 1024, None, &[1], false).unwrap();

        update_duplicate_group_resolution(&conn, group_id, "kept_original").unwrap();

        let group = get_duplicate_group(&conn, group_id).unwrap().unwrap();
        assert_eq!(group.resolution, Some("kept_original".to_string()));
    }
}
