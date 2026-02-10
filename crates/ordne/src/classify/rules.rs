//! Classification rules engine for automatic file categorization.
//!
//! This module provides a flexible rules engine that can classify files based on:
//! - Path glob patterns
//! - File extensions
//! - Size thresholds
//! - Age (modified time)
//! - Duplicate status
//! - EXIF metadata for photos
//!
//! Rules are loaded from TOML configuration files and applied with priority-based
//! conflict resolution.

use crate::error::{OrdneError, Result};
use crate::db::{File, Priority};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;


/// A classification rule that matches files and assigns categories.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRule {
    /// Unique identifier for this rule
    #[serde(default)]
    pub name: String,
    /// Rule type (pattern, extension, size, age, duplicate)
    #[serde(flatten)]
    pub rule_type: RuleType,
    /// Category to assign when rule matches
    pub category: String,
    /// Optional subcategory
    pub subcategory: Option<String>,
    /// Pattern for generating dynamic subcategories from EXIF data
    #[serde(default)]
    pub subcategory_from_exif: Option<String>,
    /// Priority level for the file
    #[serde(default)]
    pub priority: Option<String>,
    /// Rule priority (higher = evaluated first)
    #[serde(default = "default_priority")]
    pub rule_priority: i32,
}

fn default_priority() -> i32 {
    50
}

/// Types of classification rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RuleType {
    /// Match files by glob patterns
    #[serde(rename = "pattern")]
    Pattern { patterns: Vec<String> },
    /// Match files by extension
    #[serde(rename = "extension")]
    Extension { extensions: Vec<String> },
    /// Match files by size
    #[serde(rename = "size")]
    Size {
        min_bytes: Option<i64>,
        max_bytes: Option<i64>,
    },
    /// Match files by age
    #[serde(rename = "age")]
    Age {
        older_than_days: Option<i64>,
        newer_than_days: Option<i64>,
    },
    /// Match duplicate files
    #[serde(rename = "duplicate")]
    Duplicate { keep_strategy: DuplicateStrategy },
}

/// Strategy for handling duplicate files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateStrategy {
    KeepOldest,
    KeepNewest,
    KeepOriginal,
}

/// Result of matching a rule against a file.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    pub rule_name: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub priority: Priority,
    pub rule_priority: i32,
}

/// EXIF metadata extracted from image files.
#[derive(Debug, Clone)]
pub struct ExifData {
    pub year: Option<String>,
    pub month: Option<String>,
    pub day: Option<String>,
    pub datetime: Option<DateTime<Utc>>,
    pub make: Option<String>,
    pub model: Option<String>,
}

/// Complete classification rules configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationRules {
    #[serde(default)]
    pub rules: HashMap<String, ClassificationRule>,
}

impl ClassificationRules {
    /// Load rules from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
    }

    /// Parse rules from TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self> {
        let mut config: toml::Value = toml::from_str(toml_str)
            .map_err(|e| OrdneError::Config(format!("Failed to parse TOML: {}", e)))?;

        let mut rules = HashMap::new();

        if let Some(rules_table) = config.get_mut("rules").and_then(|v| v.as_table_mut()) {
            for (name, rule_value) in rules_table.iter_mut() {
                let mut rule: ClassificationRule = rule_value.clone().try_into()
                    .map_err(|e| OrdneError::Config(format!("Failed to parse rule '{}': {}", name, e)))?;
                rule.name = name.clone();
                rules.insert(name.clone(), rule);
            }
        }

        Ok(ClassificationRules { rules })
    }

    /// Get all rules sorted by priority (highest first).
    pub fn sorted_rules(&self) -> Vec<&ClassificationRule> {
        let mut rules: Vec<&ClassificationRule> = self.rules.values().collect();
        rules.sort_by(|a, b| b.rule_priority.cmp(&a.rule_priority));
        rules
    }

    /// Save rules to a TOML file.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let mut rules_map = std::collections::HashMap::new();
        for (name, rule) in &self.rules {
            rules_map.insert(name.clone(), rule.clone());
        }
        let mut config = std::collections::HashMap::new();
        config.insert("rules", rules_map);
        let toml_string = toml::to_string_pretty(&config)
            .map_err(|e| OrdneError::Config(format!("Failed to serialize rules: {}", e)))?;
        std::fs::write(path, toml_string)?;
        Ok(())
    }
}

/// Classification engine that applies rules to files.
pub struct RuleEngine {
    rules: ClassificationRules,
    glob_cache: HashMap<String, GlobSet>,
}

impl RuleEngine {
    /// Create a new rule engine with the given rules.
    pub fn new(rules: ClassificationRules) -> Result<Self> {
        let mut engine = Self {
            rules,
            glob_cache: HashMap::new(),
        };
        engine.build_glob_cache()?;
        Ok(engine)
    }

    /// Build glob matchers for pattern-based rules.
    fn build_glob_cache(&mut self) -> Result<()> {
        for (name, rule) in &self.rules.rules {
            if let RuleType::Pattern { patterns } = &rule.rule_type {
                let mut builder = GlobSetBuilder::new();
                for pattern in patterns {
                    let glob = Glob::new(pattern)
                        .map_err(|e| OrdneError::Config(format!("Invalid glob pattern '{}': {}", pattern, e)))?;
                    builder.add(glob);
                }
                let globset = builder.build()
                    .map_err(|e| OrdneError::Config(format!("Failed to build globset: {}", e)))?;
                self.glob_cache.insert(name.clone(), globset);
            }
        }
        Ok(())
    }

    /// Classify a file and return the best matching rule.
    pub fn classify(&self, file: &File) -> Result<Option<RuleMatch>> {
        let mut matches = Vec::new();

        for rule in self.rules.sorted_rules() {
            if let Some(rule_match) = self.match_rule(rule, file)? {
                matches.push(rule_match);
            }
        }

        Ok(matches.into_iter().max_by_key(|m| m.rule_priority))
    }

    /// Classify multiple files in batch.
    pub fn classify_batch(&self, files: &[File]) -> Result<Vec<(i64, Option<RuleMatch>)>> {
        files.iter()
            .map(|file| Ok((file.id, self.classify(file)?)))
            .collect()
    }

    /// Check if a rule matches a file.
    fn match_rule(&self, rule: &ClassificationRule, file: &File) -> Result<Option<RuleMatch>> {
        let matches = match &rule.rule_type {
            RuleType::Pattern { .. } => self.match_pattern(rule, file)?,
            RuleType::Extension { extensions } => self.match_extension(extensions, file),
            RuleType::Size { min_bytes, max_bytes } => self.match_size(*min_bytes, *max_bytes, file),
            RuleType::Age { older_than_days, newer_than_days } => {
                self.match_age(*older_than_days, *newer_than_days, file)
            }
            RuleType::Duplicate { keep_strategy } => self.match_duplicate(keep_strategy, file),
        };

        if !matches {
            return Ok(None);
        }

        let subcategory = self.resolve_subcategory(rule, file)?;
        let priority = self.resolve_priority(rule);

        Ok(Some(RuleMatch {
            rule_name: rule.name.clone(),
            category: rule.category.clone(),
            subcategory,
            priority,
            rule_priority: rule.rule_priority,
        }))
    }

    /// Match file against pattern-based rule.
    fn match_pattern(&self, rule: &ClassificationRule, file: &File) -> Result<bool> {
        if let Some(globset) = self.glob_cache.get(&rule.name) {
            Ok(globset.is_match(&file.path) || globset.is_match(&file.abs_path))
        } else {
            Ok(false)
        }
    }

    /// Match file against extension rule.
    fn match_extension(&self, extensions: &[String], file: &File) -> bool {
        if let Some(ref ext) = file.extension {
            extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
        } else {
            false
        }
    }

    /// Match file against size rule.
    fn match_size(&self, min_bytes: Option<i64>, max_bytes: Option<i64>, file: &File) -> bool {
        let size = file.size_bytes;
        let min_ok = min_bytes.map_or(true, |min| size >= min);
        let max_ok = max_bytes.map_or(true, |max| size <= max);
        min_ok && max_ok
    }

    /// Match file against age rule.
    fn match_age(&self, older_than_days: Option<i64>, newer_than_days: Option<i64>, file: &File) -> bool {
        if let Some(modified_at) = file.modified_at {
            let now = Utc::now();
            let age_days = (now - modified_at).num_days();

            let older_ok = older_than_days.map_or(true, |days| age_days >= days);
            let newer_ok = newer_than_days.map_or(true, |days| age_days <= days);

            older_ok && newer_ok
        } else {
            false
        }
    }

    /// Match file against duplicate rule.
    fn match_duplicate(&self, _strategy: &DuplicateStrategy, file: &File) -> bool {
        file.duplicate_group.is_some()
    }

    /// Resolve subcategory, potentially using EXIF data.
    fn resolve_subcategory(&self, rule: &ClassificationRule, file: &File) -> Result<Option<String>> {
        if let Some(ref pattern) = rule.subcategory_from_exif {
            if let Some(exif) = extract_exif_data(&file.abs_path)? {
                return Ok(Some(substitute_exif_pattern(pattern, &exif)));
            }
        }

        Ok(rule.subcategory.clone())
    }

    /// Resolve priority from rule.
    fn resolve_priority(&self, rule: &ClassificationRule) -> Priority {
        rule.priority.as_ref()
            .and_then(|s| Priority::from_str(s).ok())
            .unwrap_or(Priority::Normal)
    }
}

/// Extract EXIF data from an image file.
pub fn extract_exif_data<P: AsRef<Path>>(path: P) -> Result<Option<ExifData>> {
    let path_ref = path.as_ref();

    let extension = path_ref.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let is_image = matches!(
        extension.to_lowercase().as_str(),
        "jpg" | "jpeg" | "png" | "heic" | "heif" | "tiff" | "tif"
    );

    if !is_image {
        return Ok(None);
    }

    let file = match std::fs::File::open(path_ref) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };

    let mut bufreader = std::io::BufReader::new(file);
    let exifreader = match exif::Reader::new().read_from_container(&mut bufreader) {
        Ok(reader) => reader,
        Err(_) => return Ok(None),
    };

    let mut data = ExifData {
        year: None,
        month: None,
        day: None,
        datetime: None,
        make: None,
        model: None,
    };

    if let Some(field) = exifreader.get_field(exif::Tag::DateTime, exif::In::PRIMARY) {
        if let Some(datetime_str) = field.display_value().to_string().split_whitespace().next() {
            let parts: Vec<&str> = datetime_str.split(':').collect();
            if parts.len() >= 3 {
                data.year = Some(parts[0].to_string());
                data.month = Some(parts[1].to_string());
                data.day = Some(parts[2].to_string());
            }
        }
    }

    if let Some(field) = exifreader.get_field(exif::Tag::Make, exif::In::PRIMARY) {
        data.make = Some(field.display_value().to_string());
    }

    if let Some(field) = exifreader.get_field(exif::Tag::Model, exif::In::PRIMARY) {
        data.model = Some(field.display_value().to_string());
    }

    Ok(Some(data))
}

/// Substitute EXIF data into a pattern string.
///
/// Supported patterns:
/// - `{exif_year}` - Year from EXIF DateTime
/// - `{exif_month}` - Month from EXIF DateTime (zero-padded)
/// - `{exif_day}` - Day from EXIF DateTime (zero-padded)
/// - `{exif_make}` - Camera manufacturer
/// - `{exif_model}` - Camera model
/// - `{filename}` - Original filename (no path)
pub fn substitute_exif_pattern(pattern: &str, exif: &ExifData) -> String {
    let mut result = pattern.to_string();

    if let Some(ref year) = exif.year {
        result = result.replace("{exif_year}", year);
    }
    if let Some(ref month) = exif.month {
        result = result.replace("{exif_month}", month);
    }
    if let Some(ref day) = exif.day {
        result = result.replace("{exif_day}", day);
    }
    if let Some(ref make) = exif.make {
        result = result.replace("{exif_make}", make);
    }
    if let Some(ref model) = exif.model {
        result = result.replace("{exif_model}", model);
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn create_test_file(path: &str, extension: Option<&str>, size_bytes: i64) -> File {
        File {
            id: 1,
            drive_id: 1,
            path: path.to_string(),
            abs_path: format!("/test/{}", path),
            filename: path.split('/').last().unwrap_or(path).to_string(),
            extension: extension.map(|s| s.to_string()),
            size_bytes,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: Some(Utc::now() - Duration::days(10)),
            inode: None,
            device_num: None,
            nlinks: None,
            mime_type: None,
            is_symlink: false,
            symlink_target: None,
            git_remote_url: None,
            category: None,
            subcategory: None,
            target_path: None,
            target_drive_id: None,
            priority: Priority::Normal,
            duplicate_group: None,
            is_original: false,
            rmlint_type: None,
            status: crate::db::FileStatus::Indexed,
            migrated_to: None,
            migrated_to_drive: None,
            migrated_at: None,
            verified_hash: None,
            error: None,
            indexed_at: Utc::now(),
        }
    }

    #[test]
    fn test_parse_rules_from_toml() {
        let toml = r#"
            [rules.trash]
            type = "pattern"
            patterns = ["**/node_modules/**", "**/.cache/**"]
            category = "trash"
            priority = "trash"
            rule_priority = 100

            [rules.large_files]
            type = "size"
            min_bytes = 1073741824
            category = "large"
            rule_priority = 50
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        assert_eq!(rules.rules.len(), 2);
        assert!(rules.rules.contains_key("trash"));
        assert!(rules.rules.contains_key("large_files"));

        let trash_rule = &rules.rules["trash"];
        assert_eq!(trash_rule.category, "trash");
        assert_eq!(trash_rule.rule_priority, 100);
    }

    #[test]
    fn test_pattern_matching() {
        let toml = r#"
            [rules.node_modules]
            type = "pattern"
            patterns = ["**/node_modules/**"]
            category = "trash"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let file = create_test_file("project/node_modules/package/index.js", Some("js"), 1024);
        let result = engine.classify(&file).unwrap();

        assert!(result.is_some());
        let matched = result.unwrap();
        assert_eq!(matched.category, "trash");
    }

    #[test]
    fn test_extension_matching() {
        let toml = r#"
            [rules.images]
            type = "extension"
            extensions = ["jpg", "png", "gif"]
            category = "images"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let file = create_test_file("photo.jpg", Some("jpg"), 1024);
        let result = engine.classify(&file).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "images");
    }

    #[test]
    fn test_size_matching() {
        let toml = r#"
            [rules.large]
            type = "size"
            min_bytes = 1000000
            category = "large"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let small_file = create_test_file("small.txt", Some("txt"), 1000);
        let large_file = create_test_file("large.bin", Some("bin"), 2000000);

        assert!(engine.classify(&small_file).unwrap().is_none());
        assert!(engine.classify(&large_file).unwrap().is_some());
    }

    #[test]
    fn test_age_matching() {
        let toml = r#"
            [rules.old]
            type = "age"
            older_than_days = 7
            category = "old"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let file = create_test_file("old.txt", Some("txt"), 1024);
        let result = engine.classify(&file).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "old");
    }

    #[test]
    fn test_rule_priority() {
        let toml = r#"
            [rules.low_priority]
            type = "extension"
            extensions = ["txt"]
            category = "documents"
            rule_priority = 10

            [rules.high_priority]
            type = "pattern"
            patterns = ["**/*.txt"]
            category = "text_files"
            rule_priority = 100
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let file = create_test_file("test.txt", Some("txt"), 1024);
        let result = engine.classify(&file).unwrap();

        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "text_files");
    }

    #[test]
    fn test_duplicate_matching() {
        let toml = r#"
            [rules.duplicates]
            type = "duplicate"
            keep_strategy = "keepoldest"
            category = "duplicate"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let mut file = create_test_file("dup.txt", Some("txt"), 1024);
        file.duplicate_group = Some(1);

        let result = engine.classify(&file).unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().category, "duplicate");
    }

    #[test]
    fn test_exif_pattern_substitution() {
        let exif = ExifData {
            year: Some("2024".to_string()),
            month: Some("03".to_string()),
            day: Some("15".to_string()),
            datetime: None,
            make: Some("Canon".to_string()),
            model: Some("EOS 5D".to_string()),
        };

        let pattern = "{exif_year}/{exif_month}/{exif_make}";
        let result = substitute_exif_pattern(pattern, &exif);
        assert_eq!(result, "2024/03/Canon");
    }

    #[test]
    fn test_batch_classification() {
        let toml = r#"
            [rules.images]
            type = "extension"
            extensions = ["jpg"]
            category = "images"

            [rules.documents]
            type = "extension"
            extensions = ["pdf"]
            category = "documents"
        "#;

        let rules = ClassificationRules::from_toml(toml).unwrap();
        let engine = RuleEngine::new(rules).unwrap();

        let files = vec![
            create_test_file("photo.jpg", Some("jpg"), 1024),
            create_test_file("doc.pdf", Some("pdf"), 2048),
            create_test_file("data.bin", Some("bin"), 512),
        ];

        let results = engine.classify_batch(&files).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0].1.is_some());
        assert_eq!(results[0].1.as_ref().unwrap().category, "images");
        assert!(results[1].1.is_some());
        assert_eq!(results[1].1.as_ref().unwrap().category, "documents");
        assert!(results[2].1.is_none());
    }
}
