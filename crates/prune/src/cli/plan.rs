use prune_lib::{Result, PruneError};
use console::style;
use prune_lib::{
    Database, File, Planner, PlannerOptions, PlansDatabase, PlanStatus, SqliteDatabase,
    MigrationStep,
    db::files::get_files_by_category,
};
use comfy_table::{Table, Cell, presets::UTF8_FULL};

pub fn handle_plan_command(
    db: &mut SqliteDatabase,
    subcommand: PlanSubcommand,
    verbose: bool,
) -> Result<()> {
    match subcommand {
        PlanSubcommand::Create { plan_type, source_drive, target_drive, category_filter } => {
            create_plan(db, &plan_type, source_drive.as_deref(), target_drive.as_deref(), category_filter.as_deref(), verbose)
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
        source_drive: Option<String>,
        target_drive: Option<String>,
        category_filter: Option<String>,
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
    _source_drive: Option<&str>,
    _target_drive: Option<&str>,
    category_filter: Option<&str>,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("{} Creating {} plan...", style(">>>").cyan(), plan_type);
    }

    let plan_id = match plan_type {
        "delete-trash" => {
            // Query trash files BEFORE creating planner
            let files = if let Some(category) = category_filter {
                get_files_by_category(db.conn(), category)?
            } else {
                get_files_by_category(db.conn(), "trash")?
            };

            if files.is_empty() {
                return Err(PruneError::Config("No trash files found".to_string()));
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
            // For now, return error - needs duplicate group selection
            return Err(PruneError::Config(
                "Dedup plan creation requires interactive duplicate selection (not yet implemented in CLI)".to_string()
            ));
        }
        "migrate" | "offload" => {
            // For now, return error - needs target drive and file selection
            return Err(PruneError::Config(
                format!("{} plan creation requires target drive and file selection (not yet implemented in CLI)", plan_type)
            ));
        }
        _ => {
            return Err(PruneError::Config(format!(
                "Unknown plan type: '{}'. Valid types: delete-trash, dedup, migrate, offload",
                plan_type
            )));
        }
    };

    let plan = db.get_plan(plan_id)?.ok_or(PruneError::PlanNotFound(plan_id))?;

    println!("{} Plan created (ID: {})", style("✓").green(), style(plan_id).bold());
    println!("  Type: {}", plan_type);
    println!("  Files: {}", plan.total_files);
    println!("  Size: {}", crate::util::format::format_bytes(plan.total_bytes));
    println!("  Status: {}", style(plan.status.as_str()).yellow());
    println!("\nRun 'prune plan show {}' to see details", plan_id);
    println!("Run 'prune plan approve {}' to approve for execution", plan_id);

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
    let plan = db.get_plan(id)?.ok_or(PruneError::PlanNotFound(id))?;

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
        println!("\n{} Run 'prune plan approve {}' to approve this plan", style("Tip:").cyan(), id);
    } else if plan.status.as_str() == "approved" {
        println!("\n{} Run 'prune migrate {}' to execute this plan", style("Tip:").cyan(), id);
    }

    Ok(())
}

fn approve_plan(db: &mut SqliteDatabase, id: i64) -> Result<()> {
    let plan = db.get_plan(id)?.ok_or(PruneError::PlanNotFound(id))?;

    if plan.status.as_str() != "pending" {
        return Err(PruneError::Config(format!(
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
    println!("\nRun 'prune migrate {}' to execute", id);

    Ok(())
}
