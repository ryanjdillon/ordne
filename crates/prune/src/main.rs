mod cli;
mod util;

use clap::Parser;
use prune_lib::Result;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    let mut db = cli::init_database(cli.db)?;

    match cli.command {
        cli::Commands::Drive { action } => {
            cli::drive::handle_drive_command(&mut db, action, cli.verbose)
        }

        cli::Commands::Scan { drive_label, path, all } => {
            cli::scan::handle_scan_command(&mut db, drive_label, path, all, cli.verbose)
        }

        cli::Commands::Status { space } => {
            cli::status::handle_status_command(&db, space)
        }

        cli::Commands::Query { action } => {
            cli::query::handle_query_command(&db, action)
        }

        cli::Commands::Classify { config, auto } => {
            cli::classify::handle_classify_command(&mut db, config, auto, cli.verbose)
        }

        cli::Commands::Plan { action } => {
            cli::plan::handle_plan_command(&mut db, action, cli.verbose)
        }

        cli::Commands::Migrate { plan_id, dry_run, execute } => {
            cli::migrate::handle_migrate_command(&mut db, plan_id, dry_run, execute, cli.verbose)
        }

        cli::Commands::Rollback { plan_id } => {
            cli::migrate::handle_rollback_command(&mut db, plan_id, cli.verbose)
        }

        cli::Commands::Verify { drive } => {
            cli::verify::handle_verify_command(&mut db, drive, cli.verbose)
        }

        cli::Commands::Report => {
            cli::report::handle_report_command(&db)
        }

        cli::Commands::Export { format, output } => {
            cli::report::handle_export_command(&db, &format, output)
        }
    }
}
