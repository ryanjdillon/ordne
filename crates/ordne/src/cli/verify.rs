use ordne_lib::{Result, OrdneError};
use console::style;
use ordne_lib::{
    Database, SqliteDatabase,
    index::hash_file_blake3,
};

pub fn handle_verify_command(
    db: &mut SqliteDatabase,
    drive_label: Option<String>,
    verbose: bool,
) -> Result<()> {
    if let Some(label) = drive_label {
        verify_drive(db, &label, verbose)
    } else {
        verify_all_drives(db, verbose)
    }
}

fn verify_drive(db: &mut SqliteDatabase, label: &str, verbose: bool) -> Result<()> {
    let drive = db.get_drive(label)?
        .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;

    if !drive.is_online {
        return Err(OrdneError::DriveOffline(label.to_string()));
    }

    println!(
        "{} Verifying files on drive '{}'...\n",
        style(">>>").cyan(),
        style(label).bold()
    );

    let files = super::helpers::list_files_by_drive(db, drive.id)?;

    if files.is_empty() {
        println!("{}", style("No files to verify").yellow());
        return Ok(());
    }

    let pb = if !verbose {
        Some(crate::util::progress::create_progress_bar(
            files.len() as u64,
            "Verifying hashes"
        ))
    } else {
        None
    };

    let mut verified = 0;
    let mut mismatches = 0;
    let mut errors = 0;
    let mut missing = 0;

    for file in files {
        if let Some(pb) = &pb {
            pb.inc(1);
        }

        if file.blake3_hash.is_none() && file.md5_hash.is_none() {
            if verbose {
                println!("{} No hash for {}", style("·").dim(), file.path);
            }
            continue;
        }

        let file_path = std::path::PathBuf::from(&file.abs_path);

        if !file_path.exists() {
            missing += 1;
            if verbose {
                println!("{} Missing: {}", style("×").red(), file.path);
            }
            continue;
        }

        match hash_file_blake3(&file_path) {
            Ok(new_hash) => {
                if let Some(stored_hash) = &file.blake3_hash {
                    if &new_hash == stored_hash {
                        verified += 1;
                        if verbose {
                            println!("{} OK: {}", style("✓").green(), file.path);
                        }
                    } else {
                        mismatches += 1;
                        println!(
                            "{} Mismatch: {}",
                            style("×").red(),
                            file.path
                        );
                        if verbose {
                            println!("    Expected: {}", stored_hash);
                            println!("    Got:      {}", new_hash);
                        }
                    }
                } else {
                    super::helpers::update_file_hash(db, file.id, None, Some(&new_hash))?;
                    verified += 1;
                    if verbose {
                        println!("{} Hash computed: {}", style("+").cyan(), file.path);
                    }
                }
            }
            Err(e) => {
                errors += 1;
                if verbose {
                    println!("{} Error reading {}: {}", style("!").yellow(), file.path, e);
                }
            }
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    println!("\n{} Verification complete", style("✓").green());
    println!("  Verified: {}", style(verified).green());

    if mismatches > 0 {
        println!("  Mismatches: {}", style(mismatches).red());
    }
    if missing > 0 {
        println!("  Missing: {}", style(missing).yellow());
    }
    if errors > 0 {
        println!("  Errors: {}", style(errors).yellow());
    }

    Ok(())
}

fn verify_all_drives(db: &mut SqliteDatabase, verbose: bool) -> Result<()> {
    let drives = db.list_drives()?;
    let online_drives: Vec<_> = drives.into_iter().filter(|d| d.is_online).collect();

    if online_drives.is_empty() {
        println!("{}", style("No online drives to verify").yellow());
        return Ok(());
    }

    println!(
        "{} Verifying {} online drives...\n",
        style(">>>").cyan(),
        online_drives.len()
    );

    let mut total_verified = 0;
    let mut total_mismatches = 0;

    for drive in online_drives {
        println!(
            "{} Verifying '{}'...",
            style(">>>").cyan(),
            style(&drive.label).bold()
        );

        let files = super::helpers::list_files_by_drive(db, drive.id)?;
        let mut verified = 0;
        let mut mismatches = 0;

        for file in files {
            if file.blake3_hash.is_none() {
                continue;
            }

            let file_path = std::path::PathBuf::from(&file.abs_path);
            if !file_path.exists() {
                continue;
            }

            match hash_file_blake3(&file_path) {
                Ok(new_hash) => {
                    if let Some(stored_hash) = &file.blake3_hash {
                        if &new_hash == stored_hash {
                            verified += 1;
                        } else {
                            mismatches += 1;
                            if verbose {
                                println!("  {} Mismatch: {}", style("×").red(), file.path);
                            }
                        }
                    }
                }
                Err(_) => {}
            }
        }

        total_verified += verified;
        total_mismatches += mismatches;

        println!("  {} verified, {} mismatches\n", verified, mismatches);
    }

    println!("{} All drives verified", style("✓").green());
    println!("  Total verified: {}", style(total_verified).green());

    if total_mismatches > 0 {
        println!("  Total mismatches: {}", style(total_mismatches).red());
    }

    Ok(())
}
