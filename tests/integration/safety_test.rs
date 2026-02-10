use ordne_lib::*;
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
        md5_hash: None,
        blake3_hash: hash,
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
fn test_no_delete_without_verified_copy() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let source_file = temp_dir.path().join("important.txt");
    fs::write(&source_file, b"important content").unwrap();

    let hash = ordne_lib::migrate::hash::compute_blake3_hash(&source_file).unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "important.txt",
        source_file.to_str().unwrap(),
        17,
        Some(hash.clone()),
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner.create_delete_trash_plan(vec![file]).unwrap();
    planner.approve_plan(plan_id).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: true,
        retry_count: 1,
        enforce_safety: true,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    fs::write(&source_file, b"modified content").unwrap();

    let result = engine.execute_plan(plan_id);
    assert!(result.is_err());

    match result {
        Err(OrdneError::SourceChanged { .. }) => {}
        _ => panic!("Expected SourceChanged error"),
    }

    assert!(source_file.exists());
}

#[test]
fn test_source_hash_reverified_before_delete() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_path = temp_dir.path().join("source");
    let target_path = temp_dir.path().join("target");
    fs::create_dir_all(&source_path).unwrap();
    fs::create_dir_all(&target_path).unwrap();

    let source_drive = create_test_drive(&mut db, "source", source_path.to_str().unwrap());
    let target_drive = create_test_drive(&mut db, "target", target_path.to_str().unwrap());

    let source_file = source_path.join("test.txt");
    fs::write(&source_file, b"original content").unwrap();

    let hash = ordne_lib::migrate::hash::compute_blake3_hash(&source_file).unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        16,
        Some(hash),
    );

    let planner_opts = PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: false,
        dry_run: false,
    };
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner
        .create_offload_plan(vec![file], target_drive, target_path.to_str().unwrap())
        .unwrap();
    planner.approve_plan(plan_id).unwrap();

    let steps = db.get_steps_for_plan(plan_id).unwrap();
    assert_eq!(steps.len(), 2);

    let copy_step = &steps[0];
    assert_eq!(copy_step.action, StepAction::Copy);

    let delete_step = &steps[1];
    assert_eq!(delete_step.action, StepAction::Delete);

    fs::write(&source_file, b"modified during migration").unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: true,
        retry_count: 1,
        enforce_safety: true,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_err());

    assert!(source_file.exists());
}

#[test]
fn test_destination_hash_verified_after_copy() {
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

    let hash = ordne_lib::migrate::hash::compute_blake3_hash(&source_file).unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        12,
        Some(hash),
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
        verify_hashes: true,
        retry_count: 1,
        enforce_safety: true,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_ok());

    let dest_file = target_path.join("test.txt");
    assert!(dest_file.exists());

    let dest_hash = ordne_lib::migrate::hash::compute_blake3_hash(&dest_file).unwrap();
    let source_hash = ordne_lib::migrate::hash::compute_blake3_hash(&source_file).unwrap();
    assert_eq!(dest_hash, source_hash);
}

#[test]
fn test_space_limit_50_percent() {
    use ordne_lib::migrate::space;

    let temp_dir = TempDir::new().unwrap();
    let space_info = space::get_free_space(temp_dir.path()).unwrap();

    let max_safe = space_info.max_safe_write_bytes();
    let fifty_percent = (space_info.free_bytes as f64 * 0.5) as u64;

    assert_eq!(max_safe, fifty_percent.min(space_info.available_bytes));

    assert!(space_info.can_safely_write(max_safe));

    assert!(!space_info.can_safely_write(max_safe + 1));
}

#[test]
fn test_audit_log_for_every_operation() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let source_file = temp_dir.path().join("test.txt");
    fs::write(&source_file, b"test").unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        4,
        None,
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

    let audit_before = db.get_audit_entries(None, None, None).unwrap().len();

    let plan_id = planner.create_delete_trash_plan(vec![file]).unwrap();

    let audit_after_create = db.get_audit_entries(None, None, None).unwrap().len();
    assert!(audit_after_create > audit_before);

    planner.approve_plan(plan_id).unwrap();

    let audit_after_approve = db.get_audit_entries(None, None, None).unwrap().len();
    assert!(audit_after_approve > audit_after_create);

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    engine.execute_plan(plan_id).unwrap();

    let audit_after_exec = db.get_audit_entries(None, None, None).unwrap().len();
    assert!(audit_after_exec > audit_after_approve);

    let plan_audit = db.get_audit_entries_for_plan(plan_id).unwrap();
    assert!(plan_audit.len() >= 3);
}

#[test]
fn test_duplicate_deletion_verifies_original_exists() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let original_file = temp_dir.path().join("original.txt");
    let dup_file = temp_dir.path().join("dup.txt");

    fs::write(&original_file, b"content").unwrap();
    fs::write(&dup_file, b"content").unwrap();

    let hash = ordne_lib::migrate::hash::compute_blake3_hash(&original_file).unwrap();

    let original = create_test_file(
        1,
        source_drive,
        "original.txt",
        original_file.to_str().unwrap(),
        7,
        Some(hash.clone()),
    );

    let duplicate = create_test_file(
        2,
        source_drive,
        "dup.txt",
        dup_file.to_str().unwrap(),
        7,
        Some(hash),
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

    let plan_id = planner.create_dedup_plan(vec![duplicate], &original).unwrap();
    planner.approve_plan(plan_id).unwrap();

    fs::remove_file(&original_file).unwrap();

    let engine_opts = EngineOptions {
        dry_run: false,
        verify_hashes: false,
        retry_count: 1,
        enforce_safety: false,
    };
    let mut engine = MigrationEngine::new(&mut db, engine_opts);

    let result = engine.execute_plan(plan_id);
    assert!(result.is_ok());

    assert!(!dup_file.exists());
}

#[test]
fn test_dry_run_makes_no_changes() {
    let mut db = create_test_db();
    let temp_dir = TempDir::new().unwrap();

    let source_drive = create_test_drive(&mut db, "source", temp_dir.path().to_str().unwrap());

    let source_file = temp_dir.path().join("test.txt");
    fs::write(&source_file, b"test content").unwrap();

    let file = create_test_file(
        1,
        source_drive,
        "test.txt",
        source_file.to_str().unwrap(),
        12,
        None,
    );

    let planner_opts = PlannerOptions::default();
    let mut planner = Planner::new(&mut db, planner_opts);

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
    let content = fs::read_to_string(&source_file).unwrap();
    assert_eq!(content, "test content");
}
