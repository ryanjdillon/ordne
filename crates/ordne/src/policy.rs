use crate::error::{OrdneError, Result};
use crate::{
    classify::{ClassificationRule, ClassificationRules},
    db::files::{get_files_by_category, get_files_by_category_and_drive, list_files_by_duplicate_group},
    migrate::{Planner, PlannerOptions},
    Database, Priority, SqliteDatabase,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub version: String,
    pub name: String,
    pub description: Option<String>,
    pub scope: Option<PolicyScope>,
    pub classification: Option<PolicyClassification>,
    #[serde(default)]
    pub rules: HashMap<String, ClassificationRule>,
    #[serde(default)]
    pub plans: HashMap<String, PolicyPlan>,
    pub safety: Option<PolicySafety>,
    pub schedule: Option<PolicySchedule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyScope {
    #[serde(default)]
    pub include_drives: Vec<String>,
    #[serde(default)]
    pub exclude_drives: Vec<String>,
    #[serde(default)]
    pub include_paths: Vec<String>,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyClassification {
    pub default_priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPlan {
    #[serde(rename = "type")]
    pub plan_type: String,
    pub description: Option<String>,
    pub source_drive: Option<String>,
    pub target_drive: Option<String>,
    pub category_filter: Option<String>,
    pub duplicate_group: Option<i64>,
    pub original_file: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySafety {
    pub require_approval: Option<bool>,
    pub max_bytes_per_run: Option<String>,
    pub dry_run_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicySchedule {
    pub cron: Option<String>,
    pub timezone: Option<String>,
}

impl Policy {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(&path)
            .map_err(|e| OrdneError::Config(format!("Failed to read policy file: {}", e)))?;
        let policy: Policy = toml::from_str(&contents)
            .map_err(|e| OrdneError::Config(format!("Failed to parse policy TOML: {}", e)))?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<()> {
        if self.version.trim().is_empty() {
            return Err(OrdneError::Config("Policy version cannot be empty".to_string()));
        }
        if self.name.trim().is_empty() {
            return Err(OrdneError::Config("Policy name cannot be empty".to_string()));
        }

        if let Some(classification) = &self.classification {
            if let Some(priority) = &classification.default_priority {
                Priority::from_str(priority)
                    .map_err(|e| OrdneError::Config(format!("Invalid default_priority: {}", e)))?;
            }
        }

        for plan in self.plans.values() {
            if plan.plan_type.trim().is_empty() {
                return Err(OrdneError::Config("Plan type cannot be empty".to_string()));
            }
            if !is_valid_plan_type(&plan.plan_type) {
                return Err(OrdneError::Config(format!(
                    "Invalid plan type: {} (valid: delete-trash, dedup, migrate, offload)",
                    plan.plan_type
                )));
            }
        }

        Ok(())
    }
}

fn is_valid_plan_type(plan_type: &str) -> bool {
    matches!(plan_type, "delete-trash" | "dedup" | "migrate" | "offload")
}

#[derive(Debug, Clone)]
pub struct PolicyApplyResult {
    pub plan_ids: Vec<i64>,
}

pub fn apply_policy(
    db: &mut SqliteDatabase,
    policy: &Policy,
) -> Result<PolicyApplyResult> {
    policy.validate()?;

    enum PlanInput {
        DeleteTrash { files: Vec<crate::File> },
        Dedup { duplicates: Vec<crate::File>, original: crate::File },
        Migrate { files: Vec<crate::File>, target_id: i64, target_mount: String },
        Offload { files: Vec<crate::File>, target_id: i64, target_mount: String },
    }

    let mut inputs = Vec::new();
    for plan in policy.plans.values() {
        let input = match plan.plan_type.as_str() {
            "delete-trash" => {
                let category = plan.category_filter.as_deref().unwrap_or("trash");
                let files = collect_files_by_category(db, policy, plan, category)?;
                if files.is_empty() {
                    return Err(OrdneError::Config("No files matched category filter".to_string()));
                }
                PlanInput::DeleteTrash { files }
            }
            "dedup" => {
                let group_id = plan.duplicate_group.ok_or_else(|| {
                    OrdneError::Config("Dedup plans require duplicate_group".to_string())
                })?;
                let files = list_files_by_duplicate_group(db.conn(), group_id)?;
                if files.is_empty() {
                    return Err(OrdneError::Config("No files found in duplicate group".to_string()));
                }

                let original = if let Some(original_id) = plan.original_file {
                    db.get_file(original_id)?
                        .ok_or_else(|| OrdneError::Config("Original file not found".to_string()))?
                } else {
                    files.iter()
                        .find(|f| f.is_original)
                        .cloned()
                        .ok_or_else(|| OrdneError::Config(
                            "No original marked; provide original_file".to_string()
                        ))?
                };

                let duplicates: Vec<_> = files.into_iter().filter(|f| f.id != original.id).collect();
                if duplicates.is_empty() {
                    return Err(OrdneError::Config("No duplicate files to delete".to_string()));
                }

                PlanInput::Dedup { duplicates, original }
            }
            "migrate" | "offload" => {
                let target_label = plan.target_drive.as_deref().ok_or_else(|| {
                    OrdneError::Config("target_drive is required".to_string())
                })?;
                let target = db.get_drive(target_label)?
                    .ok_or_else(|| OrdneError::DriveNotFound(target_label.to_string()))?;
                let target_mount = target.mount_path.clone()
                    .ok_or_else(|| OrdneError::Config("Target drive has no mount path".to_string()))?;

                let category = plan.category_filter.as_deref().ok_or_else(|| {
                    OrdneError::Config("category_filter is required".to_string())
                })?;

                let files = collect_files_by_category(db, policy, plan, category)?;
                if files.is_empty() {
                    return Err(OrdneError::Config("No files matched category filter".to_string()));
                }

                if plan.plan_type == "migrate" {
                    PlanInput::Migrate { files, target_id: target.id, target_mount }
                } else {
                    PlanInput::Offload { files, target_id: target.id, target_mount }
                }
            }
            _ => {
                return Err(OrdneError::Config(format!(
                    "Invalid plan type: {}",
                    plan.plan_type
                )));
            }
        };

        inputs.push(input);
    }

    let planner_options = PlannerOptions {
        max_batch_size_bytes: None,
        enforce_space_limits: true,
        dry_run: false,
    };
    let mut planner = Planner::new(db, planner_options);

    let mut plan_ids = Vec::new();
    for input in inputs {
        let plan_id = match input {
            PlanInput::DeleteTrash { files } => planner.create_delete_trash_plan(files)?,
            PlanInput::Dedup { duplicates, original } => planner.create_dedup_plan(duplicates, &original)?,
            PlanInput::Migrate { files, target_id, target_mount } => {
                planner.create_migrate_plan(files, target_id, &target_mount)?
            }
            PlanInput::Offload { files, target_id, target_mount } => {
                planner.create_offload_plan(files, target_id, &target_mount)?
            }
        };

        plan_ids.push(plan_id);
    }

    Ok(PolicyApplyResult { plan_ids })
}

fn collect_files_by_category(
    db: &SqliteDatabase,
    policy: &Policy,
    plan: &PolicyPlan,
    category: &str,
) -> Result<Vec<crate::File>> {
    let mut files = Vec::new();

    if let Some(source_drive) = plan.source_drive.as_deref() {
        let drive = db.get_drive(source_drive)?
            .ok_or_else(|| OrdneError::DriveNotFound(source_drive.to_string()))?;
        files.extend(get_files_by_category_and_drive(db.conn(), category, drive.id)?);
        return Ok(files);
    }

    let include_drives = policy
        .scope
        .as_ref()
        .map(|s| s.include_drives.clone())
        .unwrap_or_default();
    let exclude_drives = policy
        .scope
        .as_ref()
        .map(|s| s.exclude_drives.clone())
        .unwrap_or_default();

    if include_drives.is_empty() {
        let all_files = get_files_by_category(db.conn(), category)?;
        let filtered = if exclude_drives.is_empty() {
            all_files
        } else {
            let excluded_ids: Vec<i64> = db
                .list_drives()?
                .into_iter()
                .filter(|d| exclude_drives.contains(&d.label))
                .map(|d| d.id)
                .collect();
            all_files.into_iter().filter(|f| !excluded_ids.contains(&f.drive_id)).collect()
        };
        files.extend(filtered);
        return Ok(files);
    }

    for label in include_drives {
        if exclude_drives.contains(&label) {
            continue;
        }
        let drive = db.get_drive(&label)?
            .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;
        files.extend(get_files_by_category_and_drive(db.conn(), category, drive.id)?);
    }

    Ok(files)
}

pub fn load_effective_policy(
    db: &SqliteDatabase,
    policy_path: &Path,
) -> Result<(Policy, ClassificationRules)> {
    let policy = Policy::load_from_file(policy_path)?;
    policy.validate()?;

    let mut rules = ClassificationRules { rules: HashMap::new() };

    let mut sources = Vec::new();
    sources.push(config_policy_path()?);

    if let Some(root) = policy_root_from_scope(db, &policy)? {
        sources.push(root.join(".ordne/ordne.toml"));
    }

    sources.push(policy_path.to_path_buf());

    for path in sources {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let mut parsed = ClassificationRules::from_toml(&content)?;
        rules.rules.extend(parsed.rules.drain());
    }

    if !policy.rules.is_empty() {
        rules.rules.extend(policy.rules.clone());
    }

    Ok((policy, rules))
}

fn policy_root_from_scope(db: &SqliteDatabase, policy: &Policy) -> Result<Option<PathBuf>> {
    let scope = match &policy.scope {
        Some(scope) => scope,
        None => return Ok(None),
    };

    let label = match scope.include_drives.first() {
        Some(label) => label,
        None => return Ok(None),
    };

    let drive = db.get_drive(label)?
        .ok_or_else(|| OrdneError::DriveNotFound(label.to_string()))?;
    let mount = drive.mount_path.ok_or_else(|| OrdneError::Config("Drive has no mount path".to_string()))?;
    Ok(Some(PathBuf::from(mount)))
}

fn config_policy_path() -> Result<PathBuf> {
    let xdg = xdg::BaseDirectories::with_prefix("ordne")
        .map_err(|e| OrdneError::Config(format!("Failed to initialize XDG directories: {}", e)))?;
    Ok(xdg.get_config_home().join("ordne.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_empty_name() {
        let policy = Policy {
            version: "0.1".to_string(),
            name: "".to_string(),
            description: None,
            scope: None,
            classification: None,
            rules: HashMap::new(),
            plans: HashMap::new(),
            safety: None,
            schedule: None,
        };

        assert!(policy.validate().is_err());
    }
}
