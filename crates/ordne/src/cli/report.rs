use ordne_lib::{Result, OrdneError};
use comfy_table::{Table, presets::UTF8_FULL, Cell, Color};
use console::style;
use ordne_lib::{
    Database, SqliteDatabase,
    db::{
        files::get_category_stats,
        duplicates::get_duplicate_statistics,
    },
};
use crate::cli::helpers::get_drive_statistics;
use serde_json::json;
use std::path::PathBuf;

pub fn handle_report_command(db: &SqliteDatabase) -> Result<()> {
    println!("\n{}", style("Ordne System Report").bold().cyan());
    println!("{}\n", style("═".repeat(80)).dim());

    generate_report(db)?;

    Ok(())
}

pub fn handle_export_command(
    db: &SqliteDatabase,
    format: &str,
    output: Option<PathBuf>,
) -> Result<()> {
    match format.to_lowercase().as_str() {
        "json" => export_json(db, output),
        "csv" => export_csv(db, output),
        _ => Err(OrdneError::Config(format!(
            "Unsupported export format '{}'. Use 'json' or 'csv'",
            format
        ))),
    }
}

fn generate_report(db: &SqliteDatabase) -> Result<()> {
    println!("{}", style("Drive Summary").bold());
    println!("{}", style("─".repeat(80)).dim());

    let drives = db.list_drives()?;

    let mut drive_table = Table::new();
    drive_table.load_preset(UTF8_FULL);
    drive_table.set_header(vec![
        Cell::new("Drive").fg(Color::Cyan),
        Cell::new("Role").fg(Color::Cyan),
        Cell::new("Files").fg(Color::Cyan),
        Cell::new("Size").fg(Color::Cyan),
        Cell::new("Duplicates").fg(Color::Cyan),
        Cell::new("Wasted").fg(Color::Cyan),
    ]);

    for drive in &drives {
        let stats = get_drive_statistics(db, drive.id)?;

        drive_table.add_row(vec![
            Cell::new(&drive.label),
            Cell::new(drive.role.as_str()),
            Cell::new(stats.file_count),
            Cell::new(crate::util::format::format_bytes(stats.total_bytes)),
            Cell::new(stats.duplicate_file_count),
            Cell::new(crate::util::format::format_bytes(stats.duplicate_waste_bytes)),
        ]);
    }

    println!("{}\n", drive_table);

    println!("{}", style("Category Summary").bold());
    println!("{}", style("─".repeat(80)).dim());

    let category_stats = get_category_stats(db.conn())?;

    if category_stats.is_empty() {
        println!("{}\n", style("No classified files").yellow());
    } else {
        let mut cat_table = Table::new();
        cat_table.load_preset(UTF8_FULL);
        cat_table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Subcategory").fg(Color::Cyan),
            Cell::new("Files").fg(Color::Cyan),
            Cell::new("Size").fg(Color::Cyan),
        ]);

        for stat in &category_stats {
            cat_table.add_row(vec![
                Cell::new(&stat.category),
                Cell::new("-"),
                Cell::new(stat.file_count),
                Cell::new(crate::util::format::format_bytes(stat.total_bytes)),
            ]);
        }

        println!("{}\n", cat_table);
    }

    println!("{}", style("Duplicate Summary").bold());
    println!("{}", style("─".repeat(80)).dim());

    let dup_stats = get_duplicate_statistics(db.conn())?;

    if dup_stats.group_count == 0 {
        println!("{}\n", style("No duplicates found").green());
    } else {
        println!("  Groups: {}", style(dup_stats.group_count).yellow());
        println!("  Files: {}", style(dup_stats.total_duplicate_files).yellow());
        println!("  Wasted Space: {}", style(crate::util::format::format_bytes(dup_stats.total_waste_bytes)).red());
        println!("  Cross-Drive Groups: {}\n", dup_stats.cross_drive_groups);
    }

    println!("{}", style("Migration Summary").bold());
    println!("{}", style("─".repeat(80)).dim());

    let conn = db.conn();

    let total_plans: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans",
        [],
        |row| row.get(0)
    )?;

    let completed_plans: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans WHERE status = 'completed'",
        [],
        |row| row.get(0)
    )?;

    let total_migrated_bytes: i64 = conn.query_row(
        "SELECT COALESCE(SUM(completed_bytes), 0) FROM migration_plans WHERE status = 'completed'",
        [],
        |row| row.get(0)
    )?;

    if total_plans == 0 {
        println!("{}\n", style("No migration plans").yellow());
    } else {
        println!("  Total Plans: {}", total_plans);
        println!("  Completed: {}", style(completed_plans).green());
        println!("  Data Migrated: {}\n", crate::util::format::format_bytes(total_migrated_bytes));
    }

    Ok(())
}

fn export_json(db: &SqliteDatabase, output: Option<PathBuf>) -> Result<()> {
    let drives = db.list_drives()?;
    let category_stats = get_category_stats(db.conn())?;
    let dup_stats = get_duplicate_statistics(db.conn())?;

    let mut drive_data = Vec::new();
    for drive in drives {
        let stats = get_drive_statistics(db, drive.id)?;
        drive_data.push(json!({
            "label": drive.label,
            "role": drive.role.as_str(),
            "is_online": drive.is_online,
            "mount_path": drive.mount_path,
            "files": stats.file_count,
            "total_bytes": stats.total_bytes,
            "duplicate_files": stats.duplicate_file_count,
            "wasted_bytes": stats.duplicate_waste_bytes,
        }));
    }

    let mut category_data = Vec::new();
    for stat in category_stats {
        category_data.push(json!({
            "category": stat.category,
            "subcategory": None::<String>,
            "file_count": stat.file_count,
            "total_bytes": stat.total_bytes,
        }));
    }

    let report = json!({
        "drives": drive_data,
        "categories": category_data,
        "duplicates": {
            "groups": dup_stats.group_count,
            "files": dup_stats.total_duplicate_files,
            "wasted_bytes": dup_stats.total_waste_bytes,
            "cross_drive_groups": dup_stats.cross_drive_groups,
        },
        "generated_at": chrono::Utc::now().to_rfc3339(),
    });

    let json_str = serde_json::to_string_pretty(&report)?;

    if let Some(path) = output {
        std::fs::write(&path, json_str)?;
        println!("{} Report exported to {}", style("✓").green(), path.display());
    } else {
        println!("{}", json_str);
    }

    Ok(())
}

fn export_csv(db: &SqliteDatabase, output: Option<PathBuf>) -> Result<()> {
    let drives = db.list_drives()?;

    let mut csv = String::new();
    csv.push_str("Label,Role,Files,TotalBytes,DuplicateFiles,WastedBytes\n");

    for drive in drives {
        let stats = get_drive_statistics(db, drive.id)?;
        csv.push_str(&format!(
            "{},{},{},{},{},{}\n",
            drive.label,
            drive.role.as_str(),
            stats.file_count,
            stats.total_bytes,
            stats.duplicate_file_count,
            stats.duplicate_waste_bytes
        ));
    }

    if let Some(path) = output {
        std::fs::write(&path, csv)?;
        println!("{} Report exported to {}", style("✓").green(), path.display());
    } else {
        println!("{}", csv);
    }

    Ok(())
}
