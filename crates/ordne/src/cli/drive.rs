use ordne_lib::{Result, OrdneError};
use clap::Subcommand;
use comfy_table::{Table, presets::UTF8_FULL, Cell, Color};
use console::style;
use ordne_lib::{
    Backend, Database, DriveRole, SqliteDatabase,
    discover_device, db::drives::register_drive,
};
use crate::cli::helpers::get_drive_statistics;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DriveCommands {
    #[command(about = "Add a new drive")]
    Add {
        #[arg(help = "Drive label")]
        label: String,

        #[arg(help = "Path to drive (mount point or rclone remote)")]
        path: PathBuf,

        #[arg(long, help = "Drive role: source, target, backup, or offload")]
        role: String,

        #[arg(long, help = "Use rclone backend")]
        rclone: bool,
    },

    #[command(about = "List registered drives")]
    List,

    #[command(about = "Remove a drive")]
    Remove {
        #[arg(help = "Drive label")]
        label: String,
    },

    #[command(about = "Mark drive as online")]
    Online {
        #[arg(help = "Drive label")]
        label: String,
    },

    #[command(about = "Mark drive as offline")]
    Offline {
        #[arg(help = "Drive label")]
        label: String,
    },

    #[command(about = "Show drive information")]
    Info {
        #[arg(help = "Drive label")]
        label: String,
    },
}

pub fn handle_drive_command(db: &mut SqliteDatabase, action: DriveCommands, verbose: bool) -> Result<()> {
    match action {
        DriveCommands::Add { label, path, role, rclone } => {
            add_drive(db, &label, &path, &role, rclone, verbose)
        }
        DriveCommands::List => list_drives(db),
        DriveCommands::Remove { label } => remove_drive(db, &label),
        DriveCommands::Online { label } => set_drive_online(db, &label, true),
        DriveCommands::Offline { label } => set_drive_online(db, &label, false),
        DriveCommands::Info { label } => show_drive_info(db, &label),
    }
}

fn add_drive(
    db: &mut SqliteDatabase,
    label: &str,
    path: &PathBuf,
    role_str: &str,
    use_rclone: bool,
    verbose: bool,
) -> Result<()> {
    if db.get_drive(label)?.is_some() {
        return Err(OrdneError::Config(format!("Drive '{}' already exists", label)));
    }

    let role = DriveRole::from_str(role_str)?;
    let backend = if use_rclone {
        Backend::Rclone
    } else {
        Backend::Local
    };

    if verbose {
        println!("{} Discovering drive at {}...", style(">>>").cyan(), path.display());
    }

    let device_info = if use_rclone {
        ordne_lib::DeviceInfo {
            device_id: None,
            device_path: None,
            uuid: None,
            mount_path: Some(path.to_string_lossy().to_string()),
            fs_type: Some("rclone".to_string()),
            total_bytes: None,
            model: None,
            serial: None,
        }
    } else {
        discover_device(path)?
    };

    let conn = db.conn();
    let drive_id = register_drive(conn, label, &device_info, role, backend)?;

    println!(
        "{} Drive '{}' added successfully (ID: {})",
        style("✓").green(),
        style(label).bold(),
        drive_id
    );

    if verbose {
        if let Some(mount_path) = &device_info.mount_path {
            println!("  Mount path: {}", mount_path);
        }
        if let Some(fs_type) = &device_info.fs_type {
            println!("  Filesystem: {}", fs_type);
        }
        if let Some(uuid) = &device_info.uuid {
            println!("  UUID: {}", uuid);
        }
        println!("  Role: {}", role.as_str());
        println!("  Backend: {}", backend.as_str());
    }

    Ok(())
}

fn list_drives(db: &SqliteDatabase) -> Result<()> {
    let drives = db.list_drives()?;

    if drives.is_empty() {
        println!("{}", style("No drives registered").yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Label").fg(Color::Cyan),
        Cell::new("Role").fg(Color::Cyan),
        Cell::new("Backend").fg(Color::Cyan),
        Cell::new("Mount Path").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
        Cell::new("Files").fg(Color::Cyan),
        Cell::new("Last Scan").fg(Color::Cyan),
    ]);

    for drive in drives {
        let status = if drive.is_online {
            Cell::new("Online").fg(Color::Green)
        } else {
            Cell::new("Offline").fg(Color::Red)
        };

        let mount_path = drive.mount_path.unwrap_or_else(|| "-".to_string());
        let last_scan = drive
            .scanned_at
            .map(|dt| crate::util::format::format_timestamp(&dt))
            .unwrap_or_else(|| "Never".to_string());

        let file_count = db.conn()
            .query_row(
                "SELECT COUNT(*) FROM files WHERE drive_id = ?1",
                [drive.id],
                |row| row.get::<_, i64>(0)
            )
            .unwrap_or(0);

        table.add_row(vec![
            Cell::new(&drive.label).fg(Color::White),
            Cell::new(drive.role.as_str()),
            Cell::new(drive.backend.as_str()),
            Cell::new(mount_path),
            status,
            Cell::new(file_count),
            Cell::new(last_scan),
        ]);
    }

    println!("{}", table);
    Ok(())
}

fn remove_drive(db: &mut SqliteDatabase, label: &str) -> Result<()> {
    let drive = db.get_drive(label)?
        .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;

    let file_count: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM files WHERE drive_id = ?1",
        [drive.id],
        |row| row.get(0)
    )?;

    if file_count > 0 {
        println!(
            "{} Drive '{}' has {} files indexed.",
            style("Warning:").yellow(),
            label,
            file_count
        );
        println!("Files will remain in database but orphaned.");
    }

    db.conn_mut().execute("DELETE FROM drives WHERE label = ?1", [label])?;

    println!(
        "{} Drive '{}' removed",
        style("✓").green(),
        style(label).bold()
    );
    Ok(())
}

fn set_drive_online(db: &mut SqliteDatabase, label: &str, online: bool) -> Result<()> {
    db.update_drive_online_status(label, online)?;

    println!(
        "{} Drive '{}' marked as {}",
        style("✓").green(),
        style(label).bold(),
        if online { style("online").green() } else { style("offline").red() }
    );
    Ok(())
}

fn show_drive_info(db: &SqliteDatabase, label: &str) -> Result<()> {
    let drive = db.get_drive(label)?
        .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;

    println!("\n{}", style(format!("Drive: {}", drive.label)).bold().cyan());
    println!("{}", style("─".repeat(50)).dim());

    println!("  ID: {}", drive.id);
    println!("  Role: {}", style(drive.role.as_str()).yellow());
    println!("  Backend: {}", drive.backend.as_str());

    if let Some(mount_path) = &drive.mount_path {
        println!("  Mount Path: {}", mount_path);
    }
    if let Some(device_path) = &drive.device_path {
        println!("  Device: {}", device_path);
    }
    if let Some(uuid) = &drive.uuid {
        println!("  UUID: {}", uuid);
    }
    if let Some(fs_type) = &drive.fs_type {
        println!("  Filesystem: {}", fs_type);
    }
    if let Some(total_bytes) = drive.total_bytes {
        println!("  Capacity: {}", crate::util::format::format_bytes(total_bytes));
    }

    println!("  Status: {}", if drive.is_online {
        style("Online").green()
    } else {
        style("Offline").red()
    });

    if drive.is_readonly {
        println!("  Mode: {}", style("Read-only").yellow());
    }

    if let Some(scanned_at) = drive.scanned_at {
        println!("  Last Scan: {}", crate::util::format::format_timestamp(&scanned_at));
    }

    println!("  Added: {}", crate::util::format::format_timestamp(&drive.added_at));

    let stats = get_drive_statistics(db, drive.id)?;

    println!("\n{}", style("Statistics:").bold());
    println!("  Files: {}", style(stats.file_count).cyan());
    println!("  Total Size: {}", style(crate::util::format::format_bytes(stats.total_bytes)).cyan());

    if stats.duplicate_groups > 0 {
        println!("  Duplicate Groups: {}", style(stats.duplicate_groups).yellow());
        println!("  Duplicate Files: {}", style(stats.duplicate_file_count).yellow());
        println!("  Wasted Space: {}", style(crate::util::format::format_bytes(stats.duplicate_waste_bytes)).red());
    }

    println!();
    Ok(())
}
