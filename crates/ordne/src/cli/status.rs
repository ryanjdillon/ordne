use crate::Result;
use comfy_table::{Table, presets::UTF8_FULL, Cell, Color};
use console::style;
use ordne_lib::{Database, SqliteDatabase, db::{files::get_category_stats, duplicates::get_duplicate_statistics}};
use crate::cli::helpers::get_drive_statistics;

pub fn handle_status_command(db: &SqliteDatabase, show_space: bool) -> Result<()> {
    println!("\n{}", style("Ordne System Status").bold().cyan());
    println!("{}\n", style("═".repeat(60)).dim());

    show_drive_summary(db)?;
    show_file_summary(db)?;
    show_duplicate_summary(db)?;
    show_classification_summary(db)?;
    show_plan_summary(db)?;

    if show_space {
        println!();
        show_space_details(db)?;
    }

    Ok(())
}

fn show_drive_summary(db: &SqliteDatabase) -> Result<()> {
    let drives = db.list_drives()?;
    let online_count = drives.iter().filter(|d| d.is_online).count();
    let offline_count = drives.len() - online_count;

    println!("{}", style("Drives").bold());
    println!("  Total: {}", drives.len());
    println!("  Online: {}", style(online_count).green());
    if offline_count > 0 {
        println!("  Offline: {}", style(offline_count).red());
    }
    println!();

    Ok(())
}

fn show_file_summary(db: &SqliteDatabase) -> Result<()> {
    let conn = db.conn();

    let total_files: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE status != 'source_removed'",
        [],
        |row| row.get(0)
    )?;

    let total_size: i64 = conn.query_row(
        "SELECT COALESCE(SUM(size_bytes), 0) FROM files WHERE status != 'source_removed'",
        [],
        |row| row.get(0)
    )?;

    println!("{}", style("Files").bold());
    println!("  Total Files: {}", style(total_files).cyan());
    println!("  Total Size: {}", style(crate::util::format::format_bytes(total_size)).cyan());
    println!();

    Ok(())
}

fn show_duplicate_summary(db: &SqliteDatabase) -> Result<()> {
    let stats = get_duplicate_statistics(db.conn())?;

    if stats.group_count > 0 {
        println!("{}", style("Duplicates").bold());
        println!("  Duplicate Groups: {}", style(stats.group_count).yellow());
        println!("  Duplicate Files: {}", style(stats.total_duplicate_files).yellow());
        println!("  Wasted Space: {}", style(crate::util::format::format_bytes(stats.total_waste_bytes)).red());
        println!("  Cross-Drive Groups: {}", stats.cross_drive_groups);
        println!();
    }

    Ok(())
}

fn show_classification_summary(db: &SqliteDatabase) -> Result<()> {
    let conn = db.conn();

    let classified: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE category IS NOT NULL",
        [],
        |row| row.get(0)
    )?;

    let unclassified: i64 = conn.query_row(
        "SELECT COUNT(*) FROM files WHERE category IS NULL AND status = 'indexed'",
        [],
        |row| row.get(0)
    )?;

    if classified > 0 || unclassified > 0 {
        println!("{}", style("Classification").bold());
        println!("  Classified: {}", style(classified).green());
        if unclassified > 0 {
            println!("  Unclassified: {}", style(unclassified).yellow());
        }

        let stats = get_category_stats(conn)?;
        if !stats.is_empty() {
            println!("  Categories: {}", stats.len());
        }
        println!();
    }

    Ok(())
}

fn show_plan_summary(db: &SqliteDatabase) -> Result<()> {
    let conn = db.conn();

    let draft_plans: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans WHERE status = 'draft'",
        [],
        |row| row.get(0)
    )?;

    let approved_plans: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans WHERE status = 'approved'",
        [],
        |row| row.get(0)
    )?;

    let in_progress: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans WHERE status = 'in_progress'",
        [],
        |row| row.get(0)
    )?;

    let completed: i64 = conn.query_row(
        "SELECT COUNT(*) FROM migration_plans WHERE status = 'completed'",
        [],
        |row| row.get(0)
    )?;

    if draft_plans + approved_plans + in_progress + completed > 0 {
        println!("{}", style("Migration Plans").bold());
        if draft_plans > 0 {
            println!("  Draft: {}", style(draft_plans).yellow());
        }
        if approved_plans > 0 {
            println!("  Approved: {}", style(approved_plans).cyan());
        }
        if in_progress > 0 {
            println!("  In Progress: {}", style(in_progress).blue());
        }
        if completed > 0 {
            println!("  Completed: {}", style(completed).green());
        }
        println!();
    }

    Ok(())
}

fn show_space_details(db: &SqliteDatabase) -> Result<()> {
    let drives = db.list_drives()?;

    if drives.is_empty() {
        return Ok(());
    }

    println!("{}", style("Space Details").bold().cyan());
    println!("{}", style("─".repeat(60)).dim());

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        Cell::new("Drive").fg(Color::Cyan),
        Cell::new("Used").fg(Color::Cyan),
        Cell::new("Capacity").fg(Color::Cyan),
        Cell::new("Files").fg(Color::Cyan),
    ]);

    for drive in drives {
        let file_stats = get_drive_statistics(db, drive.id)?;

        let capacity_str = drive.total_bytes
            .map(|b| crate::util::format::format_bytes(b))
            .unwrap_or_else(|| "Unknown".to_string());

        table.add_row(vec![
            Cell::new(&drive.label),
            Cell::new(crate::util::format::format_bytes(file_stats.total_bytes)),
            Cell::new(capacity_str),
            Cell::new(file_stats.file_count),
        ]);
    }

    println!("{}", table);

    Ok(())
}
