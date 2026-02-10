use crate::db::{
    AuditDatabase, AuditLogEntry, Backend, Database, MigrationStep, PlansDatabase, StepAction,
    StepStatus,
};
use crate::error::{PruneError, Result};
use crate::migrate::{hash, rclone, rsync};
use chrono::Utc;
use std::fs;
use std::path::Path;

pub struct RollbackEngine<'a, D: Database + PlansDatabase + AuditDatabase> {
    db: &'a mut D,
    verify_hashes: bool,
}

impl<'a, D: Database + PlansDatabase + AuditDatabase> RollbackEngine<'a, D> {
    pub fn new(db: &'a mut D, verify_hashes: bool) -> Self {
        Self { db, verify_hashes }
    }

    pub fn rollback_plan(&mut self, plan_id: i64) -> Result<()> {
        log::info!("Starting rollback for plan {}", plan_id);

        let _plan = self
            .db
            .get_plan(plan_id)?
            .ok_or(PruneError::PlanNotFound(plan_id))?;

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "rollback_started".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some("Starting plan rollback".to_string()),
            agent_mode: Some("manual".to_string()),
        })?;

        let steps = self.db.get_steps_for_plan(plan_id)?;
        let completed_steps: Vec<_> = steps
            .into_iter()
            .filter(|s| s.status == StepStatus::Completed)
            .collect();

        log::info!(
            "Found {} completed steps to rollback",
            completed_steps.len()
        );

        for step in completed_steps.iter().rev() {
            match self.rollback_step(step) {
                Ok(()) => {
                    self.db
                        .update_step_status(step.id, StepStatus::RolledBack, None)?;

                    self.db.log_audit(&AuditLogEntry {
                        id: 0,
                        timestamp: Utc::now(),
                        action: "step_rolled_back".to_string(),
                        file_id: Some(step.file_id),
                        plan_id: Some(plan_id),
                        drive_id: Some(step.source_drive_id),
                        details: Some(format!("Step {} rolled back successfully", step.id)),
                        agent_mode: Some("manual".to_string()),
                    })?;
                }
                Err(e) => {
                    log::error!("Failed to rollback step {}: {}", step.id, e);

                    self.db.log_audit(&AuditLogEntry {
                        id: 0,
                        timestamp: Utc::now(),
                        action: "step_rollback_failed".to_string(),
                        file_id: Some(step.file_id),
                        plan_id: Some(plan_id),
                        drive_id: Some(step.source_drive_id),
                        details: Some(format!("Step rollback failed: {}", e)),
                        agent_mode: Some("manual".to_string()),
                    })?;

                    return Err(e);
                }
            }
        }

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "rollback_completed".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some(format!(
                "Rollback completed for {} steps",
                completed_steps.len()
            )),
            agent_mode: Some("manual".to_string()),
        })?;

        Ok(())
    }

    fn rollback_step(&mut self, step: &MigrationStep) -> Result<()> {
        log::info!(
            "Rolling back step {}: {:?} {}",
            step.id,
            step.action,
            step.source_path
        );

        match step.action {
            StepAction::Copy => self.rollback_copy(step),
            StepAction::Move => self.rollback_move(step),
            StepAction::Delete => self.rollback_delete(step),
            StepAction::Hardlink => self.rollback_hardlink(step),
            StepAction::Symlink => self.rollback_symlink(step),
        }
    }

    fn rollback_copy(&mut self, step: &MigrationStep) -> Result<()> {
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path for rollback".to_string()))?;
        let dest_path = Path::new(dest_path);

        if dest_path.exists() {
            if self.verify_hashes {
                if let Some(post_hash) = &step.post_hash {
                    hash::verify_destination(dest_path, post_hash)?;
                }
            }

            fs::remove_file(dest_path)?;
            log::info!("Removed copied file: {}", dest_path.display());
        } else {
            log::warn!("Destination file already removed: {}", dest_path.display());
        }

        Ok(())
    }

    fn rollback_move(&mut self, step: &MigrationStep) -> Result<()> {
        let source_path = Path::new(&step.source_path);
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path for rollback".to_string()))?;
        let dest_path = Path::new(dest_path);

        if !dest_path.exists() {
            return Err(PruneError::Migration(format!(
                "Cannot rollback move: destination file not found: {}",
                dest_path.display()
            )));
        }

        if self.verify_hashes {
            if let Some(pre_hash) = &step.pre_hash {
                hash::verify_destination(dest_path, pre_hash)?;
            }
        }

        if let Some(parent) = source_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let dest_drive_id = step
            .dest_drive_id
            .ok_or_else(|| PruneError::Migration("No destination drive".to_string()))?;

        let dest_drive = self
            .db
            .get_drive_by_id(dest_drive_id)?
            .ok_or_else(|| PruneError::DriveNotFound("destination".to_string()))?;

        match dest_drive.backend {
            Backend::Local => {
                rsync::copy_file(dest_path, source_path)?;
            }
            Backend::Rclone => {
                let remote = dest_drive
                    .rclone_remote
                    .as_ref()
                    .ok_or_else(|| PruneError::Migration("No rclone remote".to_string()))?;
                let remote_path = dest_path
                    .to_str()
                    .ok_or_else(|| PruneError::Migration("Invalid path".to_string()))?;
                rclone::copy_from_remote(remote, remote_path, source_path)?;
            }
        }

        if self.verify_hashes && dest_drive.backend == Backend::Local {
            if let Some(pre_hash) = &step.pre_hash {
                hash::verify_destination(source_path, pre_hash)?;
            }
        }

        if dest_drive.backend == Backend::Local {
            fs::remove_file(dest_path)?;
        }

        log::info!(
            "Restored moved file from {} to {}",
            dest_path.display(),
            source_path.display()
        );

        Ok(())
    }

    fn rollback_delete(&mut self, _step: &MigrationStep) -> Result<()> {
        Err(PruneError::Migration(
            "Cannot rollback delete: file is permanently deleted".to_string(),
        ))
    }

    fn rollback_hardlink(&mut self, step: &MigrationStep) -> Result<()> {
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path for rollback".to_string()))?;
        let dest_path = Path::new(dest_path);

        if dest_path.exists() {
            fs::remove_file(dest_path)?;
            log::info!("Removed hardlink: {}", dest_path.display());
        } else {
            log::warn!("Hardlink already removed: {}", dest_path.display());
        }

        Ok(())
    }

    fn rollback_symlink(&mut self, step: &MigrationStep) -> Result<()> {
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path for rollback".to_string()))?;
        let dest_path = Path::new(dest_path);

        if dest_path.exists() {
            fs::remove_file(dest_path)?;
            log::info!("Removed symlink: {}", dest_path.display());
        } else {
            log::warn!("Symlink already removed: {}", dest_path.display());
        }

        Ok(())
    }

    pub fn can_rollback(&self, plan_id: i64) -> Result<bool> {
        let steps = self.db.get_steps_for_plan(plan_id)?;

        for step in steps {
            if step.status == StepStatus::Completed {
                if step.action == StepAction::Delete {
                    log::warn!(
                        "Cannot rollback plan {}: contains completed delete operations",
                        plan_id
                    );
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Backend, Drive, DriveRole, PlanStatus, SqliteDatabase};
    use std::fs;
    use tempfile::TempDir;

    fn create_test_db() -> SqliteDatabase {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    fn create_test_drive(db: &mut SqliteDatabase, label: &str, mount_path: &str) -> i64 {
        let drive = Drive {
            id: 0,
            label: label.to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: Some(mount_path.to_string()),
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

        db.add_drive(&drive).unwrap()
    }

    #[test]
    fn test_rollback_copy() {
        let mut db = create_test_db();
        let temp_dir = TempDir::new().unwrap();

        let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());
        let dest_drive = create_test_drive(&mut db, "dest", temp_dir.path().to_str().unwrap());

        let source_file = temp_dir.path().join("source.txt");
        let dest_file = temp_dir.path().join("dest.txt");
        fs::write(&source_file, b"test content").unwrap();
        fs::write(&dest_file, b"test content").unwrap();

        let step = MigrationStep {
            id: 1,
            plan_id: 1,
            file_id: 1,
            action: StepAction::Copy,
            source_path: source_file.to_str().unwrap().to_string(),
            source_drive_id: source_drive,
            dest_path: Some(dest_file.to_str().unwrap().to_string()),
            dest_drive_id: Some(dest_drive),
            status: StepStatus::Completed,
            pre_hash: None,
            post_hash: None,
            executed_at: Some(Utc::now()),
            error: None,
            step_order: 0,
        };

        let mut rollback = RollbackEngine::new(&mut db, false);
        let result = rollback.rollback_copy(&step);
        assert!(result.is_ok());
        assert!(!dest_file.exists());
        assert!(source_file.exists());
    }

    #[test]
    fn test_can_rollback() {
        let mut db = create_test_db();

        // Create required drives
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (1, 'drive1', 'source', 1, 'local')",
            [],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (2, 'drive2', 'target', 1, 'local')",
            [],
        ).unwrap();

        // Create required files
        db.conn().execute(
            "INSERT INTO files (id, drive_id, path, abs_path, filename, size_bytes, status, indexed_at)
             VALUES (1, 1, '/source/file.txt', '/mnt/drive1/file.txt', 'file.txt', 1000, 'indexed', datetime('now'))",
            [],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO files (id, drive_id, path, abs_path, filename, size_bytes, status, indexed_at)
             VALUES (2, 1, '/source/file2.txt', '/mnt/drive1/file2.txt', 'file2.txt', 1000, 'indexed', datetime('now'))",
            [],
        ).unwrap();

        let plan = crate::db::MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some("Test plan".to_string()),
            source_drive_id: Some(1),
            target_drive_id: Some(2),
            status: PlanStatus::Completed,
            total_files: 1,
            total_bytes: 1000,
            completed_files: 1,
            completed_bytes: 1000,
        };

        let plan_id = db.create_plan(&plan).unwrap();

        let copy_step = MigrationStep {
            id: 0,
            plan_id,
            file_id: 1,
            action: StepAction::Copy,
            source_path: "/source/file.txt".to_string(),
            source_drive_id: 1,
            dest_path: Some("/dest/file.txt".to_string()),
            dest_drive_id: Some(2),
            status: StepStatus::Completed,
            pre_hash: None,
            post_hash: None,
            executed_at: Some(Utc::now()),
            error: None,
            step_order: 0,
        };

        db.add_step(&copy_step).unwrap();

        {
            let rollback = RollbackEngine::new(&mut db, false);
            let can_rollback = rollback.can_rollback(plan_id).unwrap();
            assert!(can_rollback);
        }

        let delete_step = MigrationStep {
            id: 0,
            plan_id,
            file_id: 2,
            action: StepAction::Delete,
            source_path: "/source/file2.txt".to_string(),
            source_drive_id: 1,
            dest_path: None,
            dest_drive_id: None,
            status: StepStatus::Completed,
            pre_hash: None,
            post_hash: None,
            executed_at: Some(Utc::now()),
            error: None,
            step_order: 1,
        };

        db.add_step(&delete_step).unwrap();

        {
            let rollback = RollbackEngine::new(&mut db, false);
            let can_rollback = rollback.can_rollback(plan_id).unwrap();
            assert!(!can_rollback);
        }
    }
}
