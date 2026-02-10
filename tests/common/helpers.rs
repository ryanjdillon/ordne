use prune_lib::{Backend, DriveRole, File, FileStatus, Priority};
use chrono::Utc;

pub fn create_file_builder(drive_id: i64, path: &str) -> FileBuilder {
    FileBuilder::new(drive_id, path)
}

pub struct FileBuilder {
    file: File,
}

impl FileBuilder {
    pub fn new(drive_id: i64, path: &str) -> Self {
        let filename = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(path)
            .to_string();

        let extension = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_string());

        let abs_path = format!("/mnt/test/{}", path);

        Self {
            file: File {
                id: 0,
                drive_id,
                path: path.to_string(),
                abs_path,
                filename,
                extension,
                size_bytes: 1024,
                md5_hash: Some("abc123def456".to_string()),
                blake3_hash: None,
                created_at: None,
                modified_at: Some(Utc::now()),
                inode: Some(12345),
                device_num: Some(1),
                nlinks: Some(1),
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
            },
        }
    }

    pub fn with_size(mut self, size: i64) -> Self {
        self.file.size_bytes = size;
        self
    }

    pub fn with_hash(mut self, hash: &str) -> Self {
        self.file.md5_hash = Some(hash.to_string());
        self
    }

    pub fn with_blake3_hash(mut self, hash: &str) -> Self {
        self.file.blake3_hash = Some(hash.to_string());
        self
    }

    pub fn with_category(mut self, category: &str) -> Self {
        self.file.category = Some(category.to_string());
        self
    }

    pub fn with_subcategory(mut self, subcategory: &str) -> Self {
        self.file.subcategory = Some(subcategory.to_string());
        self
    }

    pub fn with_duplicate_group(mut self, group_id: i64) -> Self {
        self.file.duplicate_group = Some(group_id);
        self
    }

    pub fn with_status(mut self, status: FileStatus) -> Self {
        self.file.status = status;
        self
    }

    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.file.priority = priority;
        self
    }

    pub fn build(self) -> File {
        self.file
    }
}
