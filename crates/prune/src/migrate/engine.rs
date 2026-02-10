use crate::db::{
    AuditDatabase, AuditLogEntry, Backend, Database, MigrationStep, PlanStatus, PlansDatabase,
    StepAction, StepStatus,
};
use crate::error::{PruneError, Result};
use crate::migrate::{hash, rclone, rsync, space};
use chrono::Utc;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub dry_run: bool,
    pub verify_hashes: bool,
    pub retry_count: u32,
    pub enforce_safety: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self {
            dry_run: true,
            verify_hashes: true,
            retry_count: 3,
            enforce_safety: true,
        }
    }
}

pub struct MigrationEngine<'a, D: Database + PlansDatabase + AuditDatabase> {
    db: &'a mut D,
    options: EngineOptions,
}

impl<'a, D: Database + PlansDatabase + AuditDatabase> MigrationEngine<'a, D> {
    pub fn new(db: &'a mut D, options: EngineOptions) -> Self {
        Self { db, options }
    }

    pub fn execute_plan(&mut self, plan_id: i64) -> Result<()> {
        let plan = self
            .db
            .get_plan(plan_id)?
            .ok_or(PruneError::PlanNotFound(plan_id))?;

        if plan.status != PlanStatus::Approved {
            return Err(PruneError::PlanNotApproved(plan_id));
        }

        if self.options.dry_run {
            log::info!("DRY RUN: Would execute plan {}", plan_id);
            return self.dry_run_plan(plan_id);
        }

        self.db
            .update_plan_status(plan_id, PlanStatus::InProgress)?;

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_execution_started".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some("Starting plan execution".to_string()),
            agent_mode: Some("automated".to_string()),
        })?;

        let steps = self.db.get_pending_steps(plan_id)?;
        let mut completed_files = 0;
        let mut completed_bytes = 0i64;

        for step in steps {
            match self.execute_step(&step) {
                Ok(step_bytes) => {
                    completed_files += 1;
                    completed_bytes += step_bytes;
                    self.db
                        .update_plan_progress(plan_id, completed_files, completed_bytes)?;
                }
                Err(e) => {
                    log::error!("Step {} failed: {}", step.id, e);
                    self.db
                        .update_step_status(step.id, StepStatus::Failed, Some(e.to_string()))?;

                    self.db.log_audit(&AuditLogEntry {
                        id: 0,
                        timestamp: Utc::now(),
                        action: "step_failed".to_string(),
                        file_id: Some(step.file_id),
                        plan_id: Some(plan_id),
                        drive_id: Some(step.source_drive_id),
                        details: Some(format!("Step failed: {}", e)),
                        agent_mode: Some("automated".to_string()),
                    })?;

                    self.db
                        .update_plan_status(plan_id, PlanStatus::Aborted)?;
                    return Err(e);
                }
            }
        }

        self.db
            .update_plan_status(plan_id, PlanStatus::Completed)?;

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_execution_completed".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some(format!(
                "Completed {} files, {} bytes",
                completed_files, completed_bytes
            )),
            agent_mode: Some("automated".to_string()),
        })?;

        Ok(())
    }

    fn execute_step(&mut self, step: &MigrationStep) -> Result<i64> {
        log::info!(
            "Executing step {}: {:?} {} -> {:?}",
            step.id,
            step.action,
            step.source_path,
            step.dest_path
        );

        self.db
            .update_step_status(step.id, StepStatus::InProgress, None)?;

        let result = match step.action {
            StepAction::Copy => self.execute_copy(step),
            StepAction::Move => self.execute_move(step),
            StepAction::Delete => self.execute_delete(step),
            StepAction::Hardlink => self.execute_hardlink(step),
            StepAction::Symlink => self.execute_symlink(step),
        };

        match result {
            Ok(bytes) => {
                self.db
                    .update_step_status(step.id, StepStatus::Completed, None)?;
                self.db.mark_step_executed(step.id)?;

                self.db.log_audit(&AuditLogEntry {
                    id: 0,
                    timestamp: Utc::now(),
                    action: format!("step_completed_{}", step.action.as_str()),
                    file_id: Some(step.file_id),
                    plan_id: Some(step.plan_id),
                    drive_id: Some(step.source_drive_id),
                    details: Some(format!("Step {} completed successfully", step.id)),
                    agent_mode: Some("automated".to_string()),
                })?;

                Ok(bytes)
            }
            Err(e) => {
                self.db
                    .update_step_status(step.id, StepStatus::Failed, Some(e.to_string()))?;
                Err(e)
            }
        }
    }

    fn execute_copy(&mut self, step: &MigrationStep) -> Result<i64> {
        let source_path = Path::new(&step.source_path);
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path".to_string()))?;
        let dest_path = Path::new(dest_path);

        if !source_path.exists() {
            return Err(PruneError::FileNotFound(source_path.to_path_buf()));
        }

        let file_size = fs::metadata(source_path)?.len() as i64;

        let pre_hash = if self.options.verify_hashes {
            let hash = hash::compute_blake3_hash(source_path)?;
            self.db.update_step_hashes(step.id, hash.clone(), None)?;
            hash
        } else {
            step.pre_hash
                .clone()
                .ok_or_else(|| PruneError::Migration("No pre-hash available".to_string()))?
        };

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let dest_drive = self
            .db
            .get_drive_by_id(
                step.dest_drive_id
                    .ok_or_else(|| PruneError::Migration("No destination drive".to_string()))?,
            )?
            .ok_or_else(|| PruneError::DriveNotFound("destination".to_string()))?;

        match dest_drive.backend {
            Backend::Local => {
                rsync::copy_file(source_path, dest_path)?;
            }
            Backend::Rclone => {
                let remote = dest_drive
                    .rclone_remote
                    .as_ref()
                    .ok_or_else(|| PruneError::Migration("No rclone remote".to_string()))?;
                let remote_path = dest_path
                    .to_str()
                    .ok_or_else(|| PruneError::Migration("Invalid path".to_string()))?;
                rclone::copy_to_remote(source_path, remote, remote_path)?;
            }
        }

        if self.options.verify_hashes && dest_drive.backend == Backend::Local {
            hash::verify_destination(dest_path, &pre_hash)?;
            self.db
                .update_step_hashes(step.id, pre_hash.clone(), Some(pre_hash))?;
        }

        Ok(file_size)
    }

    fn execute_move(&mut self, step: &MigrationStep) -> Result<i64> {
        let bytes = self.execute_copy(step)?;

        let step = self
            .db
            .get_step(step.id)?
            .ok_or_else(|| PruneError::Migration("Step disappeared".to_string()))?;

        if step.status != StepStatus::Completed {
            return Err(PruneError::Migration(
                "Copy step did not complete".to_string(),
            ));
        }

        let source_path = Path::new(&step.source_path);

        if self.options.enforce_safety && self.options.verify_hashes {
            if let Some(pre_hash) = &step.pre_hash {
                hash::verify_source_unchanged(source_path, pre_hash)?;
            }
        }

        fs::remove_file(source_path)?;

        Ok(bytes)
    }

    fn execute_delete(&mut self, step: &MigrationStep) -> Result<i64> {
        let source_path = Path::new(&step.source_path);

        if !source_path.exists() {
            log::warn!("File already deleted: {}", step.source_path);
            return Ok(0);
        }

        let file_size = fs::metadata(source_path)?.len() as i64;

        if self.options.enforce_safety {
            if let Some(expected_hash) = &step.pre_hash {
                hash::verify_source_unchanged(source_path, expected_hash)?;
            } else {
                return Err(PruneError::Migration(
                    "Cannot delete without hash verification".to_string(),
                ));
            }
        }

        fs::remove_file(source_path)?;

        Ok(file_size)
    }

    fn execute_hardlink(&mut self, step: &MigrationStep) -> Result<i64> {
        let source_path = Path::new(&step.source_path);
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path".to_string()))?;
        let dest_path = Path::new(dest_path);

        if !source_path.exists() {
            return Err(PruneError::FileNotFound(source_path.to_path_buf()));
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::hard_link(source_path, dest_path)?;

        let file_size = fs::metadata(source_path)?.len() as i64;
        Ok(file_size)
    }

    fn execute_symlink(&mut self, step: &MigrationStep) -> Result<i64> {
        let source_path = Path::new(&step.source_path);
        let dest_path = step
            .dest_path
            .as_ref()
            .ok_or_else(|| PruneError::Migration("No destination path".to_string()))?;
        let dest_path = Path::new(dest_path);

        if !source_path.exists() {
            return Err(PruneError::FileNotFound(source_path.to_path_buf()));
        }

        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(source_path, dest_path)?;

        #[cfg(not(unix))]
        return Err(PruneError::Migration(
            "Symlinks not supported on this platform".to_string(),
        ));

        Ok(0)
    }

    fn dry_run_plan(&mut self, plan_id: i64) -> Result<()> {
        let steps = self.db.get_steps_for_plan(plan_id)?;

        log::info!("DRY RUN: Plan {} has {} steps", plan_id, steps.len());

        for step in steps {
            log::info!(
                "DRY RUN: Would execute {:?}: {} -> {:?}",
                step.action,
                step.source_path,
                step.dest_path
            );

            if step.action == StepAction::Copy || step.action == StepAction::Move {
                if let Some(dest_path) = &step.dest_path {
                    if let Some(parent) = Path::new(dest_path).parent() {
                        if let Some(dest_drive_id) = step.dest_drive_id {
                            if let Ok(Some(drive)) = self.db.get_drive_by_id(dest_drive_id) {
                                if drive.backend == Backend::Local {
                                    if parent.exists() {
                                        let space_info = space::get_free_space(parent)?;
                                        log::info!(
                                            "DRY RUN: Destination has {} bytes free (max safe: {})",
                                            space_info.free_bytes,
                                            space_info.max_safe_write_bytes()
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Backend, Drive, DriveRole, File, FileStatus, Priority, SqliteDatabase};
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
    fn test_dry_run_mode() {
        let mut db = create_test_db();
        let temp_dir = TempDir::new().unwrap();

        let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

        let source_file = temp_dir.path().join("source.txt");
        fs::write(&source_file, b"test content").unwrap();

        let mut file = File {
            id: 1,
            drive_id: source_drive,
            path: "source.txt".to_string(),
            abs_path: source_file.to_str().unwrap().to_string(),
            filename: "source.txt".to_string(),
            extension: Some("txt".to_string()),
            size_bytes: 12,
            md5_hash: None,
            blake3_hash: None,
            created_at: Some(Utc::now()),
            modified_at: Some(Utc::now()),
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
            priority: Priority::Trash,
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

        // Insert file into database before creating planner
        db.conn().execute(
            "INSERT INTO files (drive_id, path, abs_path, filename, extension, size_bytes,
                               priority, status, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                file.drive_id,
                &file.path,
                &file.abs_path,
                &file.filename,
                &file.extension,
                file.size_bytes,
                file.priority.as_str(),
                file.status.as_str(),
                file.indexed_at.to_rfc3339(),
            ],
        ).unwrap();

        // Update file struct with actual database ID
        file.id = db.conn().last_insert_rowid();

        let planner_opts = crate::migrate::planner::PlannerOptions::default();
        let mut planner = crate::migrate::planner::Planner::new(&mut db, planner_opts);

        let plan_id = planner.create_delete_trash_plan(vec![file]).unwrap();
        planner.approve_plan(plan_id).unwrap();

        let engine_opts = EngineOptions {
            dry_run: true,
            verify_hashes: false,
            retry_count: 1,
            enforce_safety: false,
        };
        let mut engine = MigrationEngine::new(&mut db, engine_opts);

        let result = engine.execute_plan(plan_id);
        assert!(result.is_ok());

        assert!(source_file.exists());
    }

    #[test]
    fn test_execute_delete_safety() {
        let mut db = create_test_db();
        let temp_dir = TempDir::new().unwrap();

        let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

        let source_file = temp_dir.path().join("to_delete.txt");
        fs::write(&source_file, b"test content").unwrap();

        let step = MigrationStep {
            id: 1,
            plan_id: 1,
            file_id: 1,
            action: StepAction::Delete,
            source_path: source_file.to_str().unwrap().to_string(),
            source_drive_id: source_drive,
            dest_path: None,
            dest_drive_id: None,
            status: StepStatus::Pending,
            pre_hash: None,
            post_hash: None,
            executed_at: None,
            error: None,
            step_order: 0,
        };

        let engine_opts = EngineOptions {
            dry_run: false,
            verify_hashes: true,
            retry_count: 1,
            enforce_safety: true,
        };
        let mut engine = MigrationEngine::new(&mut db, engine_opts);

        let result = engine.execute_delete(&step);
        assert!(result.is_err());
    }
}
