use crate::error::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: i32 = 1;

pub fn initialize_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS drives (
            id              INTEGER PRIMARY KEY,
            label           TEXT NOT NULL UNIQUE,
            device_id       TEXT,
            device_path     TEXT,
            uuid            TEXT,
            mount_path      TEXT,
            fs_type         TEXT,
            total_bytes     INTEGER,
            role            TEXT DEFAULT 'source',
            is_online       BOOLEAN DEFAULT 1,
            is_readonly     BOOLEAN DEFAULT 0,
            backend         TEXT DEFAULT 'local',
            rclone_remote   TEXT,
            scanned_at      TEXT,
            added_at        TEXT DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS files (
            id              INTEGER PRIMARY KEY,
            drive_id        INTEGER NOT NULL REFERENCES drives(id),
            path            TEXT NOT NULL,
            abs_path        TEXT NOT NULL,
            filename        TEXT NOT NULL,
            extension       TEXT,
            size_bytes      INTEGER NOT NULL,
            md5_hash        TEXT,
            blake3_hash     TEXT,
            created_at      TEXT,
            modified_at     TEXT,
            inode           INTEGER,
            device_num      INTEGER,
            nlinks          INTEGER,
            mime_type       TEXT,
            is_symlink      BOOLEAN DEFAULT 0,
            symlink_target  TEXT,
            git_remote_url  TEXT,
            category        TEXT,
            subcategory     TEXT,
            target_path     TEXT,
            target_drive_id INTEGER REFERENCES drives(id),
            priority        TEXT DEFAULT 'normal',
            duplicate_group INTEGER,
            is_original     BOOLEAN DEFAULT 0,
            rmlint_type     TEXT,
            status          TEXT DEFAULT 'indexed',
            migrated_to     TEXT,
            migrated_to_drive INTEGER REFERENCES drives(id),
            migrated_at     TEXT,
            verified_hash   TEXT,
            error           TEXT,
            indexed_at      TEXT DEFAULT (datetime('now')),
            UNIQUE(drive_id, path)
        );

        CREATE TABLE IF NOT EXISTS duplicate_groups (
            group_id        INTEGER PRIMARY KEY,
            hash            TEXT NOT NULL,
            file_count      INTEGER,
            total_waste_bytes INTEGER,
            original_id     INTEGER REFERENCES files(id),
            drives_involved TEXT,
            cross_drive     BOOLEAN DEFAULT 0,
            resolution      TEXT
        );

        CREATE TABLE IF NOT EXISTS migration_plans (
            id              INTEGER PRIMARY KEY,
            created_at      TEXT DEFAULT (datetime('now')),
            description     TEXT,
            source_drive_id INTEGER REFERENCES drives(id),
            target_drive_id INTEGER REFERENCES drives(id),
            status          TEXT DEFAULT 'draft',
            total_files     INTEGER,
            total_bytes     INTEGER,
            completed_files INTEGER DEFAULT 0,
            completed_bytes INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS migration_steps (
            id              INTEGER PRIMARY KEY,
            plan_id         INTEGER REFERENCES migration_plans(id),
            file_id         INTEGER REFERENCES files(id),
            action          TEXT NOT NULL,
            source_path     TEXT NOT NULL,
            source_drive_id INTEGER REFERENCES drives(id),
            dest_path       TEXT,
            dest_drive_id   INTEGER REFERENCES drives(id),
            status          TEXT DEFAULT 'pending',
            pre_hash        TEXT,
            post_hash       TEXT,
            executed_at     TEXT,
            error           TEXT,
            step_order      INTEGER
        );

        CREATE TABLE IF NOT EXISTS audit_log (
            id              INTEGER PRIMARY KEY,
            timestamp       TEXT DEFAULT (datetime('now')),
            action          TEXT NOT NULL,
            file_id         INTEGER,
            plan_id         INTEGER,
            drive_id        INTEGER,
            details         TEXT,
            agent_mode      TEXT
        );
        "#,
    )?;

    create_indexes(conn)?;
    set_schema_version(conn)?;

    Ok(())
}

fn create_indexes(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        CREATE INDEX IF NOT EXISTS idx_files_hash ON files(md5_hash);
        CREATE INDEX IF NOT EXISTS idx_files_blake3 ON files(blake3_hash);
        CREATE INDEX IF NOT EXISTS idx_files_status ON files(status);
        CREATE INDEX IF NOT EXISTS idx_files_category ON files(category);
        CREATE INDEX IF NOT EXISTS idx_files_duplicate_group ON files(duplicate_group);
        CREATE INDEX IF NOT EXISTS idx_files_extension ON files(extension);
        CREATE INDEX IF NOT EXISTS idx_files_size ON files(size_bytes);
        CREATE INDEX IF NOT EXISTS idx_files_drive ON files(drive_id);
        CREATE INDEX IF NOT EXISTS idx_migration_steps_plan ON migration_steps(plan_id, step_order);
        CREATE INDEX IF NOT EXISTS idx_migration_steps_status ON migration_steps(status);
        CREATE INDEX IF NOT EXISTS idx_duplicate_groups_hash ON duplicate_groups(hash);
        CREATE INDEX IF NOT EXISTS idx_audit_log_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log(action);
        "#,
    )?;
    Ok(())
}

fn set_schema_version(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_version (version) VALUES (?1)",
        [SCHEMA_VERSION],
    )?;
    Ok(())
}

pub fn get_schema_version(conn: &Connection) -> Result<Option<i32>> {
    let mut stmt = conn.prepare("SELECT version FROM schema_version LIMIT 1")?;
    let mut rows = stmt.query([])?;

    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_initialization() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();

        let version = get_schema_version(&conn).unwrap();
        assert_eq!(version, Some(SCHEMA_VERSION));

        let table_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(table_count, 7);
    }

    #[test]
    fn test_indexes_created() {
        let conn = Connection::open_in_memory().unwrap();
        initialize_schema(&conn).unwrap();

        let index_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name LIKE 'idx_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert!(index_count >= 12);
    }
}
