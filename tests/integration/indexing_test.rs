use ordne_lib::{
    Database, Drive, DriveRole, Backend, FileStatus, SqliteDatabase,
    discover_device, scan_directory, hash_file_md5, hash_file_blake3,
};
use ordne_lib::index::{RmlintParser, ScanOptions};
use ordne_lib::db::{duplicates, files};
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;
use chrono::Utc;

fn create_test_db() -> SqliteDatabase {
    let mut db = SqliteDatabase::open_in_memory().unwrap();
    db.initialize().unwrap();
    db
}

fn setup_test_fixture() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base = temp_dir.path();

    fs::create_dir(base.join("dir1")).unwrap();
    fs::create_dir(base.join("dir2")).unwrap();
    fs::create_dir(base.join(".hidden")).unwrap();

    File::create(base.join("file1.txt"))
        .unwrap()
        .write_all(b"duplicate content")
        .unwrap();

    File::create(base.join("dir1/file2.txt"))
        .unwrap()
        .write_all(b"duplicate content")
        .unwrap();

    File::create(base.join("dir1/file3.txt"))
        .unwrap()
        .write_all(b"unique content 1")
        .unwrap();

    File::create(base.join("dir2/file4.txt"))
        .unwrap()
        .write_all(b"duplicate content")
        .unwrap();

    File::create(base.join("dir2/unique.dat"))
        .unwrap()
        .write_all(b"unique content 2")
        .unwrap();

    File::create(base.join(".hidden/hidden.txt"))
        .unwrap()
        .write_all(b"hidden file")
        .unwrap();

    temp_dir
}

#[test]
fn test_end_to_end_scan_and_index() {
    let temp_dir = setup_test_fixture();
    let mut db = create_test_db();

    let drive = Drive {
        id: 0,
        label: "test_drive".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: Some(temp_dir.path().to_string_lossy().to_string()),
        fs_type: Some("ext4".to_string()),
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };

    let drive_id = db.add_drive(&drive).unwrap();
    assert!(drive_id > 0);

    let stats = scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

    assert_eq!(stats.files_scanned, 5);
    assert!(stats.bytes_scanned > 0);
    assert_eq!(stats.dirs_scanned, 4);

    let all_files = files::list_files_by_drive(db.conn(), drive_id).unwrap();
    assert_eq!(all_files.len(), 5);

    for file in &all_files {
        assert_eq!(file.status, FileStatus::Indexed);
        assert_eq!(file.drive_id, drive_id);
    }
}

#[test]
fn test_scan_with_hidden_files() {
    let temp_dir = setup_test_fixture();
    let mut db = create_test_db();

    let drive = Drive {
        id: 0,
        label: "test".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };

    let drive_id = db.add_drive(&drive).unwrap();

    let options = ScanOptions {
        include_hidden: true,
        ..Default::default()
    };

    let stats = scan_directory(&mut db, drive_id, temp_dir.path(), options).unwrap();

    assert_eq!(stats.files_scanned, 6);
}

#[test]
fn test_duplicate_detection_with_hashing() {
    let temp_dir = setup_test_fixture();
    let mut db = create_test_db();

    let drive = Drive {
        id: 0,
        label: "test".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };

    let drive_id = db.add_drive(&drive).unwrap();

    scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

    let all_files = files::list_files_by_drive(db.conn(), drive_id).unwrap();

    let mut hashes = std::collections::HashMap::new();
    for file in &all_files {
        let hash = hash_file_md5(&file.abs_path).unwrap();
        files::update_file_hash(db.conn(), file.id, Some(&hash), None).unwrap();

        hashes.entry(hash.clone()).or_insert_with(Vec::new).push(file.id);
    }

    let duplicate_groups: Vec<_> = hashes
        .iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect();

    assert_eq!(duplicate_groups.len(), 1);

    let (dup_hash, dup_ids) = duplicate_groups[0];
    assert_eq!(dup_ids.len(), 3);

    let group_id = duplicates::create_duplicate_group(
        db.conn(),
        dup_hash,
        dup_ids.len() as i32,
        0,
        Some(dup_ids[0]),
        &[drive_id],
        false,
    )
    .unwrap();

    duplicates::assign_files_to_duplicate_group(
        db.conn(),
        dup_ids,
        group_id,
        Some(dup_ids[0]),
    )
    .unwrap();

    let dup_files = files::list_files_by_hash(db.conn(), dup_hash).unwrap();
    assert_eq!(dup_files.len(), 3);

    let originals: Vec<_> = dup_files.iter().filter(|f| f.is_original).collect();
    assert_eq!(originals.len(), 1);
}

#[test]
fn test_cross_drive_duplicate_detection() {
    let temp_dir1 = TempDir::new().unwrap();
    let temp_dir2 = TempDir::new().unwrap();

    File::create(temp_dir1.path().join("file.txt"))
        .unwrap()
        .write_all(b"shared content")
        .unwrap();

    File::create(temp_dir2.path().join("copy.txt"))
        .unwrap()
        .write_all(b"shared content")
        .unwrap();

    let mut db = create_test_db();

    let drive1 = Drive {
        id: 0,
        label: "drive1".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };
    let drive1_id = db.add_drive(&drive1).unwrap();

    let drive2 = Drive {
        id: 0,
        label: "drive2".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Target,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };
    let drive2_id = db.add_drive(&drive2).unwrap();

    scan_directory(&mut db, drive1_id, temp_dir1.path(), ScanOptions::default()).unwrap();
    scan_directory(&mut db, drive2_id, temp_dir2.path(), ScanOptions::default()).unwrap();

    let files1 = files::list_files_by_drive(db.conn(), drive1_id).unwrap();
    let files2 = files::list_files_by_drive(db.conn(), drive2_id).unwrap();

    let hash1 = hash_file_md5(&files1[0].abs_path).unwrap();
    let hash2 = hash_file_md5(&files2[0].abs_path).unwrap();

    assert_eq!(hash1, hash2);

    files::update_file_hash(db.conn(), files1[0].id, Some(&hash1), None).unwrap();
    files::update_file_hash(db.conn(), files2[0].id, Some(&hash2), None).unwrap();

    let group_id = duplicates::create_duplicate_group(
        db.conn(),
        &hash1,
        2,
        files1[0].size_bytes,
        Some(files1[0].id),
        &[drive1_id, drive2_id],
        true,
    )
    .unwrap();

    let dup_group = duplicates::get_duplicate_group(db.conn(), group_id).unwrap().unwrap();
    assert!(dup_group.cross_drive);
    assert_eq!(dup_group.drives_involved.len(), 2);
}

#[test]
fn test_rmlint_json_parsing() {
    let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/tmp/file2.txt","size":1024,"checksum":"abc123","is_original":false}
{"type":"duplicate_file","path":"/tmp/file3.txt","size":2048,"checksum":"def456","is_original":true}
{"type":"duplicate_file","path":"/tmp/file4.txt","size":2048,"checksum":"def456","is_original":false}
{"type":"emptyfile","path":"/tmp/empty.txt","size":0}
"#;

    let mut parser = RmlintParser::new();
    parser.parse_string(json).unwrap();

    assert_eq!(parser.lints().len(), 5);

    let groups = parser.extract_duplicate_groups();
    assert_eq!(groups.len(), 2);

    let stats = parser.statistics();
    assert_eq!(stats.duplicate_files, 4);
    assert_eq!(stats.duplicate_groups, 2);
    assert_eq!(stats.empty_files, 1);
}

#[test]
fn test_hash_verification() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");

    File::create(&file_path)
        .unwrap()
        .write_all(b"test content")
        .unwrap();

    let md5_hash = hash_file_md5(&file_path).unwrap();
    assert_eq!(md5_hash.len(), 32);

    let blake3_hash = hash_file_blake3(&file_path).unwrap();
    assert_eq!(blake3_hash.len(), 64);

    use ordne_lib::index::verify_hash;
    verify_hash(&file_path, &md5_hash).unwrap();
    verify_hash(&file_path, &blake3_hash).unwrap();
}

#[test]
fn test_rescan_updates_existing_files() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("file.txt");

    File::create(&file_path)
        .unwrap()
        .write_all(b"initial content")
        .unwrap();

    let mut db = create_test_db();

    let drive = Drive {
        id: 0,
        label: "test".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };
    let drive_id = db.add_drive(&drive).unwrap();

    scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();
    let files_before = files::list_files_by_drive(db.conn(), drive_id).unwrap();
    assert_eq!(files_before.len(), 1);

    File::create(&file_path)
        .unwrap()
        .write_all(b"modified content with more bytes")
        .unwrap();

    scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();
    let files_after = files::list_files_by_drive(db.conn(), drive_id).unwrap();
    assert_eq!(files_after.len(), 1);

    assert!(files_after[0].size_bytes > files_before[0].size_bytes);
}

#[test]
fn test_drive_statistics() {
    let temp_dir = setup_test_fixture();
    let mut db = create_test_db();

    let drive = Drive {
        id: 0,
        label: "test".to_string(),
        device_id: None,
        device_path: None,
        uuid: None,
        mount_path: None,
        fs_type: None,
        total_bytes: None,
        role: DriveRole::Source,
        is_online: true,
        is_readonly: false,
        backend: Backend::Local,
        rclone_remote: None,
        scanned_at: None,
        added_at: Utc::now(),
    };
    let drive_id = db.add_drive(&drive).unwrap();

    scan_directory(&mut db, drive_id, temp_dir.path(), ScanOptions::default()).unwrap();

    let stats = files::get_drive_statistics(db.conn(), drive_id).unwrap();
    assert_eq!(stats.file_count, 5);
    assert!(stats.total_bytes > 0);
}
