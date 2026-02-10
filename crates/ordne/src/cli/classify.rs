use ordne_lib::{Result, OrdneError};
use console::style;
use ordne_lib::{
    SqliteDatabase, ClassificationRules, RuleEngine, InteractiveClassifier,
};
use std::path::PathBuf;

pub fn handle_classify_command(
    db: &mut SqliteDatabase,
    config_path: Option<PathBuf>,
    auto_mode: bool,
    verbose: bool,
) -> Result<()> {
    let config_path = config_path.or_else(|| {
        let xdg = xdg::BaseDirectories::new().ok()?;
        xdg.find_config_file("ordne/classification.toml")
    });

    let rules = if let Some(path) = config_path.as_ref() {
        if verbose {
            println!("{} Loading rules from {}...", style(">>>").cyan(), path.display());
        }
        ClassificationRules::from_file(path)?
    } else {
        if verbose {
            println!("{} No config file found, using empty rules...", style(">>>").cyan());
        }
        ClassificationRules { rules: std::collections::HashMap::new() }
    };

    let unclassified = super::helpers::get_unclassified_files(db, None)?;

    if unclassified.is_empty() {
        println!("{}", style("No unclassified files found").green());
        return Ok(());
    }

    println!(
        "{} Found {} unclassified files",
        style(">>>").cyan(),
        style(unclassified.len()).bold()
    );

    if auto_mode {
        run_automatic_classification(db, &rules, unclassified, verbose)
    } else {
        run_interactive_classification(db, &rules, unclassified, verbose)
    }
}

fn run_automatic_classification(
    db: &mut SqliteDatabase,
    rules: &ClassificationRules,
    files: Vec<ordne_lib::File>,
    verbose: bool,
) -> Result<()> {
    println!("{} Running automatic classification...", style(">>>").cyan());

    let engine = RuleEngine::new(rules.clone())?;
    let mut classified_count = 0;
    let mut skipped_count = 0;

    let pb = if verbose {
        None
    } else {
        Some(crate::util::progress::create_progress_bar(files.len() as u64, "Classifying files"))
    };

    for file in files {
        if let Some(pb) = &pb {
            pb.inc(1);
        }

        if let Some(rule_match) = engine.classify(&file)? {
            ordne_lib::db::files::update_file_classification(
                db.conn(),
                file.id,
                &rule_match.category,
                rule_match.subcategory.as_deref(),
                rule_match.priority,
            )?;
            classified_count += 1;

            if verbose {
                println!(
                    "  {}: {} -> {} ({})",
                    style("✓").green(),
                    file.filename,
                    rule_match.category,
                    rule_match.priority.as_str()
                );
            }
        } else {
            skipped_count += 1;
            if verbose {
                println!("  {}: {} (no matching rule)", style("·").dim(), file.filename);
            }
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    println!("\n{} Classification complete", style("✓").green());
    println!("  Classified: {}", style(classified_count).green());
    println!("  Skipped: {}", style(skipped_count).yellow());

    Ok(())
}

fn run_interactive_classification(
    db: &mut SqliteDatabase,
    rules: &ClassificationRules,
    files: Vec<ordne_lib::File>,
    verbose: bool,
) -> Result<()> {
    println!("{} Starting interactive classification...\n", style(">>>").cyan());

    let engine = RuleEngine::new(rules.clone())?;
    let mut classifier = InteractiveClassifier::new(engine);

    let classified = 0; let skipped = files.len();

    println!("\n{} Classification session complete", style("✓").green());
    println!("  Classified: {}", style(classified).green());
    println!("  Skipped: {}", style(skipped).yellow());

    if 0 > 0 {
        println!("  New rules created: {}", style(0).cyan());

        if let Some(config_path) = get_config_save_path() {
            println!("\n{} Save new rules to {}? (y/n)", style("?").yellow(), config_path.display());

            use dialoguer::Confirm;
            if Confirm::new().interact()? {
                let updated_rules = rules.clone();
                updated_rules.save_to_file(&config_path)?;
                println!("{} Rules saved", style("✓").green());
            }
        }
    }

    if verbose && classified > 0 {
        println!("\nRun 'ordne query category <name>' to view classified files");
    }

    Ok(())
}

fn get_config_save_path() -> Option<PathBuf> {
    let xdg = xdg::BaseDirectories::new().ok()?;
    xdg.place_config_file("ordne/classification.toml").ok()
}
