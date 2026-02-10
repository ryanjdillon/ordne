use crate::db::{
    AuditDatabase, AuditLogEntry, Database, File, MigrationPlan, MigrationStep,
    PlanStatus, PlansDatabase, StepAction, StepStatus,
};
use crate::error::Result;
use crate::migrate::space;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct PlannerOptions {
    pub max_batch_size_bytes: Option<u64>,
    pub enforce_space_limits: bool,
    pub dry_run: bool,
}

impl Default for PlannerOptions {
    fn default() -> Self {
        Self {
            max_batch_size_bytes: None,
            enforce_space_limits: true,
            dry_run: true,
        }
    }
}

pub struct Planner<'a, D: Database + PlansDatabase + AuditDatabase> {
    db: &'a mut D,
    options: PlannerOptions,
}

impl<'a, D: Database + PlansDatabase + AuditDatabase> Planner<'a, D> {
    pub fn new(db: &'a mut D, options: PlannerOptions) -> Self {
        Self { db, options }
    }

    pub fn create_delete_trash_plan(&mut self, files: Vec<File>) -> Result<i64> {
        let total_files = files.len() as i32;
        let total_bytes: i64 = files.iter().map(|f| f.size_bytes).sum();

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some(format!("Delete {} trash files", total_files)),
            source_drive_id: None,
            target_drive_id: None,
            status: PlanStatus::Draft,
            total_files,
            total_bytes,
            completed_files: 0,
            completed_bytes: 0,
        };

        let plan_id = self.db.create_plan(&plan)?;

        for (order, file) in files.iter().enumerate() {
            let step = MigrationStep {
                id: 0,
                plan_id,
                file_id: file.id,
                action: StepAction::Delete,
                source_path: file.abs_path.clone(),
                source_drive_id: file.drive_id,
                dest_path: None,
                dest_drive_id: None,
                status: StepStatus::Pending,
                pre_hash: None,
                post_hash: None,
                executed_at: None,
                error: None,
                step_order: order as i32,
            };

            self.db.add_step(&step)?;
        }

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_created".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some(format!("Delete trash plan: {} files", total_files)),
            agent_mode: Some("automated".to_string()),
        })?;

        Ok(plan_id)
    }

    pub fn create_dedup_plan(&mut self, duplicate_files: Vec<File>, original: &File) -> Result<i64> {
        if duplicate_files.is_empty() {
            return Err(crate::error::PruneError::Migration(
                "No duplicate files provided".to_string(),
            ));
        }

        let total_files = duplicate_files.len() as i32;
        let total_bytes: i64 = duplicate_files.iter().map(|f| f.size_bytes).sum();

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some(format!(
                "Deduplicate {} files (keep original: {})",
                total_files, original.abs_path
            )),
            source_drive_id: None,
            target_drive_id: None,
            status: PlanStatus::Draft,
            total_files,
            total_bytes,
            completed_files: 0,
            completed_bytes: 0,
        };

        let plan_id = self.db.create_plan(&plan)?;

        for (order, file) in duplicate_files.iter().enumerate() {
            let step = MigrationStep {
                id: 0,
                plan_id,
                file_id: file.id,
                action: StepAction::Delete,
                source_path: file.abs_path.clone(),
                source_drive_id: file.drive_id,
                dest_path: None,
                dest_drive_id: None,
                status: StepStatus::Pending,
                pre_hash: file.blake3_hash.clone().or_else(|| file.md5_hash.clone()),
                post_hash: None,
                executed_at: None,
                error: None,
                step_order: order as i32,
            };

            self.db.add_step(&step)?;
        }

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_created".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some(format!(
                "Deduplication plan: {} duplicates, original: {}",
                total_files, original.abs_path
            )),
            agent_mode: Some("automated".to_string()),
        })?;

        Ok(plan_id)
    }

    pub fn create_migrate_plan(
        &mut self,
        files: Vec<File>,
        target_drive_id: i64,
        target_mount: &str,
    ) -> Result<i64> {
        if files.is_empty() {
            return Err(crate::error::PruneError::Migration(
                "No files provided for migration".to_string(),
            ));
        }

        let source_drive_id = files[0].drive_id;
        let total_files = files.len() as i32;
        let total_bytes: i64 = files.iter().map(|f| f.size_bytes).sum();

        if self.options.enforce_space_limits {
            space::verify_sufficient_space(target_mount, total_bytes as u64)?;
        }

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some(format!(
                "Migrate {} files to target drive {}",
                total_files, target_drive_id
            )),
            source_drive_id: Some(source_drive_id),
            target_drive_id: Some(target_drive_id),
            status: PlanStatus::Draft,
            total_files,
            total_bytes,
            completed_files: 0,
            completed_bytes: 0,
        };

        let plan_id = self.db.create_plan(&plan)?;

        for (order, file) in files.iter().enumerate() {
            let target_path = file
                .target_path
                .clone()
                .unwrap_or_else(|| file.path.clone());

            let step = MigrationStep {
                id: 0,
                plan_id,
                file_id: file.id,
                action: StepAction::Copy,
                source_path: file.abs_path.clone(),
                source_drive_id: file.drive_id,
                dest_path: Some(format!("{}/{}", target_mount, target_path)),
                dest_drive_id: Some(target_drive_id),
                status: StepStatus::Pending,
                pre_hash: file.blake3_hash.clone().or_else(|| file.md5_hash.clone()),
                post_hash: None,
                executed_at: None,
                error: None,
                step_order: order as i32,
            };

            self.db.add_step(&step)?;
        }

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_created".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: Some(target_drive_id),
            details: Some(format!(
                "Migration plan: {} files, {} bytes",
                total_files, total_bytes
            )),
            agent_mode: Some("automated".to_string()),
        })?;

        Ok(plan_id)
    }

    pub fn create_offload_plan(
        &mut self,
        files: Vec<File>,
        offload_drive_id: i64,
        offload_mount: &str,
    ) -> Result<i64> {
        if files.is_empty() {
            return Err(crate::error::PruneError::Migration(
                "No files provided for offload".to_string(),
            ));
        }

        let source_drive_id = files[0].drive_id;
        let total_files = files.len() as i32;
        let total_bytes: i64 = files.iter().map(|f| f.size_bytes).sum();

        if self.options.enforce_space_limits {
            space::verify_sufficient_space(offload_mount, total_bytes as u64)?;
        }

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some(format!(
                "Offload {} low-priority files to drive {}",
                total_files, offload_drive_id
            )),
            source_drive_id: Some(source_drive_id),
            target_drive_id: Some(offload_drive_id),
            status: PlanStatus::Draft,
            total_files,
            total_bytes,
            completed_files: 0,
            completed_bytes: 0,
        };

        let plan_id = self.db.create_plan(&plan)?;

        for (order, file) in files.iter().enumerate() {
            let copy_step = MigrationStep {
                id: 0,
                plan_id,
                file_id: file.id,
                action: StepAction::Copy,
                source_path: file.abs_path.clone(),
                source_drive_id: file.drive_id,
                dest_path: Some(format!("{}/{}", offload_mount, file.path)),
                dest_drive_id: Some(offload_drive_id),
                status: StepStatus::Pending,
                pre_hash: file.blake3_hash.clone().or_else(|| file.md5_hash.clone()),
                post_hash: None,
                executed_at: None,
                error: None,
                step_order: (order * 2) as i32,
            };

            self.db.add_step(&copy_step)?;

            let delete_step = MigrationStep {
                id: 0,
                plan_id,
                file_id: file.id,
                action: StepAction::Delete,
                source_path: file.abs_path.clone(),
                source_drive_id: file.drive_id,
                dest_path: None,
                dest_drive_id: None,
                status: StepStatus::Pending,
                pre_hash: file.blake3_hash.clone().or_else(|| file.md5_hash.clone()),
                post_hash: None,
                executed_at: None,
                error: None,
                step_order: (order * 2 + 1) as i32,
            };

            self.db.add_step(&delete_step)?;
        }

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_created".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: Some(offload_drive_id),
            details: Some(format!("Offload plan: {} files", total_files)),
            agent_mode: Some("automated".to_string()),
        })?;

        Ok(plan_id)
    }

    pub fn approve_plan(&mut self, plan_id: i64) -> Result<()> {
        self.db
            .update_plan_status(plan_id, PlanStatus::Approved)?;

        self.db.log_audit(&AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "plan_approved".to_string(),
            file_id: None,
            plan_id: Some(plan_id),
            drive_id: None,
            details: Some("Plan approved for execution".to_string()),
            agent_mode: Some("manual".to_string()),
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{FileStatus, Priority, SqliteDatabase};

    fn create_test_db() -> SqliteDatabase {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    fn create_test_file(id: i64, drive_id: i64, path: &str, size: i64) -> File {
        File {
            id,
            drive_id,
            path: path.to_string(),
            abs_path: format!("/mnt/drive/{}", path),
            filename: path.to_string(),
            extension: Some("txt".to_string()),
            size_bytes: size,
            md5_hash: Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
            blake3_hash: None,
            created_at: Some(Utc::now()),
            modified_at: Some(Utc::now()),
            inode: None,
            device_num: None,
            nlinks: None,
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
        }
    }

    fn insert_test_file_to_db(db: &SqliteDatabase, file: &File) -> i64 {
        db.conn().execute(
            "INSERT INTO files (id, drive_id, path, abs_path, filename, extension, size_bytes,
                               md5_hash, mime_type, priority, status, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                file.id,
                file.drive_id,
                &file.path,
                &file.abs_path,
                &file.filename,
                &file.extension,
                file.size_bytes,
                &file.md5_hash,
                &file.mime_type,
                file.priority.as_str(),
                file.status.as_str(),
                file.indexed_at.to_rfc3339(),
            ],
        ).unwrap();
        file.id
    }

    #[test]
    fn test_create_delete_trash_plan() {
        let mut db = create_test_db();

        // Create required drive
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (1, 'drive1', 'source', 1, 'local')",
            [],
        ).unwrap();

        let files = vec![
            create_test_file(1, 1, "trash1.txt", 1000),
            create_test_file(2, 1, "trash2.txt", 2000),
        ];

        // Insert files into database
        for file in &files {
            insert_test_file_to_db(&db, file);
        }

        let options = PlannerOptions::default();
        let mut planner = Planner::new(&mut db, options);

        let plan_id = planner.create_delete_trash_plan(files).unwrap();
        assert!(plan_id > 0);

        let plan = db.get_plan(plan_id).unwrap().unwrap();
        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.total_bytes, 3000);
        assert_eq!(plan.status, PlanStatus::Draft);

        let steps = db.get_steps_for_plan(plan_id).unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].action, StepAction::Delete);
    }

    #[test]
    fn test_create_dedup_plan() {
        let mut db = create_test_db();

        // Create required drive
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (1, 'drive1', 'source', 1, 'local')",
            [],
        ).unwrap();

        let original = create_test_file(1, 1, "original.txt", 1000);
        let duplicates = vec![
            create_test_file(2, 1, "dup1.txt", 1000),
            create_test_file(3, 1, "dup2.txt", 1000),
        ];

        // Insert files into database
        insert_test_file_to_db(&db, &original);
        for file in &duplicates {
            insert_test_file_to_db(&db, file);
        }

        let options = PlannerOptions::default();
        let mut planner = Planner::new(&mut db, options);

        let plan_id = planner.create_dedup_plan(duplicates, &original).unwrap();
        assert!(plan_id > 0);

        let plan = db.get_plan(plan_id).unwrap().unwrap();
        assert_eq!(plan.total_files, 2);
        assert_eq!(plan.status, PlanStatus::Draft);
    }

    #[test]
    fn test_approve_plan() {
        let mut db = create_test_db();

        // Create required drive
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (1, 'drive1', 'source', 1, 'local')",
            [],
        ).unwrap();

        let files = vec![create_test_file(1, 1, "test.txt", 1000)];

        // Insert file into database
        for file in &files {
            insert_test_file_to_db(&db, file);
        }

        let options = PlannerOptions::default();
        let mut planner = Planner::new(&mut db, options);

        let plan_id = planner.create_delete_trash_plan(files).unwrap();
        planner.approve_plan(plan_id).unwrap();

        let plan = db.get_plan(plan_id).unwrap().unwrap();
        assert_eq!(plan.status, PlanStatus::Approved);
    }
}
