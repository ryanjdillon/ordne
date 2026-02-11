pub mod drive;
pub mod scan;
pub mod status;
pub mod query;
pub mod classify;
pub mod plan;
pub mod migrate;
pub mod verify;
pub mod report;
pub mod policy;
pub mod run_policy;
mod helpers;

use ordne_lib::{Config, Database, Result, SqliteDatabase};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "ordne")]
#[command(about = "Safe file deduplication, classification and migration", long_about = None)]
#[command(version)]
pub struct Cli {
    #[arg(long, global = true, help = "Path to database file")]
    pub db: Option<PathBuf>,

    #[arg(long, short = 'v', global = true, help = "Enable verbose output")]
    pub verbose: bool,

    #[arg(long, short = 'q', global = true, help = "Suppress non-error output")]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Manage drives")]
    Drive {
        #[command(subcommand)]
        action: drive::DriveCommands,
    },

    #[command(about = "Scan drives for files")]
    Scan {
        #[arg(help = "Drive label to scan")]
        drive_label: Option<String>,

        #[arg(help = "Optional path within drive to scan")]
        path: Option<PathBuf>,

        #[arg(long, help = "Scan all online drives")]
        all: bool,
    },

    #[command(about = "Show system status")]
    Status {
        #[arg(long, help = "Show detailed space information")]
        space: bool,
    },

    #[command(about = "Query files and duplicates")]
    Query {
        #[command(subcommand)]
        action: query::QueryCommands,
    },

    #[command(about = "Classify files")]
    Classify {
        #[arg(long, help = "Path to classification rules config")]
        config: Option<PathBuf>,

        #[arg(long, help = "Run automatic classification without interaction")]
        auto: bool,
    },

    #[command(about = "Manage migration plans")]
    Plan {
        #[command(subcommand)]
        action: plan::PlanSubcommand,
    },

    #[command(about = "Execute migrations")]
    Migrate {
        #[arg(help = "Plan ID to migrate")]
        plan_id: i64,

        #[arg(long, help = "Perform dry run without actual changes")]
        dry_run: bool,

        #[arg(long, help = "Execute the migration (required for actual execution)")]
        execute: bool,
    },

    #[command(about = "Rollback a migration")]
    Rollback {
        #[arg(help = "Plan ID to rollback")]
        plan_id: i64,
    },

    #[command(about = "Verify file hashes")]
    Verify {
        #[arg(long, help = "Drive label to verify")]
        drive: Option<String>,
    },

    #[command(about = "Generate report")]
    Report,

    #[command(about = "Export data")]
    Export {
        #[arg(help = "Export format (json, csv)")]
        format: String,

        #[arg(long, short = 'o', help = "Output file path")]
        output: Option<PathBuf>,
    },

    #[command(about = "Manage policies")]
    Policy {
        #[command(subcommand)]
        action: policy::PolicySubcommand,
    },

    #[command(about = "Run a policy (non-interactive)")]
    RunPolicy {
        #[arg(help = "Path to policy file")]
        path: PathBuf,

        #[arg(long, help = "Perform dry run without actual changes")]
        dry_run: bool,

        #[arg(long, help = "Execute the migration (required for actual execution)")]
        execute: bool,
    },
}

pub fn init_database(db_path: Option<PathBuf>) -> Result<SqliteDatabase> {
    let config = Config::new(db_path)?;
    config.ensure_db_directory()?;

    let mut db = SqliteDatabase::open(&config.db_path)?;
    db.initialize()?;

    Ok(db)
}
