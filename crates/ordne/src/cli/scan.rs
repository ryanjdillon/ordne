use ordne_lib::{Result, OrdneError};
use console::style;
use ordne_lib::{Database, SqliteDatabase, scan_directory, ScanOptions, db::drives::mark_drive_scanned};
use std::path::PathBuf;

pub fn handle_scan_command(
    db: &mut SqliteDatabase,
    drive_label: Option<String>,
    path: Option<PathBuf>,
    scan_all: bool,
    verbose: bool,
) -> Result<()> {
    if scan_all {
        scan_all_drives(db, verbose)
    } else if let Some(label) = drive_label {
        scan_single_drive(db, &label, path.as_deref(), verbose)
    } else {
        Err(OrdneError::Config(
            "Must specify either a drive label or --all".to_string(),
        ))
    }
}

fn scan_single_drive(
    db: &mut SqliteDatabase,
    label: &str,
    subpath: Option<&std::path::Path>,
    _verbose: bool,
) -> Result<()> {
    let drive = db.get_drive(label)?
        .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;

    if !drive.is_online {
        return Err(OrdneError::DriveOffline(label.to_string()));
    }

    let mount_path = drive.mount_path
        .as_ref()
        .ok_or_else(|| OrdneError::Config(format!("Drive '{}' has no mount path", label)))?;

    let scan_path = if let Some(sub) = subpath {
        PathBuf::from(mount_path).join(sub)
    } else {
        PathBuf::from(mount_path)
    };

    if !scan_path.exists() {
        return Err(OrdneError::FileNotFound(scan_path));
    }

    println!(
        "{} Scanning drive '{}' at {}...",
        style(">>>").cyan(),
        style(label).bold(),
        scan_path.display()
    );

    let stats = scan_directory(db, drive.id, &scan_path, ScanOptions::default())?;

    mark_drive_scanned(db.conn(), drive.id)?;

    println!("\n{} Scan completed", style("✓").green());
    println!("  Files indexed: {}", style(stats.files_scanned).cyan());
    println!("  Directories scanned: {}", style(stats.dirs_scanned).cyan());
    println!("  Total size: {}", style(crate::util::format::format_bytes(stats.bytes_scanned as i64)).cyan());

    if stats.errors > 0 {
        println!("  Errors: {}", style(stats.errors).yellow());
    }

    if stats.symlinks_found > 0 {
        println!("  Symlinks: {}", style(stats.symlinks_found).dim());
    }

    Ok(())
}

fn scan_all_drives(db: &mut SqliteDatabase, _verbose: bool) -> Result<()> {
    let drives = db.list_drives()?;
    let online_drives: Vec<_> = drives.into_iter().filter(|d| d.is_online).collect();

    if online_drives.is_empty() {
        println!("{}", style("No online drives to scan").yellow());
        return Ok(());
    }

    println!(
        "{} Scanning {} online drives...\n",
        style(">>>").cyan(),
        online_drives.len()
    );

    let mut total_files = 0;
    let mut total_errors = 0;

    for drive in online_drives {
        let mount_path = match &drive.mount_path {
            Some(p) => PathBuf::from(p),
            None => {
                println!(
                    "{} Skipping drive '{}': no mount path",
                    style("!").yellow(),
                    drive.label
                );
                continue;
            }
        };

        if !mount_path.exists() {
            println!(
                "{} Skipping drive '{}': path does not exist",
                style("!").yellow(),
                drive.label
            );
            continue;
        }

        println!(
            "{} Scanning '{}'...",
            style(">>>").cyan(),
            style(&drive.label).bold()
        );

        match scan_directory(db, drive.id, &mount_path, ScanOptions::default()) {
            Ok(stats) => {
                mark_drive_scanned(db.conn(), drive.id)?;
                total_files += stats.files_scanned;
                total_errors += stats.errors;
                println!(
                    "  {} files indexed, {} errors\n",
                    stats.files_scanned,
                    stats.errors
                );
            }
            Err(e) => {
                println!("  {}: {}\n", style("Error").red(), e);
                total_errors += 1;
            }
        }
    }

    println!("{} All drives scanned", style("✓").green());
    println!("  Total files: {}", style(total_files).cyan());

    if total_errors > 0 {
        println!("  Total errors: {}", style(total_errors).yellow());
    }

    Ok(())
}
