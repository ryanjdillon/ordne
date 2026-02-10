pub mod audit;
pub mod drives;
pub mod duplicates;
pub mod files;
pub mod plans;
pub mod schema;

use crate::error::{OrdneError, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use audit::AuditDatabase;
pub use plans::PlansDatabase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    pub id: i64,
    pub label: String,
    pub device_id: Option<String>,
    pub device_path: Option<String>,
    pub uuid: Option<String>,
    pub mount_path: Option<String>,
    pub fs_type: Option<String>,
    pub total_bytes: Option<i64>,
    pub role: DriveRole,
    pub is_online: bool,
    pub is_readonly: bool,
    pub backend: Backend,
    pub rclone_remote: Option<String>,
    pub scanned_at: Option<DateTime<Utc>>,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum DriveRole {
    Source,
    Target,
    Backup,
    Offload,
}

impl DriveRole {
    pub fn as_str(&self) -> &'static str {
        match self {
            DriveRole::Source => "source",
            DriveRole::Target => "target",
            DriveRole::Backup => "backup",
            DriveRole::Offload => "offload",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "source" => Ok(DriveRole::Source),
            "target" => Ok(DriveRole::Target),
            "backup" => Ok(DriveRole::Backup),
            "offload" => Ok(DriveRole::Offload),
            _ => Err(OrdneError::Config(format!("Invalid drive role: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Backend {
    Local,
    Rclone,
}

impl Backend {
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Local => "local",
            Backend::Rclone => "rclone",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "local" => Ok(Backend::Local),
            "rclone" => Ok(Backend::Rclone),
            _ => Err(OrdneError::InvalidBackend(s.to_string())),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub id: i64,
    pub drive_id: i64,
    pub path: String,
    pub abs_path: String,
    pub filename: String,
    pub extension: Option<String>,
    pub size_bytes: i64,
    pub md5_hash: Option<String>,
    pub blake3_hash: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub inode: Option<i64>,
    pub device_num: Option<i64>,
    pub nlinks: Option<i32>,
    pub mime_type: Option<String>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub git_remote_url: Option<String>,
    pub category: Option<String>,
    pub subcategory: Option<String>,
    pub target_path: Option<String>,
    pub target_drive_id: Option<i64>,
    pub priority: Priority,
    pub duplicate_group: Option<i64>,
    pub is_original: bool,
    pub rmlint_type: Option<String>,
    pub status: FileStatus,
    pub migrated_to: Option<String>,
    pub migrated_to_drive: Option<i64>,
    pub migrated_at: Option<DateTime<Utc>>,
    pub verified_hash: Option<String>,
    pub error: Option<String>,
    pub indexed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum FileStatus {
    Indexed,
    Classified,
    Planned,
    Migrating,
    Verified,
    SourceRemoved,
}

impl FileStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileStatus::Indexed => "indexed",
            FileStatus::Classified => "classified",
            FileStatus::Planned => "planned",
            FileStatus::Migrating => "migrating",
            FileStatus::Verified => "verified",
            FileStatus::SourceRemoved => "source_removed",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "indexed" => Ok(FileStatus::Indexed),
            "classified" => Ok(FileStatus::Classified),
            "planned" => Ok(FileStatus::Planned),
            "migrating" => Ok(FileStatus::Migrating),
            "verified" => Ok(FileStatus::Verified),
            "source_removed" => Ok(FileStatus::SourceRemoved),
            _ => Err(OrdneError::Config(format!("Invalid file status: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Priority {
    Critical,
    Normal,
    Low,
    Trash,
}

impl Priority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Priority::Critical => "critical",
            Priority::Normal => "normal",
            Priority::Low => "low",
            Priority::Trash => "trash",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "critical" => Ok(Priority::Critical),
            "normal" => Ok(Priority::Normal),
            "low" => Ok(Priority::Low),
            "trash" => Ok(Priority::Trash),
            _ => Err(OrdneError::Config(format!("Invalid priority: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub group_id: i64,
    pub hash: String,
    pub file_count: i32,
    pub total_waste_bytes: i64,
    pub original_id: Option<i64>,
    pub drives_involved: Vec<i64>,
    pub cross_drive: bool,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub description: Option<String>,
    pub source_drive_id: Option<i64>,
    pub target_drive_id: Option<i64>,
    pub status: PlanStatus,
    pub total_files: i32,
    pub total_bytes: i64,
    pub completed_files: i32,
    pub completed_bytes: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Draft,
    Approved,
    InProgress,
    Completed,
    Aborted,
}

impl PlanStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            PlanStatus::Draft => "draft",
            PlanStatus::Approved => "approved",
            PlanStatus::InProgress => "in_progress",
            PlanStatus::Completed => "completed",
            PlanStatus::Aborted => "aborted",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "draft" => Ok(PlanStatus::Draft),
            "approved" => Ok(PlanStatus::Approved),
            "in_progress" => Ok(PlanStatus::InProgress),
            "completed" => Ok(PlanStatus::Completed),
            "aborted" => Ok(PlanStatus::Aborted),
            _ => Err(OrdneError::Config(format!("Invalid plan status: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    pub id: i64,
    pub plan_id: i64,
    pub file_id: i64,
    pub action: StepAction,
    pub source_path: String,
    pub source_drive_id: i64,
    pub dest_path: Option<String>,
    pub dest_drive_id: Option<i64>,
    pub status: StepStatus,
    pub pre_hash: Option<String>,
    pub post_hash: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub step_order: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum StepAction {
    Move,
    Copy,
    Delete,
    Hardlink,
    Symlink,
}

impl StepAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepAction::Move => "move",
            StepAction::Copy => "copy",
            StepAction::Delete => "delete",
            StepAction::Hardlink => "hardlink",
            StepAction::Symlink => "symlink",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "move" => Ok(StepAction::Move),
            "copy" => Ok(StepAction::Copy),
            "delete" => Ok(StepAction::Delete),
            "hardlink" => Ok(StepAction::Hardlink),
            "symlink" => Ok(StepAction::Symlink),
            _ => Err(OrdneError::Config(format!("Invalid step action: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    RolledBack,
}

impl StepStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            StepStatus::Pending => "pending",
            StepStatus::InProgress => "in_progress",
            StepStatus::Completed => "completed",
            StepStatus::Failed => "failed",
            StepStatus::RolledBack => "rolled_back",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(StepStatus::Pending),
            "in_progress" => Ok(StepStatus::InProgress),
            "completed" => Ok(StepStatus::Completed),
            "failed" => Ok(StepStatus::Failed),
            "rolled_back" => Ok(StepStatus::RolledBack),
            _ => Err(OrdneError::Config(format!("Invalid step status: {}", s))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub action: String,
    pub file_id: Option<i64>,
    pub plan_id: Option<i64>,
    pub drive_id: Option<i64>,
    pub details: Option<String>,
    pub agent_mode: Option<String>,
}

pub trait Database {
    fn initialize(&mut self) -> Result<()>;
    fn get_drive(&self, label: &str) -> Result<Option<Drive>>;
    fn get_drive_by_id(&self, id: i64) -> Result<Option<Drive>>;
    fn add_drive(&mut self, drive: &Drive) -> Result<i64>;
    fn list_drives(&self) -> Result<Vec<Drive>>;
    fn update_drive_online_status(&mut self, label: &str, is_online: bool) -> Result<()>;
    fn get_file(&self, id: i64) -> Result<Option<File>>;
    fn add_file(&mut self, file: &File) -> Result<i64>;
    fn update_file_status(&mut self, id: i64, status: FileStatus) -> Result<()>;
}

pub struct SqliteDatabase {
    conn: Connection,
}

impl SqliteDatabase {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        Ok(Self { conn })
    }

    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn conn_mut(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

impl Database for SqliteDatabase {
    fn initialize(&mut self) -> Result<()> {
        schema::initialize_schema(&self.conn)
    }

    fn get_drive(&self, label: &str) -> Result<Option<Drive>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, device_id, device_path, uuid, mount_path, fs_type, total_bytes,
                    role, is_online, is_readonly, backend, rclone_remote, scanned_at, added_at
             FROM drives WHERE label = ?1"
        )?;

        let drive = stmt
            .query_row([label], |row| {
                Ok(Drive {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    device_id: row.get(2)?,
                    device_path: row.get(3)?,
                    uuid: row.get(4)?,
                    mount_path: row.get(5)?,
                    fs_type: row.get(6)?,
                    total_bytes: row.get(7)?,
                    role: DriveRole::from_str(&row.get::<_, String>(8)?).unwrap(),
                    is_online: row.get(9)?,
                    is_readonly: row.get(10)?,
                    backend: Backend::from_str(&row.get::<_, String>(11)?).unwrap(),
                    rclone_remote: row.get(12)?,
                    scanned_at: row.get::<_, Option<String>>(13)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    added_at: row.get::<_, Option<String>>(14)?
                        .and_then(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .or_else(|| {
                                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                        .ok()
                                        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).fixed_offset())
                                })
                        })
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|| Utc::now()),
                })
            })
            .optional()?;

        Ok(drive)
    }

    fn get_drive_by_id(&self, id: i64) -> Result<Option<Drive>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, device_id, device_path, uuid, mount_path, fs_type, total_bytes,
                    role, is_online, is_readonly, backend, rclone_remote, scanned_at, added_at
             FROM drives WHERE id = ?1"
        )?;

        let drive = stmt
            .query_row([id], |row| {
                Ok(Drive {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    device_id: row.get(2)?,
                    device_path: row.get(3)?,
                    uuid: row.get(4)?,
                    mount_path: row.get(5)?,
                    fs_type: row.get(6)?,
                    total_bytes: row.get(7)?,
                    role: DriveRole::from_str(&row.get::<_, String>(8)?).unwrap(),
                    is_online: row.get(9)?,
                    is_readonly: row.get(10)?,
                    backend: Backend::from_str(&row.get::<_, String>(11)?).unwrap(),
                    rclone_remote: row.get(12)?,
                    scanned_at: row.get::<_, Option<String>>(13)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    added_at: row.get::<_, Option<String>>(14)?
                        .and_then(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .or_else(|| {
                                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                        .ok()
                                        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).fixed_offset())
                                })
                        })
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|| Utc::now()),
                })
            })
            .optional()?;

        Ok(drive)
    }

    fn add_drive(&mut self, drive: &Drive) -> Result<i64> {
        self.conn.execute(
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
        Ok(self.conn.last_insert_rowid())
    }

    fn list_drives(&self) -> Result<Vec<Drive>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, label, device_id, device_path, uuid, mount_path, fs_type, total_bytes,
                    role, is_online, is_readonly, backend, rclone_remote, scanned_at, added_at
             FROM drives ORDER BY added_at"
        )?;

        let drives = stmt
            .query_map([], |row| {
                Ok(Drive {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    device_id: row.get(2)?,
                    device_path: row.get(3)?,
                    uuid: row.get(4)?,
                    mount_path: row.get(5)?,
                    fs_type: row.get(6)?,
                    total_bytes: row.get(7)?,
                    role: DriveRole::from_str(&row.get::<_, String>(8)?).unwrap(),
                    is_online: row.get(9)?,
                    is_readonly: row.get(10)?,
                    backend: Backend::from_str(&row.get::<_, String>(11)?).unwrap(),
                    rclone_remote: row.get(12)?,
                    scanned_at: row.get::<_, Option<String>>(13)?
                        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                        .map(|dt| dt.with_timezone(&Utc)),
                    added_at: row.get::<_, Option<String>>(14)?
                        .and_then(|s| {
                            DateTime::parse_from_rfc3339(&s)
                                .ok()
                                .or_else(|| {
                                    chrono::NaiveDateTime::parse_from_str(&s, "%Y-%m-%d %H:%M:%S")
                                        .ok()
                                        .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc).fixed_offset())
                                })
                        })
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|| Utc::now()),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(drives)
    }

    fn update_drive_online_status(&mut self, label: &str, is_online: bool) -> Result<()> {
        let rows = self.conn.execute(
            "UPDATE drives SET is_online = ?1 WHERE label = ?2",
            (is_online, label),
        )?;
        if rows == 0 {
            return Err(OrdneError::DriveNotFound(label.to_string()));
        }
        Ok(())
    }

    fn get_file(&self, id: i64) -> Result<Option<File>> {
        files::get_file(&self.conn, id)
    }

    fn add_file(&mut self, file: &File) -> Result<i64> {
        files::add_file(&self.conn, file)
    }

    fn update_file_status(&mut self, id: i64, status: FileStatus) -> Result<()> {
        files::update_file_status(&self.conn, id, status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> SqliteDatabase {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn test_drive_crud() {
        let mut db = create_test_db();

        let drive = Drive {
            id: 0,
            label: "test_drive".to_string(),
            device_id: Some("/dev/disk/by-id/test".to_string()),
            device_path: Some("/dev/sda1".to_string()),
            uuid: Some("test-uuid".to_string()),
            mount_path: Some("/mnt/test".to_string()),
            fs_type: Some("ext4".to_string()),
            total_bytes: Some(1_000_000_000),
            role: DriveRole::Source,
            is_online: true,
            is_readonly: false,
            backend: Backend::Local,
            rclone_remote: None,
            scanned_at: None,
            added_at: Utc::now(),
        };

        let id = db.add_drive(&drive).unwrap();
        assert!(id > 0);

        let retrieved = db.get_drive("test_drive").unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.label, "test_drive");
        assert_eq!(retrieved.role, DriveRole::Source);
        assert_eq!(retrieved.backend, Backend::Local);

        db.update_drive_online_status("test_drive", false)
            .unwrap();
        let updated = db.get_drive("test_drive").unwrap().unwrap();
        assert!(!updated.is_online);

        let all_drives = db.list_drives().unwrap();
        assert_eq!(all_drives.len(), 1);
    }

    #[test]
    fn test_audit_log() {
        let mut db = create_test_db();

        let entry = AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "test_action".to_string(),
            file_id: Some(1),
            plan_id: None,
            drive_id: Some(1),
            details: Some("test details".to_string()),
            agent_mode: Some("manual".to_string()),
        };

        let id = db.log_audit(&entry).unwrap();
        assert!(id > 0);
    }
}
