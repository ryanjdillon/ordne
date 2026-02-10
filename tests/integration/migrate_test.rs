use prune_lib::*;
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
        added_at: chrono::Utc::now(),
    };

    db.add_drive(&drive).unwrap()
}

fn create_test_file(
    id: i64,
    drive_id: i64,
    path: &str,
    abs_path: &str,
    size: i64,
    hash: Option<String>,
) -> File {
    File {
        id,
        drive_id,
        path: path.to_string(),
        abs_path: abs_path.to_string(),
        filename: path.to_string(),
        extension: Some("txt".to_string()),
        size_bytes: size,
        md5_hash: hash.clone(),
        blake3_hash: None,
        created_at: Some(chrono::Utc::now()),
        modified_at: Some(chrono::Utc::now()),
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
        indexed_at: chrono::Utc::now(),
    }
}

#[test]
fn test_full_migration_cycle() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_path = temp_dir.path().join("source");
    let target_path = temp_dir.path().join("target");
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(&target_path).unwrap();

    let source_drive = create_test_drive(&mut db, "source", source_path.to_str().unwrap());
    let target_drive = create_test_drive(&mut db, "target", target_path.to_str().unwrap());

    let source_file = source_path.join("test.txt");
    fs::write(&source_file, b"test content").unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        12,
        None,
    );

    let planner_opts = PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: false,
        dry_run: false,
    };
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner
        .create_migrate_plan(vec![file], target_drive, target_path.to_str().unwrap())
        .unwrap();

    planner.approve_plan(plan_id).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_ok(), "Migration failed: {:?}", result);

    let plan = db.get_plan(plan_id).unwrap().unwrap();
    assert_eq!(plan.status, PlanStatus::Completed);
    assert_eq!(plan.completed_files, 1);

    let dest_file = target_path.join("test.txt");
    assert!(dest_file.exists());
    let content = fs::read_to_string(&dest_file).unwrap();
    assert_eq!(content, "test content");
}

#[test]
fn test_delete_trash_scenario() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let trash_file = temp_dir.path().join("trash.txt");
    fs::write(&trash_file, b"trash content").unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "trash.txt",
        trash_file.to_str().unwrap(),
        13,
        None,
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner.create_delete_trash_plan(vec![file]).unwrap();
    planner.approve_plan(plan_id).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_ok());

    assert!(!trash_file.exists());
}

#[test]
fn test_dedup_scenario() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let original_file = temp_dir.path().join("original.txt");
    let dup1_file = temp_dir.path().join("dup1.txt");
    let dup2_file = temp_dir.path().join("dup2.txt");

    fs::write(&original_file, b"content").unwrap();
    fs::write(&dup1_file, b"content").unwrap();
    fs::write(&dup2_file, b"content").unwrap();

    let original = create_test_file(
        1,
        source_drive,
        "original.txt",
        original_file.to_str().unwrap(),
        7,
        Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
    );

    let dup1 = create_test_file(
        2,
        source_drive,
        "dup1.txt",
        dup1_file.to_str().unwrap(),
        7,
        Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
    );

    let dup2 = create_test_file(
        3,
        source_drive,
        "dup2.txt",
        dup2_file.to_str().unwrap(),
        7,
        Some("d41d8cd98f00b204e9800998ecf8427e".to_string()),
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner
        .create_dedup_plan(vec![dup1, dup2], &original)
        .unwrap();
    planner.approve_plan(plan_id).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_ok());

    assert!(original_file.exists());
    assert!(!dup1_file.exists());
    assert!(!dup2_file.exists());
}

#[test]
fn test_rollback_scenario() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_path = temp_dir.path().join("source");
    let target_path = temp_dir.path().join("target");
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(&target_path).unwrap();

    let source_drive = create_test_drive(&mut db, "source", source_path.to_str().unwrap());
    let target_drive = create_test_drive(&mut db, "target", target_path.to_str().unwrap());

    let source_file = source_path.join("test.txt");
    fs::write(&source_file, b"test content").unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        12,
        None,
    );

    let planner_opts = PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: false,
        dry_run: false,
    };
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner
        .create_migrate_plan(vec![file], target_drive, target_path.to_str().unwrap())
        .unwrap();
    planner.approve_plan(plan_id).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    engine.execute_plan(plan_id).unwrap();

    let dest_file = target_path.join("test.txt");
    assert!(dest_file.exists());

    let mut rollback = RollbackEngine::new(&mut db, false);
    let can_rollback = rollback.can_rollback(plan_id).unwrap();
    assert!(can_rollback);

    let result = rollback.rollback_plan(plan_id);
    assert!(result.is_ok());

    assert!(!dest_file.exists());
}

#[test]
fn test_insufficient_space_handling() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());
    let target_drive = create_test_drive(&mut db, "target", temp_dir.path().to_str().unwrap());

    let huge_file = create_test_file(
        1,
        source_drive,
        "huge.txt",
        "/fake/path",
        1_000_000_000_000_000,
        None,
    );

    let planner_opts = PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: true,
        dry_run: false,
    };
    let mut planner = Planner::new(&mut db, planner_opts);

    let result = planner.create_migrate_plan(
        vec![huge_file],
        target_drive,
        temp_dir.path().to_str().unwrap(),
    );

    assert!(result.is_err());
    match result {
        Err(PruneError::InsufficientSpace { .. }) => {}
        _ => panic!("Expected InsufficientSpace error"),
    }
}
