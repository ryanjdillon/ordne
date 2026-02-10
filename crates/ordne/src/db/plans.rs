use crate::db::{MigrationPlan, MigrationStep, PlanStatus, StepAction, StepStatus};
use crate::error::{OrdneError, Result};
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

pub trait PlansDatabase {
    fn create_plan(&mut self, plan: &MigrationPlan) -> Result<i64>;
    fn get_plan(&self, id: i64) -> Result<Option<MigrationPlan>>;
    fn list_plans(&self, status_filter: Option<PlanStatus>) -> Result<Vec<MigrationPlan>>;
    fn update_plan_status(&mut self, id: i64, status: PlanStatus) -> Result<()>;
    fn update_plan_progress(&mut self, id: i64, completed_files: i32, completed_bytes: i64)
        -> Result<()>;
    fn add_step(&mut self, step: &MigrationStep) -> Result<i64>;
    fn get_step(&self, id: i64) -> Result<Option<MigrationStep>>;
    fn get_steps_for_plan(&self, plan_id: i64) -> Result<Vec<MigrationStep>>;
    fn update_step_status(
        &mut self,
        id: i64,
        status: StepStatus,
        error: Option<String>,
    ) -> Result<()>;
    fn update_step_hashes(&mut self, id: i64, pre_hash: String, post_hash: Option<String>)
        -> Result<()>;
    fn mark_step_executed(&mut self, id: i64) -> Result<()>;
    fn get_pending_steps(&self, plan_id: i64) -> Result<Vec<MigrationStep>>;
}

impl PlansDatabase for crate::db::SqliteDatabase {
    fn create_plan(&mut self, plan: &MigrationPlan) -> Result<i64> {
        let conn = self.conn_mut();
        conn.execute(
            "INSERT INTO migration_plans (description, source_drive_id, target_drive_id,
                                          status, total_files, total_bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &plan.description,
                plan.source_drive_id,
                plan.target_drive_id,
                plan.status.as_str(),
                plan.total_files,
                plan.total_bytes,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn get_plan(&self, id: i64) -> Result<Option<MigrationPlan>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, description, source_drive_id, target_drive_id,
                    status, total_files, total_bytes, completed_files, completed_bytes
             FROM migration_plans WHERE id = ?1",
        )?;

        let plan = stmt
            .query_row([id], |row| {
                Ok(MigrationPlan {
                    id: row.get(0)?,
                    created_at: row
                        .get::<_, String>(1)?
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    description: row.get(2)?,
                    source_drive_id: row.get(3)?,
                    target_drive_id: row.get(4)?,
                    status: PlanStatus::from_str(&row.get::<_, String>(5)?).unwrap(),
                    total_files: row.get(6)?,
                    total_bytes: row.get(7)?,
                    completed_files: row.get(8)?,
                    completed_bytes: row.get(9)?,
                })
            })
            .optional()?;

        Ok(plan)
    }

    fn list_plans(&self, status_filter: Option<PlanStatus>) -> Result<Vec<MigrationPlan>> {
        let conn = self.conn();
        let query = if let Some(status) = status_filter {
            format!(
                "SELECT id, created_at, description, source_drive_id, target_drive_id,
                        status, total_files, total_bytes, completed_files, completed_bytes
                 FROM migration_plans WHERE status = '{}' ORDER BY created_at DESC",
                status.as_str()
            )
        } else {
            "SELECT id, created_at, description, source_drive_id, target_drive_id,
                    status, total_files, total_bytes, completed_files, completed_bytes
             FROM migration_plans ORDER BY created_at DESC"
                .to_string()
        };

        let mut stmt = conn.prepare(&query)?;
        let plans = stmt
            .query_map([], |row| {
                Ok(MigrationPlan {
                    id: row.get(0)?,
                    created_at: row
                        .get::<_, String>(1)?
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    description: row.get(2)?,
                    source_drive_id: row.get(3)?,
                    target_drive_id: row.get(4)?,
                    status: PlanStatus::from_str(&row.get::<_, String>(5)?).unwrap(),
                    total_files: row.get(6)?,
                    total_bytes: row.get(7)?,
                    completed_files: row.get(8)?,
                    completed_bytes: row.get(9)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(plans)
    }

    fn update_plan_status(&mut self, id: i64, status: PlanStatus) -> Result<()> {
        let conn = self.conn_mut();
        let rows = conn.execute(
            "UPDATE migration_plans SET status = ?1 WHERE id = ?2",
            (status.as_str(), id),
        )?;
        if rows == 0 {
            return Err(OrdneError::PlanNotFound(id));
        }
        Ok(())
    }

    fn update_plan_progress(
        &mut self,
        id: i64,
        completed_files: i32,
        completed_bytes: i64,
    ) -> Result<()> {
        let conn = self.conn_mut();
        let rows = conn.execute(
            "UPDATE migration_plans SET completed_files = ?1, completed_bytes = ?2 WHERE id = ?3",
            (completed_files, completed_bytes, id),
        )?;
        if rows == 0 {
            return Err(OrdneError::PlanNotFound(id));
        }
        Ok(())
    }

    fn add_step(&mut self, step: &MigrationStep) -> Result<i64> {
        let conn = self.conn_mut();
        conn.execute(
            "INSERT INTO migration_steps (plan_id, file_id, action, source_path,
                                          source_drive_id, dest_path, dest_drive_id,
                                          status, step_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (
                step.plan_id,
                step.file_id,
                step.action.as_str(),
                &step.source_path,
                step.source_drive_id,
                &step.dest_path,
                step.dest_drive_id,
                step.status.as_str(),
                step.step_order,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn get_step(&self, id: i64) -> Result<Option<MigrationStep>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, plan_id, file_id, action, source_path, source_drive_id,
                    dest_path, dest_drive_id, status, pre_hash, post_hash,
                    executed_at, error, step_order
             FROM migration_steps WHERE id = ?1",
        )?;

        let step = stmt
            .query_row([id], |row| {
                Ok(MigrationStep {
                    id: row.get(0)?,
                    plan_id: row.get(1)?,
                    file_id: row.get(2)?,
                    action: StepAction::from_str(&row.get::<_, String>(3)?).unwrap(),
                    source_path: row.get(4)?,
                    source_drive_id: row.get(5)?,
                    dest_path: row.get(6)?,
                    dest_drive_id: row.get(7)?,
                    status: StepStatus::from_str(&row.get::<_, String>(8)?).unwrap(),
                    pre_hash: row.get(9)?,
                    post_hash: row.get(10)?,
                    executed_at: row
                        .get::<_, Option<String>>(11)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    error: row.get(12)?,
                    step_order: row.get(13)?,
                })
            })
            .optional()?;

        Ok(step)
    }

    fn get_steps_for_plan(&self, plan_id: i64) -> Result<Vec<MigrationStep>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, plan_id, file_id, action, source_path, source_drive_id,
                    dest_path, dest_drive_id, status, pre_hash, post_hash,
                    executed_at, error, step_order
             FROM migration_steps WHERE plan_id = ?1 ORDER BY step_order",
        )?;

        let steps = stmt
            .query_map([plan_id], |row| {
                Ok(MigrationStep {
                    id: row.get(0)?,
                    plan_id: row.get(1)?,
                    file_id: row.get(2)?,
                    action: StepAction::from_str(&row.get::<_, String>(3)?).unwrap(),
                    source_path: row.get(4)?,
                    source_drive_id: row.get(5)?,
                    dest_path: row.get(6)?,
                    dest_drive_id: row.get(7)?,
                    status: StepStatus::from_str(&row.get::<_, String>(8)?).unwrap(),
                    pre_hash: row.get(9)?,
                    post_hash: row.get(10)?,
                    executed_at: row
                        .get::<_, Option<String>>(11)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    error: row.get(12)?,
                    step_order: row.get(13)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(steps)
    }

    fn update_step_status(
        &mut self,
        id: i64,
        status: StepStatus,
        error: Option<String>,
    ) -> Result<()> {
        let conn = self.conn_mut();
        conn.execute(
            "UPDATE migration_steps SET status = ?1, error = ?2 WHERE id = ?3",
            (status.as_str(), error, id),
        )?;
        Ok(())
    }

    fn update_step_hashes(
        &mut self,
        id: i64,
        pre_hash: String,
        post_hash: Option<String>,
    ) -> Result<()> {
        let conn = self.conn_mut();
        conn.execute(
            "UPDATE migration_steps SET pre_hash = ?1, post_hash = ?2 WHERE id = ?3",
            (pre_hash, post_hash, id),
        )?;
        Ok(())
    }

    fn mark_step_executed(&mut self, id: i64) -> Result<()> {
        let conn = self.conn_mut();
        conn.execute(
            "UPDATE migration_steps SET executed_at = ?1 WHERE id = ?2",
            (Utc::now().to_rfc3339(), id),
        )?;
        Ok(())
    }

    fn get_pending_steps(&self, plan_id: i64) -> Result<Vec<MigrationStep>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT id, plan_id, file_id, action, source_path, source_drive_id,
                    dest_path, dest_drive_id, status, pre_hash, post_hash,
                    executed_at, error, step_order
             FROM migration_steps WHERE plan_id = ?1 AND status = 'pending'
             ORDER BY step_order",
        )?;

        let steps = stmt
            .query_map([plan_id], |row| {
                Ok(MigrationStep {
                    id: row.get(0)?,
                    plan_id: row.get(1)?,
                    file_id: row.get(2)?,
                    action: StepAction::from_str(&row.get::<_, String>(3)?).unwrap(),
                    source_path: row.get(4)?,
                    source_drive_id: row.get(5)?,
                    dest_path: row.get(6)?,
                    dest_drive_id: row.get(7)?,
                    status: StepStatus::from_str(&row.get::<_, String>(8)?).unwrap(),
                    pre_hash: row.get(9)?,
                    post_hash: row.get(10)?,
                    executed_at: row
                        .get::<_, Option<String>>(11)?
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok()),
                    error: row.get(12)?,
                    step_order: row.get(13)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(steps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Database, SqliteDatabase};

    fn create_test_db() -> SqliteDatabase {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();
        db
    }

    #[test]
    fn test_plan_crud() {
        let mut db = create_test_db();

        // Create required drives
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'source', 1, 'local')",
            rusqlite::params![1, "drive1"],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'target', 1, 'local')",
            rusqlite::params![2, "drive2"],
        ).unwrap();

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some("Test migration".to_string()),
            source_drive_id: Some(1),
            target_drive_id: Some(2),
            status: PlanStatus::Draft,
            total_files: 100,
            total_bytes: 1_000_000,
            completed_files: 0,
            completed_bytes: 0,
        };

        let id = db.create_plan(&plan).unwrap();
        assert!(id > 0);

        let retrieved = db.get_plan(id).unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.total_files, 100);
        assert_eq!(retrieved.status, PlanStatus::Draft);

        db.update_plan_status(id, PlanStatus::Approved).unwrap();
        let updated = db.get_plan(id).unwrap().unwrap();
        assert_eq!(updated.status, PlanStatus::Approved);

        db.update_plan_progress(id, 50, 500_000).unwrap();
        let progress = db.get_plan(id).unwrap().unwrap();
        assert_eq!(progress.completed_files, 50);
        assert_eq!(progress.completed_bytes, 500_000);
    }

    #[test]
    fn test_step_crud() {
        let mut db = create_test_db();

        // Create required drives and file
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'source', 1, 'local')",
            rusqlite::params![1, "drive1"],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO drives (id, label, role, is_online, backend) VALUES (?1, ?2, 'target', 1, 'local')",
            rusqlite::params![2, "drive2"],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO files (id, drive_id, path, abs_path, filename, size_bytes, status, indexed_at)
             VALUES (1, 1, '/test.txt', '/mnt/drive1/test.txt', 'test.txt', 1000, 'indexed', datetime('now'))",
            [],
        ).unwrap();

        let plan = MigrationPlan {
            id: 0,
            created_at: Utc::now(),
            description: Some("Test migration".to_string()),
            source_drive_id: Some(1),
            target_drive_id: Some(2),
            status: PlanStatus::Draft,
            total_files: 1,
            total_bytes: 1000,
            completed_files: 0,
            completed_bytes: 0,
        };

        let plan_id = db.create_plan(&plan).unwrap();

        let step = MigrationStep {
            id: 0,
            plan_id,
            file_id: 1,
            action: StepAction::Move,
            source_path: "/source/file.txt".to_string(),
            source_drive_id: 1,
            dest_path: Some("/dest/file.txt".to_string()),
            dest_drive_id: Some(2),
            status: StepStatus::Pending,
            pre_hash: None,
            post_hash: None,
            executed_at: None,
            error: None,
            step_order: 1,
        };

        let step_id = db.add_step(&step).unwrap();
        assert!(step_id > 0);

        let retrieved = db.get_step(step_id).unwrap();
        assert!(retrieved.is_some());

        let steps = db.get_steps_for_plan(plan_id).unwrap();
        assert_eq!(steps.len(), 1);

        db.update_step_status(step_id, StepStatus::Completed, None)
            .unwrap();
        let updated = db.get_step(step_id).unwrap().unwrap();
        assert_eq!(updated.status, StepStatus::Completed);
    }
}
