use ordne_lib::{Result, OrdneError};
use clap::Subcommand;
use comfy_table::{Table, presets::UTF8_FULL, Cell, Color};
use console::style;
use ordne_lib::{
    Database,
    SqliteDatabase,
    db::{
        files::get_files_by_category,
        duplicates::list_duplicate_groups,
    },
};

#[derive(Subcommand)]
pub enum QueryCommands {
    #[command(about = "Show duplicate files")]
    Duplicates {
        #[arg(long, help = "Filter by drive label")]
        drive: Option<String>,
    },

    #[command(about = "Show unclassified files")]
    Unclassified {
        #[arg(long, help = "Maximum number of files to show")]
        limit: Option<usize>,
    },

    #[command(about = "Show files in a category")]
    Category {
        #[arg(help = "Category name")]
        category: String,
    },

    #[command(about = "Show large files")]
    LargeFiles {
        #[arg(long, help = "Minimum file size (e.g., 100MB, 1GB)")]
        min_size: Option<String>,

        #[arg(long, help = "Maximum number of files to show")]
        limit: Option<usize>,
    },

    #[command(about = "Show files unique to backup drives")]
    BackupUnique,
}

pub fn handle_query_command(db: &SqliteDatabase, action: QueryCommands) -> Result<()> {
    match action {
        QueryCommands::Duplicates { drive } => query_duplicates(db, drive.as_deref()),
        QueryCommands::Unclassified { limit } => query_unclassified(db, limit),
        QueryCommands::Category { category } => query_category(db, &category),
        QueryCommands::LargeFiles { min_size, limit } => query_large_files(db, min_size.as_deref(), limit),
        QueryCommands::BackupUnique => query_backup_unique(db),
    }
}

fn query_duplicates(db: &SqliteDatabase, drive_label: Option<&str>) -> Result<()> {
    let groups = list_duplicate_groups(db.conn())?;

    let filtered_groups = if let Some(label) = drive_label {
        let drive = db.get_drive(label)?
            .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;

        groups.into_iter()
            .filter(|g| g.drives_involved.contains(&drive.id))
            .collect::<Vec<_>>()
    } else {
        groups
    };

    if filtered_groups.is_empty() {
        println!("{}", style("No duplicate groups found").yellow());
        return Ok(());
    }

    println!(
        "\n{} ({} groups)\n",
        style("Duplicate Groups").bold().cyan(),
        filtered_groups.len()
    );

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Group ID").fg(Color::Cyan),
        Cell::new("Hash").fg(Color::Cyan),
        Cell::new("Files").fg(Color::Cyan),
        Cell::new("Wasted").fg(Color::Cyan),
        Cell::new("Cross-Drive").fg(Color::Cyan),
        Cell::new("Status").fg(Color::Cyan),
    ]);

    for group in filtered_groups.iter().take(50) {
        let hash_short = if group.hash.len() > 12 {
            format!("{}...", &group.hash[..12])
        } else {
            group.hash.clone()
        };

        let cross_drive = if group.cross_drive { "Yes" } else { "No" };
        let resolution = group.resolution.as_deref().unwrap_or("-");

        table.add_row(vec![
            Cell::new(group.group_id),
            Cell::new(hash_short),
            Cell::new(group.file_count),
            Cell::new(crate::util::format::format_bytes(group.total_waste_bytes)),
            Cell::new(cross_drive),
            Cell::new(resolution),
        ]);
    }

    println!("{}", table);

    if filtered_groups.len() > 50 {
        println!("\n{}", style(format!("(Showing 50 of {} groups)", filtered_groups.len())).dim());
    }

    let total_waste: i64 = filtered_groups.iter().map(|g| g.total_waste_bytes).sum();
    println!(
        "\n{} {}",
        style("Total wasted space:").bold(),
        style(crate::util::format::format_bytes(total_waste)).red()
    );

    Ok(())
}

fn query_unclassified(db: &SqliteDatabase, limit: Option<usize>) -> Result<()> {
    let files = super::helpers::get_unclassified_files(db, limit)?;

    if files.is_empty() {
        println!("{}", style("No unclassified files").green());
        return Ok(());
    }

    println!(
        "\n{} ({} files)\n",
        style("Unclassified Files").bold().cyan(),
        files.len()
    );

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("ID").fg(Color::Cyan),
        Cell::new("Path").fg(Color::Cyan),
        Cell::new("Size").fg(Color::Cyan),
        Cell::new("Extension").fg(Color::Cyan),
    ]);

    for file in &files {
        let path_display = if file.path.len() > 60 {
            format!("...{}", &file.path[file.path.len() - 57..])
        } else {
            file.path.clone()
        };

        table.add_row(vec![
            Cell::new(file.id),
            Cell::new(path_display),
            Cell::new(crate::util::format::format_bytes(file.size_bytes)),
            Cell::new(file.extension.as_deref().unwrap_or("-")),
        ]);
    }

    println!("{}", table);

    let total_size: i64 = files.iter().map(|f| f.size_bytes).sum();
    println!(
        "\n{} {}",
        style("Total size:").bold(),
        style(crate::util::format::format_bytes(total_size)).cyan()
    );

    Ok(())
}

fn query_category(db: &SqliteDatabase, category: &str) -> Result<()> {
    let files = get_files_by_category(db.conn(), category)?;

    if files.is_empty() {
        println!("{}", style(format!("No files in category '{}'", category)).yellow());
        return Ok(());
    }

    println!(
        "\n{} '{}' ({} files)\n",
        style("Category").bold().cyan(),
        style(category).bold(),
        files.len()
    );

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("ID").fg(Color::Cyan),
        Cell::new("Subcategory").fg(Color::Cyan),
        Cell::new("Filename").fg(Color::Cyan),
        Cell::new("Size").fg(Color::Cyan),
        Cell::new("Priority").fg(Color::Cyan),
    ]);

    for file in files.iter().take(100) {
        let subcategory = file.subcategory.as_deref().unwrap_or("-");
        let filename_display = if file.filename.len() > 40 {
            format!("...{}", &file.filename[file.filename.len() - 37..])
        } else {
            file.filename.clone()
        };

        table.add_row(vec![
            Cell::new(file.id),
            Cell::new(subcategory),
            Cell::new(filename_display),
            Cell::new(crate::util::format::format_bytes(file.size_bytes)),
            Cell::new(file.priority.as_str()),
        ]);
    }

    println!("{}", table);

    if files.len() > 100 {
        println!("\n{}", style(format!("(Showing 100 of {} files)", files.len())).dim());
    }

    Ok(())
}

fn query_large_files(db: &SqliteDatabase, min_size_str: Option<&str>, limit: Option<usize>) -> Result<()> {
    let min_bytes = if let Some(size_str) = min_size_str {
        crate::util::format::parse_size_string(size_str)
            .map_err(|e| OrdneError::Config(e))?
    } else {
        100 * 1024 * 1024
    };

    let limit_val = limit.unwrap_or(50);

    let conn = db.conn();
    let mut stmt = conn.prepare(
        "SELECT id, drive_id, path, filename, size_bytes, category, priority
         FROM files
         WHERE size_bytes >= ?1
         ORDER BY size_bytes DESC
         LIMIT ?2"
    )?;

    let files = stmt.query_map([min_bytes, limit_val as i64], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;

    let files: Vec<_> = files.collect::<std::result::Result<_, _>>()?;

    if files.is_empty() {
        println!("{}", style(format!("No files larger than {}", crate::util::format::format_bytes(min_bytes))).yellow());
        return Ok(());
    }

    println!(
        "\n{} (>{}, {} files)\n",
        style("Large Files").bold().cyan(),
        crate::util::format::format_bytes(min_bytes),
        files.len()
    );

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("ID").fg(Color::Cyan),
        Cell::new("Filename").fg(Color::Cyan),
        Cell::new("Size").fg(Color::Cyan),
        Cell::new("Category").fg(Color::Cyan),
        Cell::new("Priority").fg(Color::Cyan),
    ]);

    for (id, _drive_id, _path, filename, size, category, priority) in files {
        let filename_display = if filename.len() > 50 {
            format!("...{}", &filename[filename.len() - 47..])
        } else {
            filename
        };

        table.add_row(vec![
            Cell::new(id),
            Cell::new(filename_display),
            Cell::new(crate::util::format::format_bytes(size)),
            Cell::new(category.as_deref().unwrap_or("-")),
            Cell::new(priority),
        ]);
    }

    println!("{}", table);

    Ok(())
}

fn query_backup_unique(db: &SqliteDatabase) -> Result<()> {
    let drives = db.list_drives()?;
    let backup_drives: Vec<_> = drives.iter()
        .filter(|d| matches!(d.role, ordne_lib::DriveRole::Backup))
        .collect();

    if backup_drives.is_empty() {
        println!("{}", style("No backup drives registered").yellow());
        return Ok(());
    }

    println!("\n{}\n", style("Files Unique to Backup Drives").bold().cyan());

    for drive in backup_drives {
        let conn = db.conn();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*), COALESCE(SUM(size_bytes), 0)
             FROM files
             WHERE drive_id = ?1
             AND (md5_hash IS NULL OR md5_hash NOT IN (
                 SELECT md5_hash FROM files WHERE drive_id != ?1 AND md5_hash IS NOT NULL
             ))"
        )?;

        let (count, total_size): (i64, i64) = stmt.query_row([drive.id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

        println!("{}", style(&drive.label).bold());
        println!("  Unique files: {}", style(count).cyan());
        println!("  Total size: {}", style(crate::util::format::format_bytes(total_size)).cyan());
        println!();
    }

    Ok(())
}
