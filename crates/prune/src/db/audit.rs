use crate::db::AuditLogEntry;
use crate::error::Result;
use chrono::{DateTime, Utc};

pub trait AuditDatabase {
    fn log_audit(&mut self, entry: &AuditLogEntry) -> Result<i64>;
    fn get_audit_entries(
        &self,
        plan_id: Option<i64>,
        file_id: Option<i64>,
        limit: Option<i32>,
    ) -> Result<Vec<AuditLogEntry>>;
    fn get_audit_entries_for_plan(&self, plan_id: i64) -> Result<Vec<AuditLogEntry>>;
}

impl AuditDatabase for crate::db::SqliteDatabase {
    fn log_audit(&mut self, entry: &AuditLogEntry) -> Result<i64> {
        let conn = self.conn_mut();
        conn.execute(
            "INSERT INTO audit_log (action, file_id, plan_id, drive_id, details, agent_mode)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &entry.action,
                entry.file_id,
                entry.plan_id,
                entry.drive_id,
                &entry.details,
                &entry.agent_mode,
            ),
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn get_audit_entries(
        &self,
        plan_id: Option<i64>,
        file_id: Option<i64>,
        limit: Option<i32>,
    ) -> Result<Vec<AuditLogEntry>> {
        let conn = self.conn();
        let mut query = "SELECT id, timestamp, action, file_id, plan_id, drive_id, details, agent_mode
                         FROM audit_log WHERE 1=1".to_string();

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(pid) = plan_id {
            query.push_str(" AND plan_id = ?");
            params.push(Box::new(pid));
        }

        if let Some(fid) = file_id {
            query.push_str(" AND file_id = ?");
            params.push(Box::new(fid));
        }

        query.push_str(" ORDER BY timestamp DESC");

        if let Some(lim) = limit {
            query.push_str(" LIMIT ?");
            params.push(Box::new(lim));
        }

        let mut stmt = conn.prepare(&query)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let entries = stmt
            .query_map(&param_refs[..], |row| {
                Ok(AuditLogEntry {
                    id: row.get(0)?,
                    timestamp: row
                        .get::<_, String>(1)?
                        .parse::<DateTime<Utc>>()
                        .unwrap_or_else(|_| Utc::now()),
                    action: row.get(2)?,
                    file_id: row.get(3)?,
                    plan_id: row.get(4)?,
                    drive_id: row.get(5)?,
                    details: row.get(6)?,
                    agent_mode: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(entries)
    }

    fn get_audit_entries_for_plan(&self, plan_id: i64) -> Result<Vec<AuditLogEntry>> {
        self.get_audit_entries(Some(plan_id), None, None)
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
    fn test_audit_log() {
        let mut db = create_test_db();

        let entry = AuditLogEntry {
            id: 0,
            timestamp: Utc::now(),
            action: "file_copied".to_string(),
            file_id: Some(1),
            plan_id: Some(1),
            drive_id: Some(1),
            details: Some("Test copy operation".to_string()),
            agent_mode: Some("manual".to_string()),
        };

        let id = db.log_audit(&entry).unwrap();
        assert!(id > 0);

        let entries = db.get_audit_entries(Some(1), None, None).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].action, "file_copied");
    }

    #[test]
    fn test_audit_log_filtering() {
        let mut db = create_test_db();

        for i in 1..=5 {
            let entry = AuditLogEntry {
                id: 0,
                timestamp: Utc::now(),
                action: format!("action_{}", i),
                file_id: Some(i),
                plan_id: Some(1),
                drive_id: Some(1),
                details: None,
                agent_mode: None,
            };
            db.log_audit(&entry).unwrap();
        }

        let all_entries = db.get_audit_entries(None, None, None).unwrap();
        assert_eq!(all_entries.len(), 5);

        let limited = db.get_audit_entries(None, None, Some(3)).unwrap();
        assert_eq!(limited.len(), 3);

        let for_file = db.get_audit_entries(None, Some(1), None).unwrap();
        assert_eq!(for_file.len(), 1);
    }
}
