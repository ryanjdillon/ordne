use crate::db::{
    duplicates::{assign_files_to_duplicate_group, clear_duplicate_assignments, clear_duplicate_groups, create_duplicate_group},
    files::{list_files_by_drive, update_file_hash},
    File,
    SqliteDatabase,
};
use crate::error::{OrdneError, Result};
use crate::index::{hash_file_blake3, hash_file_md5};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy)]
pub enum DedupAlgorithm {
    Md5,
    Blake3,
}

impl DedupAlgorithm {
    pub fn from_str(value: &str) -> Result<Self> {
        match value {
            "md5" => Ok(DedupAlgorithm::Md5),
            "blake3" => Ok(DedupAlgorithm::Blake3),
            _ => Err(OrdneError::Config(format!(
                "Invalid algorithm '{}'. Use 'md5' or 'blake3'",
                value
            ))),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct DedupRefreshResult {
    pub files_hashed: usize,
    pub files_skipped: usize,
    pub groups_created: usize,
    pub duplicate_files_assigned: usize,
}

pub fn refresh_duplicates_for_drive(
    db: &mut SqliteDatabase,
    drive_id: i64,
    algorithm: DedupAlgorithm,
    rehash: bool,
) -> Result<DedupRefreshResult> {
    let mut result = DedupRefreshResult::default();

    let files = list_files_by_drive(db.conn(), drive_id)?;
    let mut hash_map: HashMap<String, Vec<File>> = HashMap::new();

    for file in files {
        if file.is_symlink {
            result.files_skipped += 1;
            continue;
        }

        let has_hash = match algorithm {
            DedupAlgorithm::Md5 => file.md5_hash.is_some(),
            DedupAlgorithm::Blake3 => file.blake3_hash.is_some(),
        };

        let hash = if has_hash && !rehash {
            match algorithm {
                DedupAlgorithm::Md5 => file.md5_hash.clone().unwrap(),
                DedupAlgorithm::Blake3 => file.blake3_hash.clone().unwrap(),
            }
        } else {
            let computed = match algorithm {
                DedupAlgorithm::Md5 => hash_file_md5(&file.abs_path)?,
                DedupAlgorithm::Blake3 => hash_file_blake3(&file.abs_path)?,
            };

            match algorithm {
                DedupAlgorithm::Md5 => update_file_hash(db.conn(), file.id, Some(&computed), None)?,
                DedupAlgorithm::Blake3 => update_file_hash(db.conn(), file.id, None, Some(&computed))?,
            }

            result.files_hashed += 1;
            computed
        };

        hash_map.entry(hash).or_default().push(file);
    }

    clear_duplicate_assignments(db.conn())?;
    clear_duplicate_groups(db.conn())?;

    for (hash, files) in hash_map.into_iter() {
        if files.len() < 2 {
            continue;
        }

        let mut drives = HashSet::new();
        let mut file_ids = Vec::new();
        let mut total_waste_bytes = 0i64;

        for file in &files {
            drives.insert(file.drive_id);
            file_ids.push(file.id);
        }

        file_ids.sort_unstable();
        let original_id = file_ids.first().copied();

        if let Some(original_id) = original_id {
            for file in &files {
                if file.id != original_id {
                    total_waste_bytes += file.size_bytes;
                }
            }
        }

        let drives_vec: Vec<i64> = drives.into_iter().collect();
        let cross_drive = drives_vec.len() > 1;

        let group_id = create_duplicate_group(
            db.conn(),
            &hash,
            file_ids.len() as i32,
            total_waste_bytes,
            original_id,
            &drives_vec,
            cross_drive,
        )?;

        assign_files_to_duplicate_group(db.conn(), &file_ids, group_id, original_id)?;

        result.groups_created += 1;
        result.duplicate_files_assigned += file_ids.len();
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{schema::initialize_schema, Backend, Database, Drive, DriveRole, FileStatus, Priority};

    fn setup_db() -> SqliteDatabase {
        let db = SqliteDatabase::open_in_memory().unwrap();
        initialize_schema(db.conn()).unwrap();
        db
    }

    fn insert_drive(db: &mut SqliteDatabase, label: &str) -> i64 {
        let drive = Drive {
            id: 0,
            label: label.to_string(),
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: Some(format!("/mnt/{}", label)),
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

    fn insert_file(db: &mut SqliteDatabase, drive_id: i64, abs_path: &str) -> i64 {
        let file = File {
            id: 0,
            drive_id,
            path: abs_path.to_string(),
            abs_path: abs_path.to_string(),
            filename: abs_path.split('/').last().unwrap().to_string(),
            extension: None,
            size_bytes: 100,
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
    fn test_refresh_duplicates_for_drive() {
        let mut db = setup_db();
        let drive_id = insert_drive(&mut db, "drive1");

        let file1 = insert_file(&mut db, drive_id, "/tmp/a.txt");
        let file2 = insert_file(&mut db, drive_id, "/tmp/b.txt");

        db.conn().execute(
            "UPDATE files SET md5_hash = ?1 WHERE id = ?2",
            ("samehash", file1),
        ).unwrap();
        db.conn().execute(
            "UPDATE files SET md5_hash = ?1 WHERE id = ?2",
            ("samehash", file2),
        ).unwrap();

        let result = refresh_duplicates_for_drive(&mut db, drive_id, DedupAlgorithm::Md5, false).unwrap();
        assert_eq!(result.groups_created, 1);
        assert_eq!(result.duplicate_files_assigned, 2);
    }
}
