use clap::Subcommand;
use console::style;
use ordne_lib::{Database, Result, SqliteDatabase};
use ordne_lib::index::{refresh_duplicates_for_drive, DedupAlgorithm};

#[derive(Subcommand)]
pub enum DedupSubcommand {
    Refresh {
        #[arg(long, help = "Drive label to scan and hash")]
        drive: String,
        #[arg(long, default_value = "blake3", help = "Hash algorithm: blake3 or md5")]
        algorithm: String,
        #[arg(long, help = "Recompute hashes even if already present")]
        rehash: bool,
    },
}

pub fn handle_dedup_command(
    db: &mut SqliteDatabase,
    subcommand: DedupSubcommand,
    _verbose: bool,
) -> Result<()> {
    match subcommand {
        DedupSubcommand::Refresh { drive, algorithm, rehash } => {
            let drive_info = db.get_drive(&drive)?
                .ok_or_else(|| ordne_lib::OrdneError::DriveNotFound(drive.clone()))?;

            if !drive_info.is_online {
                return Err(ordne_lib::OrdneError::Config(format!(
                    "Drive is offline: {}",
                    drive
                )));
            }

            let mount_path = drive_info.mount_path.as_ref()
                .ok_or_else(|| ordne_lib::OrdneError::Config("Drive has no mount path".to_string()))?;

            let scan_opts = ordne_lib::ScanOptions {
                follow_symlinks: false,
                max_depth: None,
                include_hidden: false,
            };

            let stats = ordne_lib::scan_directory(db, drive_info.id, mount_path, scan_opts)?;
            let algorithm = DedupAlgorithm::from_str(&algorithm)?;

            let result = refresh_duplicates_for_drive(db, drive_info.id, algorithm, rehash)?;

            println!("{} Dedup refresh complete", style("✓").green());
            println!("  Files scanned: {}", stats.files_scanned);
            println!("  Bytes scanned: {}", stats.bytes_scanned);
            println!("  Files hashed: {}", result.files_hashed);
            println!("  Files skipped: {}", result.files_skipped);
            println!("  Groups created: {}", result.groups_created);
            println!("  Duplicate files assigned: {}", result.duplicate_files_assigned);
            Ok(())
        }
    }
}
