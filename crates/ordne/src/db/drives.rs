use crate::db::{Backend, Drive, DriveRole};
use crate::error::{OrdneError, Result};
use crate::index::DeviceInfo;
use chrono::{Utc};
use rusqlite::Connection;

/// Creates a new drive from device information
pub fn register_drive(
    conn: &Connection,
    label: &str,
    device_info: &DeviceInfo,
    role: DriveRole,
    backend: Backend,
) -> Result<i64> {
    let drive = Drive {
        id: 0,
        label: label.to_string(),
        device_id: device_info.device_id.clone(),
        device_path: device_info.device_path.clone(),
        uuid: device_info.uuid.clone(),
        mount_path: device_info.mount_path.clone(),
        fs_type: device_info.fs_type.clone(),
        total_bytes: device_info.total_bytes,
        role,
        is_online: true,
        is_readonly: false,
        backend,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };

    conn.execute(
        "INSERT INTO drives (label, device_id, device_path, uuid, mount_path, fs_type,
                            total_bytes, role, is_online, is_readonly, backend, rclone_remote)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        (
            &drive.label,
            &drive.device_id,
            &drive.device_path,
            &drive.uuid,
            &drive.mount_path,
            &drive.fs_type,
            drive.total_bytes,
            drive.role.as_str(),
            drive.is_online,
            drive.is_readonly,
            drive.backend.as_str(),
            &drive.rclone_remote,
        ),
    )?;

    Ok(conn.last_insert_rowid())
}

/// Updates the scanned_at timestamp for a drive
pub fn mark_drive_scanned(conn: &Connection, drive_id: i64) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let rows = conn.execute(
        "UPDATE drives SET scanned_at = ?1 WHERE id = ?2",
        (now, drive_id),
    )?;

    if rows == 0 {
        return Err(OrdneError::DriveNotFound(format!("id {}", drive_id)));
    }

    Ok(())
}

/// Updates drive online status
pub fn update_drive_online_status(conn: &Connection, drive_id: i64, is_online: bool) -> Result<()> {
    let rows = conn.execute(
        "UPDATE drives SET is_online = ?1 WHERE id = ?2",
        (is_online, drive_id),
    )?;

    if rows == 0 {
        return Err(OrdneError::DriveNotFound(format!("id {}", drive_id)));
    }

    Ok(())
}

/// Updates drive metadata from fresh device discovery
pub fn refresh_drive_metadata(conn: &Connection, drive_id: i64, device_info: &DeviceInfo) -> Result<()> {
    let rows = conn.execute(
        "UPDATE drives SET device_id = ?1, device_path = ?2, uuid = ?3, mount_path = ?4,
                          fs_type = ?5, total_bytes = ?6
         WHERE id = ?7",
        (
            &device_info.device_id,
            &device_info.device_path,
            &device_info.uuid,
            &device_info.mount_path,
            &device_info.fs_type,
            device_info.total_bytes,
            drive_id,
        ),
    )?;

    if rows == 0 {
        return Err(OrdneError::DriveNotFound(format!("id {}", drive_id)));
    }

    Ok(())
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
    fn test_register_drive() {
        let conn = create_test_db();

        let device_info = DeviceInfo {
            device_id: Some("test-id".to_string()),
            device_path: Some("/dev/sda1".to_string()),
            uuid: Some("test-uuid".to_string()),
            mount_path: Some("/mnt/test".to_string()),
            fs_type: Some("ext4".to_string()),
            total_bytes: Some(1_000_000_000),
            model: Some("Test Model".to_string()),
            serial: Some("TEST123".to_string()),
        };

        let drive_id = register_drive(&conn, "test_drive", &device_info, DriveRole::Source, Backend::Local).unwrap();
        assert!(drive_id > 0);

        let mut stmt = conn.prepare("SELECT label, uuid, mount_path FROM drives WHERE id = ?1").unwrap();
        let (label, uuid, mount_path): (String, Option<String>, Option<String>) = stmt
            .query_row([drive_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap();

        assert_eq!(label, "test_drive");
        assert_eq!(uuid, Some("test-uuid".to_string()));
        assert_eq!(mount_path, Some("/mnt/test".to_string()));
    }

    #[test]
    fn test_mark_drive_scanned() {
        let conn = create_test_db();

        let device_info = DeviceInfo::new();
        let drive_id = register_drive(&conn, "test", &device_info, DriveRole::Source, Backend::Local).unwrap();

        mark_drive_scanned(&conn, drive_id).unwrap();

        let mut stmt = conn.prepare("SELECT scanned_at FROM drives WHERE id = ?1").unwrap();
        let scanned_at: Option<String> = stmt.query_row([drive_id], |row| row.get(0)).unwrap();
        assert!(scanned_at.is_some());
    }

    #[test]
    fn test_update_online_status() {
        let conn = create_test_db();

        let device_info = DeviceInfo::new();
        let drive_id = register_drive(&conn, "test", &device_info, DriveRole::Source, Backend::Local).unwrap();

        update_drive_online_status(&conn, drive_id, false).unwrap();

        let mut stmt = conn.prepare("SELECT is_online FROM drives WHERE id = ?1").unwrap();
        let is_online: bool = stmt.query_row([drive_id], |row| row.get(0)).unwrap();
        assert!(!is_online);
    }

    #[test]
    fn test_refresh_drive_metadata() {
        let conn = create_test_db();

        let device_info = DeviceInfo::new();
        let drive_id = register_drive(&conn, "test", &device_info, DriveRole::Source, Backend::Local).unwrap();

        let new_device_info = DeviceInfo {
            device_id: Some("new-id".to_string()),
            device_path: Some("/dev/sdb1".to_string()),
            uuid: Some("new-uuid".to_string()),
            mount_path: Some("/mnt/new".to_string()),
            fs_type: Some("xfs".to_string()),
            total_bytes: Some(2_000_000_000),
            model: None,
            serial: None,
        };

        refresh_drive_metadata(&conn, drive_id, &new_device_info).unwrap();

        let mut stmt = conn.prepare("SELECT uuid, fs_type, total_bytes FROM drives WHERE id = ?1").unwrap();
        let (uuid, fs_type, total_bytes): (Option<String>, Option<String>, Option<i64>) = stmt
            .query_row([drive_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap();

        assert_eq!(uuid, Some("new-uuid".to_string()));
        assert_eq!(fs_type, Some("xfs".to_string()));
        assert_eq!(total_bytes, Some(2_000_000_000));
    }
}
