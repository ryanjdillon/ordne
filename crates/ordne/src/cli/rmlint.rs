use clap::Subcommand;
use console::style;
use ordne_lib::{Result, SqliteDatabase};
use ordne_lib::index::{import_rmlint_output, RmlintImportOptions};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum RmlintSubcommand {
    Import {
        #[arg(help = "Path to rmlint JSON output")]
        path: PathBuf,
        #[arg(long, help = "Do not mark empty files/dirs or bad links as trash")]
        no_classify: bool,
    },
}

pub fn handle_rmlint_command(
    db: &mut SqliteDatabase,
    subcommand: RmlintSubcommand,
    _verbose: bool,
) -> Result<()> {
    match subcommand {
        RmlintSubcommand::Import { path, no_classify } => {
            let options = RmlintImportOptions { apply_trash: !no_classify };
            let result = import_rmlint_output(db, path, options)?;

            println!("{} rmlint import complete", style("✓").green());
            println!("  Lints parsed: {}", result.lints_total);
            println!("  Matched files: {}", result.matched_files);
            println!("  Duplicate groups created: {}", result.duplicate_groups_created);
            println!("  Duplicate files assigned: {}", result.duplicate_files_assigned);
            println!("  Empty files marked: {}", result.empty_files_marked);
            println!("  Empty dirs marked: {}", result.empty_dirs_marked);
            println!("  Bad links marked: {}", result.bad_links_marked);
            println!("  Skipped lints: {}", result.skipped_lints);
            Ok(())
        }
    }
}
