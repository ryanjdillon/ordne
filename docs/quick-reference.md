# Ordne Indexing Module - Quick Reference

## Quick Start

```rust
use ordne_lib::{Database, SqliteDatabase, scan_directory, hash_file_md5};

let mut db = SqliteDatabase::open("ordne.db")?;
db.initialize()?;

let stats = scan_directory(&mut db, drive_id, "/path/to/scan", Default::default())?;
println!("Scanned {} files", stats.files_scanned);
```

## Indexing Module Overview

**Components**
- Device discovery (`discover_device`, `discover_rclone_remote`)
- Filesystem scanning (`scan_directory`, `ScanOptions`)
- Hashing (`hash_file_md5`, `hash_file_blake3`, `verify_hash`)
- Duplicate grouping (DB-backed duplicate groups)
- rmlint JSON parsing (library support)

**Notes**
- Scanning uses `walkdir` and captures metadata (size, mtime, inode, device, hardlinks).
- Hashing is streamed for large files and supports progress callbacks.
- Duplicate detection is DB-driven (hash → group).

## Core Functions

### Device Discovery

```rust
use ordne_lib::discover_device;

let device_info = discover_device("/mnt/backup")?;
println!("UUID: {:?}", device_info.uuid);
```

### Directory Scanning

```rust
use ordne_lib::{scan_directory, ScanOptions};

let options = ScanOptions {
    follow_symlinks: false,
    max_depth: Some(5),
    include_hidden: true,
};

let stats = scan_directory(&mut db, drive_id, "/data", options)?;
```

### File Hashing

```rust
use ordne_lib::{hash_file_md5, hash_file_blake3, verify_hash};

let md5 = hash_file_md5("/path/to/file")?;

let blake3 = hash_file_blake3("/path/to/file")?;

verify_hash("/path/to/copy", &md5)?;
```

### rmlint Integration

```rust
use ordne_lib::index::parse_rmlint_output;

let parser = parse_rmlint_output("rmlint.json")?;
let groups = parser.extract_duplicate_groups();

for group in groups {
    println!("Hash: {}, Files: {}", group.hash, group.files.len());
}
```

## Database Operations

### Drive Operations

```rust
use ordne_lib::db::drives;

let drive_id = drives::register_drive(
    db.conn(),
    "my_drive",
    &device_info,
    DriveRole::Source,
    Backend::Local,
)?;

drives::mark_drive_scanned(db.conn(), drive_id)?;

drives::update_drive_online_status(db.conn(), drive_id, true)?;
```

### File Operations

```rust
use ordne_lib::db::files;

let all_files = files::list_files_by_drive(db.conn(), drive_id)?;

files::update_file_hash(db.conn(), file_id, Some("hash"), None)?;

let duplicates = files::list_files_by_hash(db.conn(), "hash")?;

let stats = files::get_drive_statistics(db.conn(), drive_id)?;
```

### Duplicate Operations

```rust
use ordne_lib::db::duplicates;

let group_id = duplicates::create_duplicate_group(
    db.conn(),
    "hash",
    file_count,
    waste_bytes,
    Some(original_id),
    &[drive1, drive2],
    true,
)?;

let groups = duplicates::list_cross_drive_duplicates(db.conn())?;

let stats = duplicates::get_duplicate_statistics(db.conn())?;
```

## Common Patterns

### Complete Scan and Duplicate Detection

```rust
use ordne_lib::{Database, SqliteDatabase, scan_directory, hash_file_md5};
use ordne_lib::db::{files, duplicates};
use std::collections::HashMap;

let mut db = SqliteDatabase::open("ordne.db")?;
db.initialize()?;

let stats = scan_directory(&mut db, drive_id, "/data", Default::default())?;

let all_files = files::list_files_by_drive(db.conn(), drive_id)?;
let mut hash_groups: HashMap<String, Vec<i64>> = HashMap::new();

for file in &all_files {
    if !file.is_symlink && file.size_bytes > 0 {
        let hash = hash_file_md5(&file.abs_path)?;
        files::update_file_hash(db.conn(), file.id, Some(&hash), None)?;
        hash_groups.entry(hash).or_insert_with(Vec::new).push(file.id);
    }
}

for (hash, file_ids) in hash_groups.iter().filter(|(_, ids)| ids.len() > 1) {
    let group_id = duplicates::create_duplicate_group(
        db.conn(),
        hash,
        file_ids.len() as i32,
        0,
        Some(file_ids[0]),
        &[drive_id],
        false,
    )?;

    duplicates::assign_files_to_duplicate_group(
        db.conn(),
        file_ids,
        group_id,
        Some(file_ids[0]),
    )?;
}
```

### Cross-Drive Scan

```rust
let drive1_id = drives::register_drive(db.conn(), "drive1", &info1, ...)?;
let drive2_id = drives::register_drive(db.conn(), "drive2", &info2, ...)?;

scan_directory(&mut db, drive1_id, "/mnt/drive1", Default::default())?;
scan_directory(&mut db, drive2_id, "/mnt/drive2", Default::default())?;

let cross_drive = duplicates::list_cross_drive_duplicates(db.conn())?;
println!("Found {} cross-drive duplicate groups", cross_drive.len());
```

### Verify Data Integrity

```rust
use ordne_lib::index::verify_hash;

let file = files::get_file(db.conn(), file_id)?.unwrap();
if let Some(hash) = &file.md5_hash {
    match verify_hash(&file.abs_path, hash) {
        Ok(_) => println!("✓ File verified"),
        Err(e) => println!("✗ Verification failed: {}", e),
    }
}
```

### Progress Tracking

```rust
use ordne_lib::index::hash_file_md5_with_progress;

let hash = hash_file_md5_with_progress(&path, Box::new(|bytes, total| {
    let percent = (bytes as f64 / total as f64) * 100.0;
    println!("Hashing: {:.1}%", percent);
}))?;
```

## Error Handling

All functions return `Result<T>` with `OrdneError`:

```rust
use ordne_lib::{Result, OrdneError};

match scan_directory(&mut db, drive_id, "/data", options) {
    Ok(stats) => println!("Scanned {} files", stats.files_scanned),
    Err(OrdneError::FileNotFound(path)) => {
        eprintln!("Path not found: {:?}", path);
    }
    Err(OrdneError::Database(e)) => {
        eprintln!("Database error: {}", e);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

## Type Reference

### DeviceInfo
```rust
pub struct DeviceInfo {
    pub device_id: Option<String>,
    pub device_path: Option<String>,
    pub uuid: Option<String>,
    pub mount_path: Option<String>,
    pub fs_type: Option<String>,
    pub total_bytes: Option<i64>,
    pub model: Option<String>,
    pub serial: Option<String>,
}
```

### ScanStats
```rust
pub struct ScanStats {
    pub files_scanned: usize,
    pub dirs_scanned: usize,
    pub bytes_scanned: u64,
    pub symlinks_found: usize,
    pub git_repos_found: usize,
    pub errors: usize,
}
```

### ScanOptions
```rust
pub struct ScanOptions {
    pub follow_symlinks: bool,
    pub max_depth: Option<usize>,
    pub include_hidden: bool,
}
```

## Configuration

### Database Setup

```rust
use ordne_lib::{Config, SqliteDatabase};

let config = Config::new(Some("ordne.db".into()))?;
config.ensure_db_directory()?;

let mut db = SqliteDatabase::open(&config.db_path)?;
db.initialize()?;
```

### Scan Configuration

```rust
let options = ScanOptions {
    follow_symlinks: false,      // Don't follow symlinks
    max_depth: Some(10),          // Limit recursion depth
    include_hidden: false,        // Skip .hidden files
};
```

## Testing

Run tests:
```bash
cargo test                              # All tests
cargo test --lib                        # Unit tests only
cargo test --test indexing_test         # Integration tests
cargo test test_duplicate_detection     # Specific test
```

## CLI Examples

```bash
ordne drive add my_drive /mnt/backup --role source
ordne scan my_drive
ordne query duplicates
```

## Architecture Notes

1. All indexing operations return `Result<T>` with `OrdneError`.
2. Scans are incremental and insert/update file records.
3. Hashing is streaming and safe for large files.
4. DB operations are centralized in `ordne_lib::db`.

## Performance Tips

1. **Batch operations**: Process files in batches for better DB performance
2. **Filter by size**: Skip small files to reduce overhead
3. **Use blake3**: Faster than MD5 for large files
4. **Parallel hashing**: Use rayon for multi-threaded hashing
5. **Index maintenance**: Regularly vacuum database

## Best Practices

1. Always initialize database before use
2. Use transactions for batch operations
3. Verify hashes after file operations
4. Handle errors appropriately
5. Log scan statistics for monitoring
6. Update drive metadata periodically
7. Mark drives offline when unmounted
8. Use appropriate scan options for your use case

## Troubleshooting

**Issue**: "Permission denied" errors during scan
**Solution**: Run with appropriate permissions or skip unreadable files

**Issue**: Slow scanning on network drives
**Solution**: Use `max_depth` to limit recursion

**Issue**: Out of memory on large directories
**Solution**: Streaming implementation handles this automatically

**Issue**: Duplicate detection not working
**Solution**: Ensure files are hashed before grouping

## Known Limitations

1. Device discovery relies on Linux utilities (`blkid`, `lsblk`, `findmnt`).
2. Symlinks are not followed by default.
3. Some files may be unreadable due to permissions.

## See Also

- Spec: `docs/spec.md`
- Example code: `examples/indexing_example.rs`
- Integration tests: `tests/integration/indexing_test.rs`
