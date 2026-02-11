use ordne_lib::{
    apply_policy, load_effective_policy,
    classify::{RuleEngine},
    db::files::update_file_classification,
    Database, EngineOptions, MigrationEngine, OrdneError, PlansDatabase, Policy, PolicyScope,
    Result, SqliteDatabase,
};

pub fn handle_run_policy_command(
    db: &mut SqliteDatabase,
    path: std::path::PathBuf,
    dry_run: bool,
    execute: bool,
) -> Result<()> {
    if !execute && !dry_run {
        return Err(OrdneError::Config(
            "Must specify either --execute or --dry-run".to_string(),
        ));
    }

    let (policy, rules) = load_effective_policy(db, &path)?;
    policy.validate()?;

    apply_classification_rules(db, &rules, policy.scope.as_ref())?;

    let result = apply_policy(db, &policy)?;

    execute_policy_plans(db, &policy, result.plan_ids, dry_run, execute)
}

fn apply_classification_rules(
    db: &mut SqliteDatabase,
    rules: &ordne_lib::ClassificationRules,
    scope: Option<&PolicyScope>,
) -> Result<()> {
    if rules.rules.is_empty() {
        return Ok(());
    }

    let engine = RuleEngine::new(rules.clone())?;
    let mut files = crate::cli::helpers::get_unclassified_files(db, None)?;

    let (include_ids, exclude_ids, include_paths, exclude_paths) =
        scope_filters(db, scope)?;

    files.retain(|f| {
        if !include_ids.is_empty() && !include_ids.contains(&f.drive_id) {
            return false;
        }
        if exclude_ids.contains(&f.drive_id) {
            return false;
        }
        if !include_paths.is_empty() {
            let matched = include_paths.iter().any(|p| f.abs_path.starts_with(p));
            if !matched {
                return false;
            }
        }
        if exclude_paths.iter().any(|p| f.abs_path.starts_with(p)) {
            return false;
        }
        true
    });

    for file in files {
        if let Some(rule_match) = engine.classify(&file)? {
            update_file_classification(
                db.conn(),
                file.id,
                &rule_match.category,
                rule_match.subcategory.as_deref(),
                rule_match.priority,
            )?;
        }
    }

    Ok(())
}

fn scope_filters(
    db: &SqliteDatabase,
    scope: Option<&PolicyScope>,
) -> Result<(Vec<i64>, Vec<i64>, Vec<String>, Vec<String>)> {
    let mut include_ids = Vec::new();
    let mut exclude_ids = Vec::new();
    let mut include_paths = Vec::new();
    let mut exclude_paths = Vec::new();

    if let Some(scope) = scope {
        include_paths = scope.include_paths.clone();
        exclude_paths = scope.exclude_paths.clone();

        if !scope.include_drives.is_empty() {
            for label in &scope.include_drives {
                if let Some(drive) = db.get_drive(label)? {
                    include_ids.push(drive.id);
                }
            }
        }

        if !scope.exclude_drives.is_empty() {
            for label in &scope.exclude_drives {
                if let Some(drive) = db.get_drive(label)? {
                    exclude_ids.push(drive.id);
                }
            }
        }
    }

    Ok((include_ids, exclude_ids, include_paths, exclude_paths))
}
pub fn execute_policy_plans(
    db: &mut SqliteDatabase,
    policy: &Policy,
    plan_ids: Vec<i64>,
    dry_run: bool,
    execute: bool,
) -> Result<()> {
    let safety = policy.safety.clone();
    let require_approval = safety.as_ref().and_then(|s| s.require_approval).unwrap_or(false);
    let dry_run_only = safety.as_ref().and_then(|s| s.dry_run_only).unwrap_or(false);

    if execute && dry_run_only {
        return Err(OrdneError::Config(
            "Policy is dry-run only".to_string(),
        ));
    }

    if let Some(max_str) = safety.as_ref().and_then(|s| s.max_bytes_per_run.as_deref()) {
        let max_bytes = crate::util::format::parse_size_string(max_str)
            .map_err(|e| OrdneError::Config(format!("Invalid max_bytes_per_run: {}", e)))?;
        let total_bytes: i64 = plan_ids
            .iter()
            .filter_map(|id| db.get_plan(*id).ok().flatten())
            .map(|p| p.total_bytes)
            .sum();
        if total_bytes > max_bytes {
            return Err(OrdneError::Config(format!(
                "Plans total {} bytes exceeds policy max {}",
                total_bytes, max_bytes
            )));
        }
    }

    if execute && require_approval {
        for plan_id in &plan_ids {
            let plan = db.get_plan(*plan_id)?
                .ok_or(OrdneError::PlanNotFound(*plan_id))?;
            if plan.status.as_str() != "approved" {
                return Err(OrdneError::Config(format!(
                    "Plan {} not approved; run 'ordne plan approve {}'",
                    plan_id, plan_id
                )));
            }
        }
    }

    let engine_opts = EngineOptions {
        dry_run: dry_run || !execute,
        verify_hashes: true,
        retry_count: 3,
        enforce_safety: true,
    };

    let mut engine = MigrationEngine::new(db, engine_opts);
    for plan_id in plan_ids {
        engine.execute_plan(plan_id)?;
    }

    Ok(())
}
