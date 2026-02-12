use crate::db::{duplicates, files, SqliteDatabase};
use crate::error::Result;
use crate::index::rmlint::RmlintLintType;
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct RmlintImportOptions {
    pub apply_trash: bool,
    pub clear_existing_duplicates: bool,
}

impl Default for RmlintImportOptions {
    fn default() -> Self {
        Self {
            apply_trash: true,
            clear_existing_duplicates: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct RmlintImportResult {
    pub lints_total: usize,
    pub matched_files: usize,
    pub duplicate_groups_created: usize,
    pub duplicate_files_assigned: usize,
    pub empty_files_marked: usize,
    pub empty_dirs_marked: usize,
    pub bad_links_marked: usize,
    pub skipped_lints: usize,
}

pub fn import_rmlint_output<P: AsRef<Path>>(
    db: &mut SqliteDatabase,
    path: P,
    options: RmlintImportOptions,
) -> Result<RmlintImportResult> {
    let parser = crate::index::parse_rmlint_output(path)?;

    if options.clear_existing_duplicates {
        duplicates::clear_duplicate_assignments(db.conn())?;
        duplicates::clear_duplicate_groups(db.conn())?;
    }

    let mut result = RmlintImportResult {
        lints_total: parser.lints().len(),
        ..Default::default()
    };

    let mut lint_to_file: HashMap<String, i64> = HashMap::new();
    for lint in parser.lints() {
        let abs_path = lint.path.to_string_lossy().to_string();
        if let Some(file) = files::get_file_by_abs_path(db.conn(), &abs_path)? {
            files::update_file_rmlint_type(db.conn(), file.id, lint.lint_type.as_str())?;
            lint_to_file.insert(abs_path, file.id);
            result.matched_files += 1;
        } else {
            result.skipped_lints += 1;
        }
    }

    for group in parser.extract_duplicate_groups() {
        let mut file_ids = Vec::new();
        let mut drive_ids = HashSet::new();
        let mut original_id = None;
        let mut total_waste_bytes = 0i64;

        for lint in &group.files {
            let abs_path = lint.path.to_string_lossy().to_string();
            if let Some(&file_id) = lint_to_file.get(&abs_path) {
                file_ids.push(file_id);

                if lint.is_original {
                    original_id = Some(file_id);
                }

                if let Some(file) = files::get_file(db.conn(), file_id)? {
                    drive_ids.insert(file.drive_id);
                }
            }
        }

        if file_ids.len() < 2 {
            continue;
        }

        if let Some(original_id) = original_id {
            for file_id in &file_ids {
                if *file_id != original_id {
                    if let Some(file) = files::get_file(db.conn(), *file_id)? {
                        total_waste_bytes += file.size_bytes;
                    }
                }
            }
        } else {
            for file_id in &file_ids {
                if let Some(file) = files::get_file(db.conn(), *file_id)? {
                    total_waste_bytes += file.size_bytes;
                }
            }
        }

        let drives: Vec<i64> = drive_ids.into_iter().collect();
        let cross_drive = drives.len() > 1;

        let group_id = duplicates::create_duplicate_group(
            db.conn(),
            &group.hash,
            file_ids.len() as i32,
            total_waste_bytes,
            original_id,
            &drives,
            cross_drive,
        )?;

        duplicates::assign_files_to_duplicate_group(db.conn(), &file_ids, group_id, original_id)?;
        result.duplicate_groups_created += 1;
        result.duplicate_files_assigned += file_ids.len();
    }

    if options.apply_trash {
        for lint in parser.lints() {
            let abs_path = lint.path.to_string_lossy().to_string();
            let Some(&file_id) = lint_to_file.get(&abs_path) else {
                continue;
            };

            match lint.lint_type {
                RmlintLintType::EmptyFile => {
                    files::update_file_as_trash(db.conn(), file_id)?;
                    result.empty_files_marked += 1;
                }
                RmlintLintType::EmptyDir => {
                    files::update_file_as_trash(db.conn(), file_id)?;
                    result.empty_dirs_marked += 1;
                }
                RmlintLintType::BadLink => {
                    files::update_file_as_trash(db.conn(), file_id)?;
                    result.bad_links_marked += 1;
                }
                _ => {}
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::initialize_schema;
    use crate::db::{Backend, Database, Drive, DriveRole, File, FileStatus, Priority};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn setup_db() -> SqliteDatabase {
        let db = SqliteDatabase::open_in_memory().unwrap();
        initialize_schema(db.conn()).unwrap();
        db
    }

    fn insert_drive(db: &mut SqliteDatabase) -> i64 {
        let drive = Drive {
            id: 0,
            label: "drive1".to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: Some("/mnt/drive1".to_string()),
            fs_type: None,
            total_bytes: None,
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

    fn insert_file(db: &mut SqliteDatabase, drive_id: i64, abs_path: &str, size: i64) -> i64 {
        let file = File {
            id: 0,
            drive_id,
            path: abs_path.to_string(),
            abs_path: abs_path.to_string(),
            filename: abs_path.split('/').last().unwrap().to_string(),
            extension: None,
            size_bytes: size,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: None,
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
        };

        db.add_file(&file).unwrap()
    }

    #[test]
    fn test_import_rmlint_output() {
        let mut db = setup_db();
        let drive_id = insert_drive(&mut db);

        let file1_id = insert_file(&mut db, drive_id, "/mnt/drive1/a.txt", 100);
        let _file2_id = insert_file(&mut db, drive_id, "/mnt/drive1/b.txt", 100);
        let empty_id = insert_file(&mut db, drive_id, "/mnt/drive1/empty.txt", 0);

        let json = r#"{"type":"duplicate_file","path":"/mnt/drive1/a.txt","size":100,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/mnt/drive1/b.txt","size":100,"checksum":"abc123","is_original":false}
{"type":"emptyfile","path":"/mnt/drive1/empty.txt","size":0}
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = import_rmlint_output(
            &mut db,
            temp_file.path(),
            RmlintImportOptions {
                apply_trash: true,
                clear_existing_duplicates: false,
            },
        )
        .unwrap();

        assert_eq!(result.duplicate_groups_created, 1);
        assert_eq!(result.duplicate_files_assigned, 2);
        assert_eq!(result.empty_files_marked, 1);

        let file1 = db.get_file(file1_id).unwrap().unwrap();
        assert!(file1.duplicate_group.is_some());

        let empty = db.get_file(empty_id).unwrap().unwrap();
        assert_eq!(empty.category.as_deref(), Some("trash"));
    }

    #[test]
    fn test_import_rmlint_output_replace_duplicates() {
        let mut db = setup_db();
        let drive_id = insert_drive(&mut db);

        let a_id = insert_file(&mut db, drive_id, "/mnt/drive1/a.txt", 100);
        let b_id = insert_file(&mut db, drive_id, "/mnt/drive1/b.txt", 100);
        let c_id = insert_file(&mut db, drive_id, "/mnt/drive1/c.txt", 100);

        db.conn().execute(
            "INSERT INTO duplicate_groups (hash, file_count, total_waste_bytes, drives_involved, cross_drive) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("legacy", 2i64, 100i64, "1", 0),
        ).unwrap();
        let legacy_group = db.conn().last_insert_rowid();

        db.conn()
            .execute(
                "UPDATE files SET duplicate_group = ?1 WHERE id IN (?2, ?3)",
                (legacy_group, a_id, b_id),
            )
            .unwrap();

        let json = r#"{"type":"duplicate_file","path":"/mnt/drive1/b.txt","size":100,"checksum":"newhash","is_original":true}
{"type":"duplicate_file","path":"/mnt/drive1/c.txt","size":100,"checksum":"newhash","is_original":false}
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let result = import_rmlint_output(
            &mut db,
            temp_file.path(),
            RmlintImportOptions {
                apply_trash: true,
                clear_existing_duplicates: true,
            },
        )
        .unwrap();

        assert_eq!(result.duplicate_groups_created, 1);

        let a = db.get_file(a_id).unwrap().unwrap();
        let b = db.get_file(b_id).unwrap().unwrap();
        let c = db.get_file(c_id).unwrap().unwrap();

        assert!(a.duplicate_group.is_none());
        assert!(b.duplicate_group.is_some());
        assert_eq!(b.duplicate_group, c.duplicate_group);
    }
}
