use ordne_lib::{Result, OrdneError, Database};
use console::style;
use ordne_lib::{
    Planner, PlannerOptions, PlansDatabase, PlanStatus, SqliteDatabase,
    MigrationStep,
    db::files::{get_files_by_category, get_files_by_category_and_drive, list_files_by_duplicate_group},
};
use comfy_table::{Table, Cell, presets::UTF8_FULL};

pub fn handle_plan_command(
    db: &mut SqliteDatabase,
    subcommand: PlanSubcommand,
    verbose: bool,
) -> Result<()> {
    match subcommand {
        PlanSubcommand::Create { plan_type, source_drive, target_drive, category_filter, duplicate_group, original_file } => {
            create_plan(
                db,
                &plan_type,
                source_drive.as_deref(),
                target_drive.as_deref(),
                category_filter.as_deref(),
                duplicate_group,
                original_file,
                verbose,
            )
        }
        PlanSubcommand::List { status_filter } => {
            list_plans(db, status_filter.as_deref())
        }
        PlanSubcommand::Show { id } => {
            show_plan(db, id)
        }
        PlanSubcommand::Approve { id } => {
            approve_plan(db, id)
        }
    }
}

#[derive(clap::Subcommand)]
pub enum PlanSubcommand {
    Create {
        plan_type: String,
        #[arg(long, help = "Source drive label")]
        source_drive: Option<String>,
        #[arg(long, help = "Target drive label")]
        target_drive: Option<String>,
        #[arg(long, help = "Category filter")]
        category_filter: Option<String>,
        #[arg(long, help = "Duplicate group ID (dedup plans)")]
        duplicate_group: Option<i64>,
        #[arg(long, help = "Original file ID to keep (dedup plans)")]
        original_file: Option<i64>,
    },
    List {
        status_filter: Option<String>,
    },
    Show {
        id: i64,
    },
    Approve {
        id: i64,
    },
}

fn create_plan(
    db: &mut SqliteDatabase,
    plan_type: &str,
    source_drive: Option<&str>,
    target_drive: Option<&str>,
    category_filter: Option<&str>,
    duplicate_group: Option<i64>,
    original_file: Option<i64>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("{} Creating {} plan...", style(">>>").cyan(), plan_type);
    }

    let plan_id = match plan_type {
        "delete-trash" => {
            // Query trash files BEFORE creating planner
            let category = category_filter.unwrap_or("trash");
            let files = if let Some(source_drive) = source_drive {
                let drive = db.get_drive(source_drive)?
                    .ok_or_else(|| OrdneError::DriveNotFound(source_drive.to_string()))?;
                get_files_by_category_and_drive(db.conn(), category, drive.id)?
            } else {
                get_files_by_category(db.conn(), category)?
            };

            if files.is_empty() {
                return Err(OrdneError::Config("No trash files found".to_string()));
            }

            // Now create planner
            let options = PlannerOptions {
                max_batch_size_bytes: None,
                enforce_space_limits: true,
                dry_run: false,
            };
            let mut planner = Planner::new(db, options);

            planner.create_delete_trash_plan(files)?
        }
        "dedup" => {
            let group_id = duplicate_group.ok_or_else(|| OrdneError::Config(
                "Dedup plans require --duplicate-group <id>".to_string()
            ))?;

            let files = list_files_by_duplicate_group(db.conn(), group_id)?;
            if files.is_empty() {
                return Err(OrdneError::Config("No files found in duplicate group".to_string()));
            }

            let original = if let Some(original_id) = original_file {
                db.get_file(original_id)?
                    .ok_or_else(|| OrdneError::Config("Original file not found".to_string()))?
            } else {
                files.iter()
                    .find(|f| f.is_original)
                    .cloned()
                    .ok_or_else(|| OrdneError::Config(
                        "No original file marked in duplicate group; use --original-file".to_string()
                    ))?
            };

            let duplicates: Vec<_> = files.into_iter().filter(|f| f.id != original.id).collect();
            if duplicates.is_empty() {
                return Err(OrdneError::Config("No duplicate files to delete".to_string()));
            }

            let options = PlannerOptions {
                max_batch_size_bytes: None,
                enforce_space_limits: true,
                dry_run: false,
            };
            let mut planner = Planner::new(db, options);
            planner.create_dedup_plan(duplicates, &original)?
        }
        "migrate" | "offload" => {
            let target_label = target_drive.ok_or_else(|| OrdneError::Config(
                "Target drive required: --target-drive <label>".to_string()
            ))?;
            let target = db.get_drive(target_label)?
                .ok_or_else(|| OrdneError::DriveNotFound(target_label.to_string()))?;
            let target_mount = target.mount_path.clone()
                .ok_or_else(|| OrdneError::Config("Target drive has no mount path".to_string()))?;

            let category = category_filter.ok_or_else(|| OrdneError::Config(
                "Category filter required: --category-filter <category>".to_string()
            ))?;

            let files = if let Some(source_drive) = source_drive {
                let drive = db.get_drive(source_drive)?
                    .ok_or_else(|| OrdneError::DriveNotFound(source_drive.to_string()))?;
                get_files_by_category_and_drive(db.conn(), category, drive.id)?
            } else {
                get_files_by_category(db.conn(), category)?
            };

            if files.is_empty() {
                return Err(OrdneError::Config("No files matched category filter".to_string()));
            }

            let options = PlannerOptions {
                max_batch_size_bytes: None,
                enforce_space_limits: true,
                dry_run: false,
            };
            let mut planner = Planner::new(db, options);

            if plan_type == "migrate" {
                planner.create_migrate_plan(files, target.id, &target_mount)?
            } else {
                planner.create_offload_plan(files, target.id, &target_mount)?
            }
        }
        _ => {
            return Err(OrdneError::Config(format!(
                "Unknown plan type: '{}'. Valid types: delete-trash, dedup, migrate, offload",
                plan_type
            )));
        }
    };

    let plan = db.get_plan(plan_id)?.ok_or(OrdneError::PlanNotFound(plan_id))?;

    println!("{} Plan created (ID: {})", style("✓").green(), style(plan_id).bold());
    println!("  Type: {}", plan_type);
    println!("  Files: {}", plan.total_files);
    println!("  Size: {}", crate::util::format::format_bytes(plan.total_bytes));
    println!("  Status: {}", style(plan.status.as_str()).yellow());
    println!("\nRun 'ordne plan show {}' to see details", plan_id);
    println!("Run 'ordne plan approve {}' to approve for execution", plan_id);

    Ok(())
}

fn list_plans(db: &SqliteDatabase, status_filter: Option<&str>) -> Result<()> {
    let status = if let Some(s) = status_filter {
        Some(PlanStatus::from_str(s)?)
    } else {
        None
    };

    let plans = db.list_plans(status)?;

    if plans.is_empty() {
        println!("{}", style("No migration plans found").yellow());
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["ID", "Type", "Files", "Size", "Status", "Created"]);

    for plan in &plans {
        table.add_row(vec![
            Cell::new(plan.id),
            Cell::new(plan.description.as_deref().unwrap_or("(no description)")),
            Cell::new(plan.total_files),
            Cell::new(crate::util::format::format_bytes(plan.total_bytes)),
            Cell::new(plan.status.as_str()),
            Cell::new(crate::util::format::format_timestamp(&plan.created_at)),
        ]);
    }

    println!("{}", table);
    println!("\n{} plans total", plans.len());

    Ok(())
}

fn show_plan(db: &SqliteDatabase, id: i64) -> Result<()> {
    let plan = db.get_plan(id)?.ok_or(OrdneError::PlanNotFound(id))?;

    println!("{} Migration Plan #{}", style(">>>").cyan(), style(id).bold());
    println!("\n{}", style("Overview").bold());
    println!("  Description: {}", plan.description.as_deref().unwrap_or("(no description)"));
    println!("  Status: {}", style(plan.status.as_str()).yellow());
    println!("  Files: {}", plan.total_files);
    println!("  Total size: {}", crate::util::format::format_bytes(plan.total_bytes));
    println!("  Created: {}", crate::util::format::format_timestamp(&plan.created_at));

    println!("\n{}", style("Progress").bold());
    println!("  Completed files: {} / {}", plan.completed_files, plan.total_files);
    println!("  Completed bytes: {} / {}",
        crate::util::format::format_bytes(plan.completed_bytes),
        crate::util::format::format_bytes(plan.total_bytes)
    );

    let steps: Vec<MigrationStep> = vec![]; // TODO: implement list_steps
    if !steps.is_empty() {
        println!("\n{} ({} steps)", style("Steps").bold(), steps.len());

        let mut table = Table::new();
        table.load_preset(UTF8_FULL);
        table.set_header(vec!["#", "Action", "File", "Status"]);

        for (i, step) in steps.iter().take(10).enumerate() {
            table.add_row(vec![
                Cell::new(i + 1),
                Cell::new(step.action.as_str()),
                Cell::new(&step.source_path),
                Cell::new(step.status.as_str()),
            ]);
        }

        println!("{}", table);

        if steps.len() > 10 {
            println!("  ... and {} more steps", steps.len() - 10);
        }
    }

    if plan.status.as_str() == "pending" {
        println!("\n{} Run 'ordne plan approve {}' to approve this plan", style("Tip:").cyan(), id);
    } else if plan.status.as_str() == "approved" {
        println!("\n{} Run 'ordne migrate {}' to execute this plan", style("Tip:").cyan(), id);
    }

    Ok(())
}

fn approve_plan(db: &mut SqliteDatabase, id: i64) -> Result<()> {
    let plan = db.get_plan(id)?.ok_or(OrdneError::PlanNotFound(id))?;

    if plan.status.as_str() != "pending" {
        return Err(OrdneError::Config(format!(
            "Plan #{} is already {} (must be pending to approve)",
            id, plan.status.as_str()
        )));
    }

    let mut planner = Planner::new(db, PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: true,
        dry_run: false,
    });

    planner.approve_plan(id)?;

    println!("{} Plan #{} approved for execution", style("✓").green(), id);
    println!("\nRun 'ordne migrate {}' to execute", id);

    Ok(())
}
