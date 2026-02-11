use clap::Subcommand;
use ordne_lib::{apply_policy, load_effective_policy, OrdneError, Result, SqliteDatabase};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum PolicySubcommand {
    Validate {
        #[arg(help = "Path to policy file")]
        path: PathBuf,
    },
    Show {
        #[arg(help = "Path to policy file")]
        path: PathBuf,
    },
    Apply {
        #[arg(help = "Path to policy file")]
        path: PathBuf,
        #[arg(long, help = "Perform dry run without actual changes")]
        dry_run: bool,
        #[arg(long, help = "Execute the migration (required for actual execution)")]
        execute: bool,
    },
}

pub fn handle_policy_command(
    _db: &mut SqliteDatabase,
    subcommand: PolicySubcommand,
    _verbose: bool,
) -> Result<()> {
    match subcommand {
        PolicySubcommand::Validate { path } => {
            let (policy, _rules) = load_effective_policy(_db, &path)?;
            policy.validate()?;
            println!("Policy OK: {}", path.display());
            Ok(())
        }
        PolicySubcommand::Show { path } => {
            let (policy, rules) = load_effective_policy(_db, &path)?;
            policy.validate()?;
            let json = serde_json::to_string_pretty(&serde_json::json!({
                "policy": policy,
                "rules": rules.rules,
            }))
                .map_err(|e| OrdneError::Config(format!("Failed to serialize policy: {}", e)))?;
            println!("{json}");
            Ok(())
        }
        PolicySubcommand::Apply { path, dry_run, execute } => {
            let (policy, _rules) = load_effective_policy(_db, &path)?;
            policy.validate()?;

            let result = apply_policy(_db, &policy)?;

            if !execute && !dry_run {
                println!(
                    "Policy applied: {} ({} plan(s) created). Use --execute or --dry-run to run.",
                    path.display(),
                    result.plan_ids.len()
                );
                return Ok(());
            }

            let plan_count = result.plan_ids.len();
            crate::cli::run_policy::execute_policy_plans(_db, &policy, result.plan_ids, dry_run, execute)?;

            println!(
                "Policy applied: {} ({} plan(s) created)",
                path.display(),
                plan_count
            );
            Ok(())
        }
    }
}
