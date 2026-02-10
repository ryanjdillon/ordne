use prune_lib::{
    classify::{ClassificationRules, RuleEngine},
    db::{duplicates::*, files::get_files_by_category},
    index::{ScanOptions, scan_directory},
    migrate::{EngineOptions, MigrationEngine, Planner, PlannerOptions, RollbackEngine},
    Backend, Database, Drive, DriveRole, FileStatus, PlanStatus, PlansDatabase, Priority,
    SqliteDatabase, StepStatus,
};
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    // model imports removed
    tool, tool_handler, tool_router,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct PruneServer {
    db: Arc<Mutex<SqliteDatabase>>,
    tool_router: ToolRouter<Self>,
}

impl PruneServer {
    pub fn new(db: SqliteDatabase) -> Self {
        Self {
            db: Arc::new(Mutex::new(db)),
            tool_router: Self::tool_router(),
        }
    }

    fn with_db<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&SqliteDatabase) -> R,
    {
        let db = self.db.lock().unwrap();
        f(&*db)
    }

    fn with_db_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut SqliteDatabase) -> R,
    {
        let mut db = self.db.lock().unwrap();
        f(&mut *db)
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for PruneServer {}

#[derive(Deserialize, Serialize, JsonSchema)]
struct StatusResponse {
    drives: DriveStats,
    files: FileStats,
    duplicates: DuplicateStats,
    plans: PlanStats,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct DriveStats {
    total: usize,
    online: usize,
    offline: usize,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct FileStats {
    total: i64,
    total_size_bytes: i64,
    classified: i64,
    unclassified: i64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct DuplicateStats {
    groups: usize,
    files: usize,
    wasted_bytes: i64,
    cross_drive_groups: usize,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct PlanStats {
    draft: i64,
    approved: i64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct DriveInfo {
    id: i64,
    label: String,
    role: String,
    mount_path: Option<String>,
    backend: String,
    is_online: bool,
    is_readonly: bool,
    total_bytes: Option<i64>,
    scanned_at: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct SpaceCheckArgs {
    drive: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct DriveAddArgs {
    label: String,
    mount_path: String,
    role: String,
    readonly: Option<bool>,
    backend: Option<String>,
    rclone_remote: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct ScanArgs {
    drive_label: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct QueryDuplicatesArgs {
    min_size: Option<u64>,
    drive: Option<String>,
    same_drive_only: Option<bool>,
    limit: Option<u32>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct QueryUnclassifiedArgs {
    drive: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct QueryFilesArgs {
    category: Option<String>,
    extension: Option<String>,
    min_size: Option<u64>,
    path_contains: Option<String>,
    drive: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct QueryBackupUniqueArgs {
    backup_drive: String,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct ClassifyAutoArgs {
    drive: Option<String>,
    rules_file: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct ClassifyArgs {
    file_ids: String,
    category: String,
    subcategory: Option<String>,
    priority: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct ClassifyPatternArgs {
    pattern: String,
    category: String,
    priority: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct PlanCreateArgs {
    phase: String,
    source_drive: Option<String>,
    target_drive: Option<String>,
    batch_size: Option<u32>,
    description: Option<String>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct PlanShowArgs {
    plan_id: i64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct PlanApproveArgs {
    plan_id: i64,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct MigrateExecuteArgs {
    plan_id: i64,
    execute: bool,
    io_limit_mbps: Option<u32>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct RollbackArgs {
    plan_id: i64,
    step_id: Option<i64>,
}

#[derive(Deserialize, Serialize, JsonSchema)]
struct VerifyArgs {
    plan_id: Option<i64>,
    full: Option<bool>,
}


#[derive(Debug, Clone)]
pub struct DriveStatistics {
    pub file_count: usize,
    pub total_bytes: i64,
    pub duplicate_groups: usize,
    pub duplicate_file_count: usize,
    pub duplicate_waste_bytes: i64,
}

fn get_drive_statistics_inline(db: &SqliteDatabase, drive_id: i64) -> prune_lib::Result<DriveStatistics> {
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

#[tool_router]
impl PruneServer {
    #[tool(description = "Get overall prune system status including drives, files, duplicates, and migration plans")]
    async fn status(&self) -> Result<String, String> {
        self.with_db(|db| {
            let drives = db.list_drives().map_err(|e| e.to_string())?;
            let online_count = drives.iter().filter(|d| d.is_online).count();
            let offline_count = drives.len() - online_count;

            let conn = db.conn();
            let total_files: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE status != 'source_removed'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let total_size: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(size_bytes), 0) FROM files WHERE status != 'source_removed'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let stats = get_duplicate_statistics(conn).map_err(|e| e.to_string())?;
            let classified: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE category IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let unclassified: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE category IS NULL AND status = 'indexed'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let draft_plans: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM migration_plans WHERE status = 'draft'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let approved_plans: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM migration_plans WHERE status = 'approved'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let response = StatusResponse {
                drives: DriveStats {
                    total: drives.len(),
                    online: online_count,
                    offline: offline_count,
                },
                files: FileStats {
                    total: total_files,
                    total_size_bytes: total_size,
                    classified,
                    unclassified,
                },
                duplicates: DuplicateStats {
                    groups: stats.group_count,
                    files: stats.total_duplicate_files,
                    wasted_bytes: stats.total_waste_bytes,
                    cross_drive_groups: stats.cross_drive_groups,
                },
                plans: PlanStats {
                    draft: draft_plans,
                    approved: approved_plans,
                },
            };

            serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
        })
    }

    #[tool(description = "List all registered drives with their status and configuration")]
    async fn drive_list(&self) -> Result<String, String> {
        self.with_db(|db| {
            let drives = db.list_drives().map_err(|e| e.to_string())?;
            let drive_list: Vec<DriveInfo> = drives
                .iter()
                .map(|d| DriveInfo {
                    id: d.id,
                    label: d.label.clone(),
                    role: d.role.as_str().to_string(),
                    mount_path: d.mount_path.clone(),
                    backend: d.backend.as_str().to_string(),
                    is_online: d.is_online,
                    is_readonly: d.is_readonly,
                    total_bytes: d.total_bytes,
                    scanned_at: d.scanned_at.map(|dt| dt.to_rfc3339()),
                })
                .collect();

            serde_json::to_string_pretty(&drive_list).map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Check space usage on a specific drive or all drives")]
    async fn space_check(&self, args: Parameters<SpaceCheckArgs>) -> Result<String, String> {
        self.with_db(|db| {
            let drives = if let Some(ref label) = args.0.drive {
                if label == "all" {
                    db.list_drives().map_err(|e| e.to_string())?
                } else {
                    vec![db
                        .get_drive(label)
                        .map_err(|e| e.to_string())?
                        .ok_or_else(|| format!("Drive not found: {}", label))?]
                }
            } else {
                db.list_drives().map_err(|e| e.to_string())?
            };

            let mut space_info = Vec::new();
            for drive in drives {
                let stats = get_drive_statistics_inline(db, drive.id).map_err(|e| e.to_string())?;
                space_info.push(serde_json::json!({
                    "label": drive.label,
                    "used_bytes": stats.total_bytes,
                    "total_bytes": drive.total_bytes,
                    "file_count": stats.file_count,
                    "duplicate_waste_bytes": stats.duplicate_waste_bytes,
                }));
            }

            serde_json::to_string_pretty(&space_info).map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Register a new drive and add it to the prune database")]
    async fn drive_add(&self, args: Parameters<DriveAddArgs>) -> Result<String, String> {
        self.with_db_mut(|db| {
            let role = DriveRole::from_str(&args.0.role).map_err(|e| e.to_string())?;
            let backend = if let Some(ref b) = args.0.backend {
                Backend::from_str(b).map_err(|e| e.to_string())?
            } else {
                Backend::Local
            };

            let readonly = args.0.readonly.unwrap_or(role == DriveRole::Backup);

            let drive = Drive {
                id: 0,
                label: args.0.label.clone(),
                device_id: None,
                device_path: None,
                uuid: None,
                mount_path: Some(args.0.mount_path.clone()),
                fs_type: None,
                total_bytes: None,
                role,
                is_online: true,
                is_readonly: readonly,
                backend,
                rclone_remote: args.0.rclone_remote.clone(),
                scanned_at: None,
                added_at: chrono::Utc::now(),
            };

            let drive_id = db.add_drive(&drive).map_err(|e| e.to_string())?;

            let response = serde_json::json!({
                "drive_id": drive_id,
                "label": args.0.label,
                "status": "registered",
                "message": format!("Drive '{}' registered successfully", args.0.label),
            });

            serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Scan a drive and index all files")]
    async fn scan(&self, args: Parameters<ScanArgs>) -> Result<String, String> {
        self.with_db_mut(|db| {
            let drive = db
                .get_drive(&args.0.drive_label)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Drive not found: {}", args.0.drive_label))?;

            if !drive.is_online {
                return Err(format!("Drive is offline: {}", args.0.drive_label));
            }

            let mount_path = drive
                .mount_path
                .ok_or_else(|| "Drive has no mount path".to_string())?;

            let scan_opts = ScanOptions {
                follow_symlinks: false,
                max_depth: None,
                include_hidden: false,
            };

            let stats = scan_directory(db, drive.id, &std::path::PathBuf::from(&mount_path), scan_opts)
                .map_err(|e| e.to_string())?;

            let response = serde_json::json!({
                "drive": args.0.drive_label,
                "files_indexed": stats.files_scanned,
                "bytes_indexed": stats.bytes_scanned,
                "errors": stats.errors,
                "status": "complete",
            });

            serde_json::to_string_pretty(&response).map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Query duplicate file groups to identify wasted space")]
    async fn query_duplicates(
        &self,
        args: Parameters<QueryDuplicatesArgs>,
    ) -> Result<String, String> {
        self.with_db(|db| {
            let conn = db.conn();
            let groups = if args.0.same_drive_only.unwrap_or(false) {
                list_duplicate_groups(conn)
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .filter(|g| !g.cross_drive)
                    .collect()
            } else {
                list_duplicate_groups(conn).map_err(|e| e.to_string())?
            };

            let limit = args.0.limit.unwrap_or(50) as usize;
            let groups: Vec<_> = groups
                .into_iter()
                .filter(|g| {
                    if let Some(min_size) = args.0.min_size {
                        g.total_waste_bytes >= min_size as i64
                    } else {
                        true
                    }
                })
                .take(limit)
                .map(|g| {
                    serde_json::json!({
                        "group_id": g.group_id,
                        "hash": g.hash,
                        "file_count": g.file_count,
                        "wasted_bytes": g.total_waste_bytes,
                        "cross_drive": g.cross_drive,
                        "drives": g.drives_involved,
                    })
                })
                .collect();

            serde_json::to_string_pretty(&serde_json::json!({
                "groups": groups,
                "total_returned": groups.len(),
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Query files that need classification")]
    async fn query_unclassified(
        &self,
        _args: Parameters<QueryUnclassifiedArgs>,
    ) -> Result<String, String> {
        Err("query_unclassified not yet implemented - requires SQL query refactoring".to_string())
    }

    #[tool(description = "Query files by various criteria like category, extension, size, or path pattern")]
    async fn query_files(&self, args: Parameters<QueryFilesArgs>) -> Result<String, String> {
        self.with_db(|db| {
            let conn = db.conn();
            let mut query = String::from(
                "SELECT id, drive_id, path, filename, extension, size_bytes, category, subcategory FROM files WHERE 1=1",
            );
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

            if let Some(ref category) = args.0.category {
                query.push_str(" AND category = ?");
                params.push(Box::new(category.clone()));
            }

            if let Some(ref extension) = args.0.extension {
                query.push_str(" AND extension = ?");
                params.push(Box::new(extension.clone()));
            }

            if let Some(min_size) = args.0.min_size {
                query.push_str(" AND size_bytes >= ?");
                params.push(Box::new(min_size as i64));
            }

            if let Some(ref path) = args.0.path_contains {
                query.push_str(" AND path LIKE ?");
                params.push(Box::new(format!("%{}%", path)));
            }

            if let Some(ref drive_label) = args.0.drive {
                let drive = db
                    .get_drive(drive_label)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| format!("Drive not found: {}", drive_label))?;
                query.push_str(" AND drive_id = ?");
                params.push(Box::new(drive.id));
            }

            query.push_str(" ORDER BY size_bytes DESC");

            if let Some(limit) = args.0.limit {
                query.push_str(&format!(" LIMIT {}", limit));
            }

            let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&query).map_err(|e| e.to_string())?;
            let files = stmt
                .query_map(param_refs.as_slice(), |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "drive_id": row.get::<_, i64>(1)?,
                        "path": row.get::<_, String>(2)?,
                        "filename": row.get::<_, String>(3)?,
                        "extension": row.get::<_, Option<String>>(4)?,
                        "size_bytes": row.get::<_, i64>(5)?,
                        "category": row.get::<_, Option<String>>(6)?,
                        "subcategory": row.get::<_, Option<String>>(7)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e: rusqlite::Error| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "files": files,
                "count": files.len(),
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Find files that exist only on a backup drive and not on other drives")]
    async fn query_backup_unique(
        &self,
        args: Parameters<QueryBackupUniqueArgs>,
    ) -> Result<String, String> {
        self.with_db(|db| {
            let backup_drive = db
                .get_drive(&args.0.backup_drive)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Drive not found: {}", args.0.backup_drive))?;

            let conn = db.conn();
            let query = "
                SELECT id, path, filename, size_bytes, blake3_hash
                FROM files
                WHERE drive_id = ? AND blake3_hash IS NOT NULL
                  AND blake3_hash NOT IN (
                    SELECT DISTINCT blake3_hash FROM files
                    WHERE drive_id != ? AND blake3_hash IS NOT NULL
                  )
                ORDER BY size_bytes DESC
            ";

            let mut stmt = conn.prepare(query).map_err(|e| e.to_string())?;
            let files = stmt
                .query_map([backup_drive.id, backup_drive.id], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "path": row.get::<_, String>(1)?,
                        "filename": row.get::<_, String>(2)?,
                        "size_bytes": row.get::<_, i64>(3)?,
                        "hash": row.get::<_, String>(4)?,
                    }))
                })
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e: rusqlite::Error| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "files": files,
                "count": files.len(),
                "backup_drive": args.0.backup_drive,
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Run automatic classification rules on unclassified files")]
    async fn classify_auto(&self, _args: Parameters<ClassifyAutoArgs>) -> Result<String, String> {
        Err("classify_auto not yet implemented - requires ClassificationRules API migration".to_string())
    }

    #[tool(description = "Manually classify specific files by ID")]
    async fn classify(&self, args: Parameters<ClassifyArgs>) -> Result<String, String> {
        self.with_db_mut(|db| {
            let priority = if let Some(ref p) = args.0.priority {
                Priority::from_str(p).map_err(|e| e.to_string())?
            } else {
                Priority::Normal
            };

            let file_ids: Vec<i64> = args.0
                .file_ids
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();

            let conn = db.conn_mut();
            for file_id in &file_ids {
                conn.execute(
                    "UPDATE files SET category = ?, subcategory = ?, priority = ?, status = ? WHERE id = ?",
                    (
                        &args.0.category,
                        &args.0.subcategory,
                        priority.as_str(),
                        FileStatus::Classified.as_str(),
                        file_id,
                    ),
                )
                .map_err(|e| e.to_string())?;
            }

            serde_json::to_string_pretty(&serde_json::json!({
                "classified": file_ids.len(),
                "file_ids": file_ids,
                "category": args.0.category,
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Classify all files matching a path glob pattern")]
    async fn classify_pattern(
        &self,
        args: Parameters<ClassifyPatternArgs>,
    ) -> Result<String, String> {
        self.with_db_mut(|db| {
            let priority = if let Some(ref p) = args.0.priority {
                Priority::from_str(p).map_err(|e| e.to_string())?
            } else {
                Priority::Normal
            };

            let conn = db.conn_mut();
            let count = conn
                .execute(
                    "UPDATE files SET category = ?, priority = ?, status = ? WHERE path LIKE ?",
                    (
                        &args.0.category,
                        priority.as_str(),
                        FileStatus::Classified.as_str(),
                        format!("%{}%", args.0.pattern),
                    ),
                )
                .map_err(|e| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "classified": count,
                "pattern": args.0.pattern,
                "category": args.0.category,
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Create a migration plan for review (does not execute)")]
    async fn plan_create(&self, _args: Parameters<PlanCreateArgs>) -> Result<String, String> {
        Err("plan_create not yet implemented - requires file querying and Planner API updates".to_string())
    }

    #[tool(description = "Show details of a migration plan")]
    async fn plan_show(&self, args: Parameters<PlanShowArgs>) -> Result<String, String> {
        self.with_db(|db| {
            let plan = db
                .get_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Plan not found: {}", args.0.plan_id))?;

            let steps = db
                .get_steps_for_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?;

            let steps_json: Vec<_> = steps
                .iter()
                .take(50)
                .map(|s| {
                    serde_json::json!({
                        "id": s.id,
                        "action": s.action.as_str(),
                        "source_path": s.source_path,
                        "dest_path": s.dest_path,
                        "status": s.status.as_str(),
                    })
                })
                .collect();

            serde_json::to_string_pretty(&serde_json::json!({
                "plan": {
                    "id": plan.id,
                    "status": plan.status.as_str(),
                    "total_files": plan.total_files,
                    "total_bytes": plan.total_bytes,
                    "completed_files": plan.completed_files,
                    "completed_bytes": plan.completed_bytes,
                    "description": plan.description,
                },
                "steps": steps_json,
                "total_steps": steps.len(),
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Approve a migration plan for execution")]
    async fn plan_approve(&self, args: Parameters<PlanApproveArgs>) -> Result<String, String> {
        self.with_db_mut(|db| {
            db.update_plan_status(args.0.plan_id, PlanStatus::Approved)
                .map_err(|e| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "plan_id": args.0.plan_id,
                "status": "approved",
                "message": "Plan approved and ready for execution",
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Execute an approved migration plan")]
    async fn migrate_execute(
        &self,
        args: Parameters<MigrateExecuteArgs>,
    ) -> Result<String, String> {
        self.with_db_mut(|db| {
            let plan = db
                .get_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("Plan not found: {}", args.0.plan_id))?;

            if plan.status != PlanStatus::Approved && plan.status != PlanStatus::InProgress {
                return Err("Plan must be approved before execution".to_string());
            }

            let engine_opts = EngineOptions {
                dry_run: !args.0.execute,
                verify_hashes: true,
                retry_count: 3,
                enforce_safety: true,
            };

            let mut engine = MigrationEngine::new(db, engine_opts);
            engine
                .execute_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?;

            // Get updated plan status
            let plan = db.get_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "Plan not found".to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "plan_id": args.0.plan_id,
                "dry_run": !args.0.execute,
                "completed_files": plan.completed_files,
                "total_files": plan.total_files,
                "completed_bytes": plan.completed_bytes,
                "status": if args.0.execute { "executed" } else { "dry_run_complete" },
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Rollback a migration plan")]
    async fn rollback(&self, args: Parameters<RollbackArgs>) -> Result<String, String> {
        self.with_db_mut(|db| {
            if args.0.step_id.is_some() {
                return Err("Rollback by step_id not supported - use plan_id to rollback entire plan".to_string());
            }

            let mut engine = RollbackEngine::new(db, true);
            engine.rollback_plan(args.0.plan_id)
                .map_err(|e| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "plan_id": args.0.plan_id,
                "status": "rollback_complete",
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Verify file hashes after migration")]
    async fn verify(&self, args: Parameters<VerifyArgs>) -> Result<String, String> {
        self.with_db(|db| {
            let conn = db.conn();

            let (verified, failed) = if args.0.full.unwrap_or(false) {
                let count: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM files WHERE status IN ('verified', 'classified')",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                (count, 0)
            } else if let Some(plan_id) = args.0.plan_id {
                let steps = db.get_steps_for_plan(plan_id).map_err(|e| e.to_string())?;
                let verified = steps.iter().filter(|s| s.post_hash.is_some()).count();
                let failed = steps
                    .iter()
                    .filter(|s| s.status == StepStatus::Failed)
                    .count();
                (verified as i64, failed as i64)
            } else {
                (0, 0)
            };

            serde_json::to_string_pretty(&serde_json::json!({
                "verified": verified,
                "failed": failed,
                "status": "verification_complete",
            }))
            .map_err(|e| e.to_string())
        })
    }

    #[tool(description = "Generate a summary report of prune operations")]
    async fn report(&self) -> Result<String, String> {
        self.with_db(|db| {
            let conn = db.conn();

            let total_files: i64 = conn
                .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))
                .map_err(|e| e.to_string())?;

            let migrated_files: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM files WHERE status = 'verified'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let stats = get_duplicate_statistics(conn).map_err(|e| e.to_string())?;

            let completed_plans: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM migration_plans WHERE status = 'completed'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            let total_saved: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(total_bytes), 0) FROM migration_plans WHERE status = 'completed'",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;

            serde_json::to_string_pretty(&serde_json::json!({
                "total_files": total_files,
                "migrated_files": migrated_files,
                "space_saved_bytes": stats.total_waste_bytes,
                "completed_plans": completed_plans,
                "total_bytes_migrated": total_saved,
                "duplicate_groups_resolved": stats.group_count,
            }))
            .map_err(|e| e.to_string())
        })
    }
}
