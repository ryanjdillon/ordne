//! Interactive terminal UI for file classification review and override.
//!
//! This module provides an interactive classification experience where users can:
//! - Review AI-suggested categories in batches
//! - Override suggestions manually
//! - Preview files with metadata
//! - Confirm batch operations before applying

use crate::classify::rules::RuleEngine;
use crate::db::{File, Priority};
use crate::error::Result;
use console::{style, Term};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, MultiSelect, Select};
use std::collections::HashMap;

/// A batch of similar files grouped for review.
#[derive(Debug, Clone)]
pub struct ClassificationBatch {
    pub category: String,
    pub subcategory: Option<String>,
    pub files: Vec<File>,
    pub suggested_by: String,
}

impl ClassificationBatch {
    /// Calculate total size of files in batch.
    pub fn total_size(&self) -> i64 {
        self.files.iter().map(|f| f.size_bytes).sum()
    }

    /// Get file count.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Format size in human-readable form.
    pub fn format_size(bytes: i64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        if unit_idx == 0 {
            format!("{} {}", bytes, UNITS[0])
        } else {
            format!("{:.2} {}", size, UNITS[unit_idx])
        }
    }
}

/// Interactive classifier for reviewing and overriding automatic classifications.
pub struct InteractiveClassifier {
    engine: RuleEngine,
    term: Term,
    theme: ColorfulTheme,
}

impl InteractiveClassifier {
    /// Create a new interactive classifier.
    pub fn new(engine: RuleEngine) -> Self {
        Self {
            engine,
            term: Term::stdout(),
            theme: ColorfulTheme::default(),
        }
    }

    /// Create classifier with custom terminal (for testing).
    #[cfg(test)]
    pub fn with_term(engine: RuleEngine, term: Term) -> Self {
        Self {
            engine,
            term,
            theme: ColorfulTheme::default(),
        }
    }

    /// Run interactive classification session on unclassified files.
    pub fn classify_interactive(&self, files: Vec<File>) -> Result<Vec<(i64, ClassificationResult)>> {
        if files.is_empty() {
            self.term.write_line("No files to classify.")?;
            return Ok(Vec::new());
        }

        self.term.write_line(&format!(
            "{} {} files found for classification",
            style("→").cyan(),
            style(files.len()).bold()
        ))?;
        self.term.write_line("")?;

        let batches = self.group_into_batches(files)?;

        self.term.write_line(&format!(
            "Grouped into {} classification batches",
            style(batches.len()).bold()
        ))?;
        self.term.write_line("")?;

        let mut results = Vec::new();

        for (idx, batch) in batches.iter().enumerate() {
            self.term.write_line(&format!(
                "{} Batch {}/{}: {} files → {}{}",
                style("→").cyan(),
                idx + 1,
                batches.len(),
                style(batch.file_count()).bold(),
                style(&batch.category).green(),
                batch.subcategory.as_ref()
                    .map(|s| format!(" / {}", style(s).yellow()))
                    .unwrap_or_default()
            ))?;

            let batch_results = self.review_batch(batch)?;
            results.extend(batch_results);
        }

        Ok(results)
    }

    /// Group files into batches by suggested classification.
    fn group_into_batches(&self, files: Vec<File>) -> Result<Vec<ClassificationBatch>> {
        let mut batch_map: HashMap<(String, Option<String>), Vec<File>> = HashMap::new();

        for file in files {
            let classification = self.engine.classify(&file)?;

            if let Some(rule_match) = classification {
                let key = (rule_match.category.clone(), rule_match.subcategory.clone());
                batch_map.entry(key).or_default().push(file);
            }
        }

        let mut batches: Vec<ClassificationBatch> = batch_map
            .into_iter()
            .map(|((category, subcategory), files)| ClassificationBatch {
                category,
                subcategory,
                suggested_by: "rules".to_string(),
                files,
            })
            .collect();

        batches.sort_by(|a, b| b.file_count().cmp(&a.file_count()));

        Ok(batches)
    }

    /// Review a single batch and get user decisions.
    fn review_batch(&self, batch: &ClassificationBatch) -> Result<Vec<(i64, ClassificationResult)>> {
        self.show_batch_summary(batch)?;

        let action = Select::with_theme(&self.theme)
            .with_prompt("Action")
            .items(&[
                "Accept all",
                "Review individually",
                "Override category",
                "Skip batch",
            ])
            .default(0)
            .interact_on(&self.term)?;

        match action {
            0 => self.accept_all(batch),
            1 => self.review_individually(batch),
            2 => self.override_category(batch),
            3 => Ok(Vec::new()),
            _ => unreachable!(),
        }
    }

    /// Show summary information for a batch.
    fn show_batch_summary(&self, batch: &ClassificationBatch) -> Result<()> {
        self.term.write_line(&format!(
            "  {} files, total size: {}",
            style(batch.file_count()).bold(),
            style(ClassificationBatch::format_size(batch.total_size())).bold()
        ))?;

        let sample_count = batch.files.len().min(3);
        self.term.write_line(&format!("  Sample files:"))?;

        for file in batch.files.iter().take(sample_count) {
            self.term.write_line(&format!(
                "    • {} ({})",
                style(&file.filename).dim(),
                ClassificationBatch::format_size(file.size_bytes)
            ))?;
        }

        if batch.files.len() > sample_count {
            self.term.write_line(&format!(
                "    ... and {} more",
                batch.files.len() - sample_count
            ))?;
        }

        self.term.write_line("")?;
        Ok(())
    }

    /// Accept all files in batch with suggested classification.
    fn accept_all(&self, batch: &ClassificationBatch) -> Result<Vec<(i64, ClassificationResult)>> {
        let confirm = Confirm::with_theme(&self.theme)
            .with_prompt(format!(
                "Classify {} files as '{}'?",
                batch.file_count(),
                batch.category
            ))
            .default(true)
            .interact_on(&self.term)?;

        if !confirm {
            return Ok(Vec::new());
        }

        let results = batch.files.iter()
            .map(|f| {
                (
                    f.id,
                    ClassificationResult {
                        category: batch.category.clone(),
                        subcategory: batch.subcategory.clone(),
                        priority: Priority::Normal,
                    },
                )
            })
            .collect();

        self.term.write_line(&format!(
            "  {} Classified {} files",
            style("✓").green(),
            batch.file_count()
        ))?;
        self.term.write_line("")?;

        Ok(results)
    }

    /// Review files individually.
    fn review_individually(&self, batch: &ClassificationBatch) -> Result<Vec<(i64, ClassificationResult)>> {
        let mut results = Vec::new();

        for (idx, file) in batch.files.iter().enumerate() {
            self.term.write_line(&format!(
                "\n{} File {}/{}",
                style("→").cyan(),
                idx + 1,
                batch.files.len()
            ))?;
            self.show_file_details(file)?;

            let accept_msg = format!("Accept as '{}'", batch.category);
            let action = Select::with_theme(&self.theme)
                .with_prompt("Action")
                .items(&[
                    accept_msg.as_str(),
                    "Enter custom category",
                    "Skip this file",
                ])
                .default(0)
                .interact_on(&self.term)?;

            match action {
                0 => {
                    results.push((
                        file.id,
                        ClassificationResult {
                            category: batch.category.clone(),
                            subcategory: batch.subcategory.clone(),
                            priority: Priority::Normal,
                        },
                    ));
                }
                1 => {
                    if let Some(result) = self.prompt_custom_category(file)? {
                        results.push((file.id, result));
                    }
                }
                2 => continue,
                _ => unreachable!(),
            }
        }

        Ok(results)
    }

    /// Override category for entire batch.
    fn override_category(&self, batch: &ClassificationBatch) -> Result<Vec<(i64, ClassificationResult)>> {
        let category: String = Input::with_theme(&self.theme)
            .with_prompt("Category")
            .default(batch.category.clone())
            .interact_text_on(&self.term)?;

        let subcategory: String = Input::with_theme(&self.theme)
            .with_prompt("Subcategory (optional)")
            .allow_empty(true)
            .interact_text_on(&self.term)?;

        let subcategory = if subcategory.is_empty() {
            None
        } else {
            Some(subcategory)
        };

        let priority_opts = vec!["normal", "low", "critical", "trash"];
        let priority_idx = Select::with_theme(&self.theme)
            .with_prompt("Priority")
            .items(&priority_opts)
            .default(0)
            .interact_on(&self.term)?;

        let priority = Priority::from_str(priority_opts[priority_idx]).unwrap();

        let confirm = Confirm::with_theme(&self.theme)
            .with_prompt(format!(
                "Classify {} files as '{}'?",
                batch.file_count(),
                category
            ))
            .default(true)
            .interact_on(&self.term)?;

        if !confirm {
            return Ok(Vec::new());
        }

        let results = batch.files.iter()
            .map(|f| {
                (
                    f.id,
                    ClassificationResult {
                        category: category.clone(),
                        subcategory: subcategory.clone(),
                        priority,
                    },
                )
            })
            .collect();

        self.term.write_line(&format!(
            "  {} Classified {} files",
            style("✓").green(),
            batch.file_count()
        ))?;
        self.term.write_line("")?;

        Ok(results)
    }

    /// Prompt user for custom category.
    fn prompt_custom_category(&self, _file: &File) -> Result<Option<ClassificationResult>> {
        let category: String = Input::with_theme(&self.theme)
            .with_prompt("Category")
            .interact_text_on(&self.term)?;

        let subcategory: String = Input::with_theme(&self.theme)
            .with_prompt("Subcategory (optional)")
            .allow_empty(true)
            .interact_text_on(&self.term)?;

        let subcategory = if subcategory.is_empty() {
            None
        } else {
            Some(subcategory)
        };

        Ok(Some(ClassificationResult {
            category,
            subcategory,
            priority: Priority::Normal,
        }))
    }

    /// Show detailed information about a file.
    fn show_file_details(&self, file: &File) -> Result<()> {
        self.term.write_line(&format!(
            "  Path: {}",
            style(&file.path).dim()
        ))?;
        self.term.write_line(&format!(
            "  Size: {}",
            ClassificationBatch::format_size(file.size_bytes)
        ))?;

        if let Some(ref modified) = file.modified_at {
            self.term.write_line(&format!(
                "  Modified: {}",
                modified.format("%Y-%m-%d %H:%M:%S")
            ))?;
        }

        if let Some(ref ext) = file.extension {
            self.term.write_line(&format!("  Extension: {}", ext))?;
        }

        Ok(())
    }

    /// Select specific files from a list for classification.
    pub fn select_files(&self, files: &[File]) -> Result<Vec<usize>> {
        if files.is_empty() {
            return Ok(Vec::new());
        }

        let items: Vec<String> = files.iter()
            .map(|f| {
                format!(
                    "{} ({})",
                    f.filename,
                    ClassificationBatch::format_size(f.size_bytes)
                )
            })
            .collect();

        let selections = MultiSelect::with_theme(&self.theme)
            .with_prompt("Select files to classify")
            .items(&items)
            .interact_on(&self.term)?;

        Ok(selections)
    }
}

/// Result of a classification decision.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassificationResult {
    pub category: String,
    pub subcategory: Option<String>,
    pub priority: Priority,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::rules::ClassificationRules;
    use chrono::Utc;

    fn create_test_file(id: i64, filename: &str, size: i64) -> File {
        File {
            id,
            drive_id: 1,
            path: format!("/test/{}", filename),
            abs_path: format!("/test/{}", filename),
            filename: filename.to_string(),
            extension: filename.split('.').last().map(|s| s.to_string()),
            size_bytes: size,
            md5_hash: None,
            blake3_hash: None,
            created_at: None,
            modified_at: Some(Utc::now()),
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
    fn test_classification_batch() {
        let files = vec![
            create_test_file(1, "photo1.jpg", 1024),
            create_test_file(2, "photo2.jpg", 2048),
        ];

        let batch = ClassificationBatch {
            category: "photos".to_string(),
            subcategory: Some("2024".to_string()),
            files,
            suggested_by: "rules".to_string(),
        };

        assert_eq!(batch.file_count(), 2);
        assert_eq!(batch.total_size(), 3072);
    }

    #[test]
    fn test_format_size() {
        assert_eq!(ClassificationBatch::format_size(500), "500 B");
        assert_eq!(ClassificationBatch::format_size(1024), "1.00 KB");
        assert_eq!(ClassificationBatch::format_size(1048576), "1.00 MB");
        assert_eq!(ClassificationBatch::format_size(1073741824), "1.00 GB");
    }

    #[test]
    fn test_group_into_batches() {
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
        let classifier = InteractiveClassifier::new(engine);

        let files = vec![
            create_test_file(1, "photo1.jpg", 1024),
            create_test_file(2, "photo2.jpg", 2048),
            create_test_file(3, "doc.pdf", 512),
        ];

        let batches = classifier.group_into_batches(files).unwrap();
        assert_eq!(batches.len(), 2);

        let image_batch = batches.iter().find(|b| b.category == "images").unwrap();
        assert_eq!(image_batch.file_count(), 2);

        let doc_batch = batches.iter().find(|b| b.category == "documents").unwrap();
        assert_eq!(doc_batch.file_count(), 1);
    }
}
