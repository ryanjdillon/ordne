use ordne_lib::{Result, OrdneError};
use console::style;
use ordne_lib::{
    MigrationEngine, PlansDatabase, RollbackEngine, SqliteDatabase,
    EngineOptions,
};

pub fn handle_migrate_command(
    db: &mut SqliteDatabase,
    plan_id: i64,
    dry_run: bool,
    execute: bool,
    _verbose: bool,
) -> Result<()> {
    if !execute && !dry_run {
        return Err(OrdneError::Config(
            "Must specify either --execute or --dry-run".to_string(),
        ));
    }

    let _plan = db.get_plan(plan_id)?
        .ok_or(OrdneError::PlanNotFound(plan_id))?;

    println!(
        "{} {} migration plan #{}...",
        style(">>>").cyan(),
        if dry_run { "Simulating" } else { "Executing" },
        plan_id
    );

    let options = EngineOptions {
        dry_run,
        verify_hashes: true,
        retry_count: 3,
        enforce_safety: true,
    };

    let mut engine = MigrationEngine::new(db, options);
    engine.execute_plan(plan_id)?;

    println!("\n{} Migration {}", 
        style("✓").green(),
        if dry_run { "simulation complete" } else { "complete" }
    );

    Ok(())
}

pub fn handle_rollback_command(
    db: &mut SqliteDatabase,
    plan_id: i64,
    _verbose: bool,
) -> Result<()> {
    let _plan = db.get_plan(plan_id)?
        .ok_or(OrdneError::PlanNotFound(plan_id))?;

    println!(
        "{} Rolling back migration plan #{}...",
        style(">>>").cyan(),
        plan_id
    );

    let mut engine = RollbackEngine::new(db, false);
    
    if !engine.can_rollback(plan_id)? {
        return Err(OrdneError::Migration(
            "Cannot rollback: plan contains completed delete operations".to_string()
        ));
    }

    engine.rollback_plan(plan_id)?;

    println!("\n{} Rollback complete", style("✓").green());

    Ok(())
}
