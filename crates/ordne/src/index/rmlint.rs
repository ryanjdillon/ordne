use crate::error::{Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RmlintLintType {
    #[serde(rename = "duplicate_file")]
    DuplicateFile,
    #[serde(rename = "duplicate_dir")]
    DuplicateDir,
    #[serde(rename = "emptydir")]
    EmptyDir,
    #[serde(rename = "emptyfile")]
    EmptyFile,
    #[serde(rename = "nonstripped")]
    NonStripped,
    #[serde(rename = "badlink")]
    BadLink,
    #[serde(rename = "baduid")]
    BadUid,
    #[serde(rename = "badgid")]
    BadGid,
    #[serde(other)]
    Other,
}

impl RmlintLintType {
    pub fn as_str(&self) -> &'static str {
        match self {
            RmlintLintType::DuplicateFile => "duplicate_file",
            RmlintLintType::DuplicateDir => "duplicate_dir",
            RmlintLintType::EmptyDir => "emptydir",
            RmlintLintType::EmptyFile => "emptyfile",
            RmlintLintType::NonStripped => "nonstripped",
            RmlintLintType::BadLink => "badlink",
            RmlintLintType::BadUid => "baduid",
            RmlintLintType::BadGid => "badgid",
            RmlintLintType::Other => "other",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RmlintLint {
    #[serde(rename = "type")]
    pub lint_type: RmlintLintType,
    pub path: PathBuf,
    pub size: i64,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub is_original: bool,
    #[serde(default)]
    pub depth: Option<i32>,
    #[serde(default)]
    pub inode: Option<i64>,
    #[serde(default)]
    pub disk_id: Option<i64>,
    #[serde(default)]
    pub mtime: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct DuplicateGroup {
    pub hash: String,
    pub files: Vec<RmlintLint>,
    pub total_size: i64,
    pub original_idx: Option<usize>,
}

pub struct RmlintParser {
    lints: Vec<RmlintLint>,
}

impl RmlintParser {
    pub fn new() -> Self {
        Self { lints: Vec::new() }
    }

    /// Parses rmlint JSON output from a file
    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        self.parse_string(&content)
    }

    /// Parses rmlint JSON output from a string
    pub fn parse_string(&mut self, content: &str) -> Result<()> {
        let lines: Vec<&str> = content.lines().collect();

        for line in lines {
            let line = line.trim();
            if line.is_empty() || line.starts_with("//") {
                continue;
            }

            if let Ok(lint) = serde_json::from_str::<RmlintLint>(line) {
                self.lints.push(lint);
            }
        }

        Ok(())
    }

    /// Returns all parsed lints
    pub fn lints(&self) -> &[RmlintLint] {
        &self.lints
    }

    /// Extracts duplicate file groups
    ///
    /// Groups files by their checksum hash and identifies originals
    pub fn extract_duplicate_groups(&self) -> Vec<DuplicateGroup> {
        let mut groups: HashMap<String, Vec<RmlintLint>> = HashMap::new();

        for lint in &self.lints {
            if lint.lint_type == RmlintLintType::DuplicateFile {
                if let Some(checksum) = &lint.checksum {
                    groups
                        .entry(checksum.clone())
                        .or_insert_with(Vec::new)
                        .push(lint.clone());
                }
            }
        }

        groups
            .into_iter()
            .filter(|(_, files)| files.len() > 1)
            .map(|(hash, files)| {
                let total_size = files.first().map(|f| f.size).unwrap_or(0);
                let original_idx = files.iter().position(|f| f.is_original);

                DuplicateGroup {
                    hash,
                    files,
                    total_size,
                    original_idx,
                }
            })
            .collect()
    }

    /// Checks if duplicate groups span multiple drives
    ///
    /// Returns true if any duplicate group contains files from different disk_id values
    pub fn has_cross_drive_duplicates(&self) -> bool {
        let groups = self.extract_duplicate_groups();

        for group in groups {
            let disk_ids: Vec<i64> = group
                .files
                .iter()
                .filter_map(|f| f.disk_id)
                .collect();

            if disk_ids.len() > 1 {
                let first = disk_ids[0];
                if disk_ids.iter().any(|&id| id != first) {
                    return true;
                }
            }
        }

        false
    }

    /// Returns statistics about the parsed lints
    pub fn statistics(&self) -> RmlintStatistics {
        let mut stats = RmlintStatistics::default();

        for lint in &self.lints {
            match lint.lint_type {
                RmlintLintType::DuplicateFile => {
                    stats.duplicate_files += 1;
                    if !lint.is_original {
                        stats.duplicate_size += lint.size;
                    }
                }
                RmlintLintType::EmptyFile => stats.empty_files += 1,
                RmlintLintType::EmptyDir => stats.empty_dirs += 1,
                _ => stats.other_lints += 1,
            }
        }

        stats.duplicate_groups = self.extract_duplicate_groups().len();

        stats
    }
}

impl Default for RmlintParser {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default, Clone)]
pub struct RmlintStatistics {
    pub duplicate_files: usize,
    pub duplicate_groups: usize,
    pub duplicate_size: i64,
    pub empty_files: usize,
    pub empty_dirs: usize,
    pub other_lints: usize,
}

/// Convenience function to parse rmlint output from a file
pub fn parse_rmlint_output<P: AsRef<Path>>(path: P) -> Result<RmlintParser> {
    let mut parser = RmlintParser::new();
    parser.parse_file(path)?;
    Ok(parser)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_duplicate_file() {
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true,"depth":1,"inode":12345,"disk_id":1}"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        assert_eq!(parser.lints().len(), 1);
        let lint = &parser.lints()[0];
        assert_eq!(lint.lint_type, RmlintLintType::DuplicateFile);
        assert_eq!(lint.size, 1024);
        assert_eq!(lint.checksum, Some("abc123".to_string()));
        assert!(lint.is_original);
    }

    #[test]
    fn test_parse_multiple_lints() {
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/tmp/file2.txt","size":1024,"checksum":"abc123","is_original":false}
{"type":"emptyfile","path":"/tmp/empty.txt","size":0}
"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        assert_eq!(parser.lints().len(), 3);
    }

    #[test]
    fn test_extract_duplicate_groups() {
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/tmp/file2.txt","size":1024,"checksum":"abc123","is_original":false}
{"type":"duplicate_file","path":"/tmp/file3.txt","size":2048,"checksum":"def456","is_original":true}
{"type":"duplicate_file","path":"/tmp/file4.txt","size":2048,"checksum":"def456","is_original":false}
"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        let groups = parser.extract_duplicate_groups();
        assert_eq!(groups.len(), 2);

        let group1 = groups.iter().find(|g| g.hash == "abc123").unwrap();
        assert_eq!(group1.files.len(), 2);
        assert_eq!(group1.total_size, 1024);
        assert!(group1.original_idx.is_some());

        let group2 = groups.iter().find(|g| g.hash == "def456").unwrap();
        assert_eq!(group2.files.len(), 2);
        assert_eq!(group2.total_size, 2048);
    }

    #[test]
    fn test_cross_drive_duplicates() {
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true,"disk_id":1}
{"type":"duplicate_file","path":"/mnt/file2.txt","size":1024,"checksum":"abc123","is_original":false,"disk_id":2}
"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        assert!(parser.has_cross_drive_duplicates());
    }

    #[test]
    fn test_statistics() {
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/tmp/file2.txt","size":1024,"checksum":"abc123","is_original":false}
{"type":"emptyfile","path":"/tmp/empty.txt","size":0}
{"type":"emptydir","path":"/tmp/emptydir","size":0}
"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        let stats = parser.statistics();
        assert_eq!(stats.duplicate_files, 2);
        assert_eq!(stats.duplicate_groups, 1);
        assert_eq!(stats.duplicate_size, 1024);
        assert_eq!(stats.empty_files, 1);
        assert_eq!(stats.empty_dirs, 1);
    }

    #[test]
    fn test_parse_from_file() {
        let mut temp_file = NamedTempFile::new().unwrap();
        let json = r#"{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
{"type":"duplicate_file","path":"/tmp/file2.txt","size":1024,"checksum":"abc123","is_original":false}
"#;
        temp_file.write_all(json.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let parser = parse_rmlint_output(temp_file.path()).unwrap();
        assert_eq!(parser.lints().len(), 2);
    }

    #[test]
    fn test_skip_comments() {
        let json = r#"// This is a comment
{"type":"duplicate_file","path":"/tmp/file1.txt","size":1024,"checksum":"abc123","is_original":true}
// Another comment

{"type":"emptyfile","path":"/tmp/empty.txt","size":0}
"#;

        let mut parser = RmlintParser::new();
        parser.parse_string(json).unwrap();

        assert_eq!(parser.lints().len(), 2);
    }
}
