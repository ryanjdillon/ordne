use prune_lib::{
    Backend, Database, DriveRole, SqliteDatabase,
    discover_device, hash_file_md5, scan_directory,
};
use prune_lib::db::{duplicates, files};
use prune_lib::index::{RmlintParser, ScanOptions, parse_rmlint_output};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Prune Indexing Module Example\n");

    let mut db = SqliteDatabase::open("example_prune.db")?;
    db.initialize()?;

    println!("=== Example 1: Device Discovery ===");
    if let Ok(device_info) = discover_device("/") {
        println!("Root filesystem:");
        println!("  UUID: {:?}", device_info.uuid);
        println!("  Type: {:?}", device_info.fs_type);
        println!("  Size: {:?} bytes", device_info.total_bytes);
        println!("  Device: {:?}", device_info.device_path);
    }
    println!();

    println!("=== Example 2: Register and Scan a Drive ===");
    let device_info = discover_device("/tmp")?;
    let drive_id = prune_lib::db::drives::register_drive(
        db.conn(),
        "tmp_drive",
        &device_info,
        DriveRole::Source,
        Backend::Local,
    )?;
    println!("Registered drive with ID: {}", drive_id);

    let options = ScanOptions {
        follow_symlinks: false,
        max_depth: Some(3),
        include_hidden: false,
    };

    println!("Scanning /tmp with max depth 3...");
    let stats = scan_directory(&mut db, drive_id, "/tmp", options)?;
    println!("Scan complete:");
    println!("  Files: {}", stats.files_scanned);
    println!("  Dirs: {}", stats.dirs_scanned);
    println!("  Bytes: {}", stats.bytes_scanned);
    println!("  Symlinks: {}", stats.symlinks_found);
    println!("  Git repos: {}", stats.git_repos_found);
    println!("  Errors: {}", stats.errors);
    println!();

    println!("=== Example 3: Hash Files and Detect Duplicates ===");
    let all_files = files::list_files_by_drive(db.conn(), drive_id)?;
    println!("Found {} files", all_files.len());

    let mut hash_groups: HashMap<String, Vec<i64>> = HashMap::new();
    let mut hashed_count = 0;

    for file in all_files.iter().take(10) {
        if file.is_symlink || file.size_bytes == 0 {
            continue;
        }

        if let Ok(hash) = hash_file_md5(&file.abs_path) {
            files::update_file_hash(db.conn(), file.id, Some(&hash), None)?;
            hash_groups.entry(hash).or_insert_with(Vec::new).push(file.id);
            hashed_count += 1;
        }
    }

    println!("Hashed {} files", hashed_count);

    let duplicate_groups: Vec<_> = hash_groups
        .iter()
        .filter(|(_, ids)| ids.len() > 1)
        .collect();

    println!("Found {} duplicate groups", duplicate_groups.len());

    for (hash, file_ids) in duplicate_groups.iter() {
        println!("  Duplicate group (hash: {}...): {} files", &hash[..8], file_ids.len());

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
    println!();

    println!("=== Example 4: Drive Statistics ===");
    let stats = files::get_drive_statistics(db.conn(), drive_id)?;
    println!("Drive statistics:");
    println!("  Total files: {}", stats.file_count);
    println!("  Total bytes: {} ({:.2} MB)", stats.total_bytes, stats.total_bytes as f64 / 1_048_576.0);
    println!("  Duplicate groups: {}", stats.duplicate_groups);
    println!("  Duplicate files: {}", stats.duplicate_file_count);
    println!("  Wasted space: {} bytes ({:.2} MB)",
             stats.duplicate_waste_bytes,
             stats.duplicate_waste_bytes as f64 / 1_048_576.0);
    println!();

    println!("=== Example 5: Parse rmlint Output ===");
    if std::path::Path::new("tests/fixtures/rmlint_sample.json").exists() {
        let parser = parse_rmlint_output("tests/fixtures/rmlint_sample.json")?;
        let rmlint_stats = parser.statistics();

        println!("rmlint statistics:");
        println!("  Duplicate files: {}", rmlint_stats.duplicate_files);
        println!("  Duplicate groups: {}", rmlint_stats.duplicate_groups);
        println!("  Duplicate waste: {} bytes", rmlint_stats.duplicate_size);
        println!("  Empty files: {}", rmlint_stats.empty_files);
        println!("  Empty dirs: {}", rmlint_stats.empty_dirs);

        let groups = parser.extract_duplicate_groups();
        println!("\nDuplicate groups from rmlint:");
        for group in groups.iter().take(3) {
            println!("  Hash {}: {} files, {} bytes total",
                     &group.hash[..8], group.files.len(), group.total_size);
            if parser.has_cross_drive_duplicates() {
                println!("    (includes cross-drive duplicates)");
            }
        }
    }
    println!();

    println!("=== Example 6: Query Files by Hash ===");
    if let Some((hash, _)) = hash_groups.iter().next() {
        let dup_files = files::list_files_by_hash(db.conn(), hash)?;
        println!("Files with hash {}...:", &hash[..8]);
        for file in dup_files.iter() {
            let original_marker = if file.is_original { " (ORIGINAL)" } else { "" };
            println!("  {} - {} bytes{}",
                     file.filename, file.size_bytes, original_marker);
        }
    }
    println!();

    println!("=== Example 7: Cross-Drive Duplicate Detection ===");
    let all_groups = duplicates::list_duplicate_groups(db.conn())?;
    let cross_drive_groups = duplicates::list_cross_drive_duplicates(db.conn())?;

    println!("Total duplicate groups: {}", all_groups.len());
    println!("Cross-drive duplicate groups: {}", cross_drive_groups.len());

    for group in cross_drive_groups.iter().take(3) {
        println!("  Group {}: {} files across {} drives",
                 group.group_id,
                 group.file_count,
                 group.drives_involved.len());
        println!("    Waste: {} bytes", group.total_waste_bytes);
    }
    println!();

    let dup_stats = duplicates::get_duplicate_statistics(db.conn())?;
    println!("Global duplicate statistics:");
    println!("  Groups: {}", dup_stats.group_count);
    println!("  Total duplicate files: {}", dup_stats.total_duplicate_files);
    println!("  Total waste: {} bytes ({:.2} MB)",
             dup_stats.total_waste_bytes,
             dup_stats.total_waste_bytes as f64 / 1_048_576.0);
    println!("  Cross-drive groups: {}", dup_stats.cross_drive_groups);

    println!("\n=== Indexing Example Complete ===");
    println!("Database saved to: example_prune.db");

    Ok(())
}
