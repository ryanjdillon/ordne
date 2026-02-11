# Classification Module Usage Guide

## Overview

The classification module provides automatic and interactive file classification based on configurable rules. It supports pattern matching, file attributes, EXIF metadata, and custom categorization strategies.

## Quick Start

### 1. Configuration

Create a configuration file at `~/.config/ordne/ordne.toml` (see `/ordne.toml.example` for a complete example):

```toml
[rules.trash]
type = "pattern"
patterns = ["**/node_modules/**", "**/.cache/**"]
category = "trash"
priority = "trash"
rule_priority = 100

[rules.photos]
type = "extension"
extensions = ["jpg", "jpeg", "heic"]
category = "photos"
subcategory_from_exif = "{exif_year}/{exif_month}"
priority = "normal"
rule_priority = 60
```

### 2. Programmatic Usage

```rust
use ordne_lib::classify::{ClassificationRules, RuleEngine};
use ordne_lib::db::{SqliteDatabase, Database, FileStatus};

// Load rules
let rules = ClassificationRules::from_file("ordne.toml")?;
let engine = RuleEngine::new(rules)?;

// Open database
let mut db = SqliteDatabase::open("ordne.db")?;
db.initialize()?;

// Get unclassified files
let files = ordne_lib::db::files::get_unclassified_files(db.conn(), None)?;

// Classify files
for file in files {
    if let Some(rule_match) = engine.classify(&file)? {
        ordne_lib::db::files::update_file_classification(
            db.conn_mut(),
            file.id,
            &rule_match.category,
            rule_match.subcategory.as_deref(),
            rule_match.priority,
        )?;
    }
}
```

### 3. Interactive Classification

```rust
use ordne_lib::classify::{InteractiveClassifier, ClassificationRules, RuleEngine};

let rules = ClassificationRules::from_file("ordne.toml")?;
let engine = RuleEngine::new(rules)?;
let classifier = InteractiveClassifier::new(engine);

// Get unclassified files from database
let files = get_unclassified_files(&db)?;

// Run interactive session
let results = classifier.classify_interactive(files)?;

// Apply classifications
for (file_id, classification) in results {
    update_file_classification(
        db.conn_mut(),
        file_id,
        &classification.category,
        classification.subcategory.as_deref(),
        classification.priority,
    )?;
}
```

## Rule Types

### Pattern Matching

Match files by glob patterns:

```toml
[rules.node_modules]
type = "pattern"
patterns = ["**/node_modules/**", "**/dist/**"]
category = "trash"
rule_priority = 100
```

Supports standard glob syntax:
- `*` - Any characters except `/`
- `**` - Any characters including `/`
- `?` - Single character
- `[abc]` - Character class

### Extension Matching

Match files by extension (case-insensitive):

```toml
[rules.images]
type = "extension"
extensions = ["jpg", "png", "gif"]
category = "images"
```

### Size-Based Rules

Match files by size thresholds:

```toml
[rules.large]
type = "size"
min_bytes = 1073741824  # 1GB
category = "large_files"

[rules.medium]
type = "size"
min_bytes = 104857600   # 100MB
max_bytes = 1073741824  # 1GB
category = "medium_files"
```

### Age-Based Rules

Match files by modification time:

```toml
[rules.old]
type = "age"
older_than_days = 365
category = "archive_candidates"

[rules.recent]
type = "age"
newer_than_days = 7
category = "recent_files"
```

### Duplicate Handling

Match duplicate files with keep strategy:

```toml
[rules.duplicates]
type = "duplicate"
keep_strategy = "keep_oldest"
category = "duplicates"
```

Strategies:
- `keep_oldest` - Keep file with oldest modification time
- `keep_newest` - Keep file with newest modification time
- `keep_original` - Keep file marked as original

## EXIF-Based Organization

For photo organization, use EXIF metadata:

```toml
[rules.photos]
type = "extension"
extensions = ["jpg", "jpeg", "heic"]
category = "photos"
subcategory_from_exif = "{exif_year}/{exif_month}"
priority = "normal"
```

Available EXIF placeholders:
- `{exif_year}` - Year (e.g., "2024")
- `{exif_month}` - Month, zero-padded (e.g., "03")
- `{exif_day}` - Day, zero-padded (e.g., "15")
- `{exif_make}` - Camera manufacturer (e.g., "Canon")
- `{exif_model}` - Camera model (e.g., "EOS 5D")
- `{filename}` - Original filename

Example subcategory patterns:
- `"{exif_year}/{exif_month}"` → "2024/03"
- `"{exif_year}/{exif_make}"` → "2024/Canon"
- `"{exif_year}/{exif_month}/{exif_day}"` → "2024/03/15"

If EXIF data is missing, the rule's regular `subcategory` field is used instead (if present).

## Priority System

Rules are evaluated by `rule_priority` (highest first). Higher priority rules take precedence when multiple rules match.

Recommended priorities:
- **100**: Critical trash (node_modules, cache)
- **80-90**: Size-based and build artifacts
- **70-75**: Git repositories
- **55-65**: Media files (photos, videos)
- **50**: Documents, code
- **30-40**: Age-based and duplicates

File priorities:
- `critical` - Important files, high retention
- `normal` - Regular files
- `low` - Less important, archival candidates
- `trash` - Safe to delete (after review)

## Database Operations

### Query Unclassified Files

```rust
use ordne_lib::db::files::get_unclassified_files;

// Get all unclassified
let files = get_unclassified_files(conn, None)?;

// Get first 100
let files = get_unclassified_files(conn, Some(100))?;
```

### Query by Category

```rust
use ordne_lib::db::files::get_files_by_category;

let photos = get_files_by_category(conn, "photos")?;
```

### Bulk Classification

```rust
use ordne_lib::db::files::bulk_update_classifications;
use ordne_lib::db::Priority;

let classifications = vec![
    (file_id_1, "photos".to_string(), Some("2024/03".to_string()), Priority::Normal),
    (file_id_2, "photos".to_string(), Some("2024/03".to_string()), Priority::Normal),
];

let count = bulk_update_classifications(conn, &classifications)?;
println!("Classified {} files", count);
```

### Category Statistics

```rust
use ordne_lib::db::files::get_category_stats;

let stats = get_category_stats(conn)?;
for stat in stats {
    println!("{}/{}: {} files, {} bytes",
        stat.category,
        stat.subcategory.unwrap_or_default(),
        stat.file_count,
        stat.total_bytes
    );
}
```

## Interactive Mode Features

The interactive classifier provides:

### Batch Grouping

Files are grouped by suggested classification for efficient review:

```
→ Batch 1/3: 15 files → photos / 2024/03
  15 files, total size: 45.2 MB
  Sample files:
    • IMG_0123.jpg (3.2 MB)
    • IMG_0124.jpg (2.8 MB)
    • IMG_0125.jpg (3.5 MB)
    ... and 12 more

Action:
  1. Accept all
  2. Review individually
  3. Override category
  4. Skip batch
```

### Review Options

1. **Accept all**: Apply suggested classification to all files in batch
2. **Review individually**: Go through files one by one
3. **Override category**: Change category for entire batch
4. **Skip batch**: Move to next batch

### Individual Review

When reviewing individually:
- View file details (path, size, modified date, extension)
- Accept suggested category
- Enter custom category
- Skip file

## Best Practices

### 1. Rule Organization

Order rules by specificity:
- Most specific patterns first (high priority)
- General patterns last (low priority)

```toml
# Specific: node_modules (priority 100)
[rules.node_modules]
type = "pattern"
patterns = ["**/node_modules/**"]
category = "trash"
rule_priority = 100

# General: all JS files (priority 50)
[rules.javascript]
type = "extension"
extensions = ["js"]
category = "code"
rule_priority = 50
```

### 2. Testing Rules

Test rules on a small dataset first:

```rust
let rules = ClassificationRules::from_file("ordne.toml")?;
let engine = RuleEngine::new(rules)?;

// Test single file
let file = create_test_file();
match engine.classify(&file)? {
    Some(m) => println!("Matched: {} → {}", file.path, m.category),
    None => println!("No match: {}", file.path),
}
```

### 3. Incremental Classification

Classify in batches to avoid overwhelming decisions:

```rust
// Classify 100 files at a time
let files = get_unclassified_files(conn, Some(100))?;
let results = classifier.classify_interactive(files)?;
```

### 4. Review Statistics

Regularly review classification statistics:

```rust
let stats = get_category_stats(conn)?;
for stat in stats {
    let size_mb = stat.total_bytes as f64 / 1_048_576.0;
    println!("{}: {} files ({:.2} MB)",
        stat.category, stat.file_count, size_mb);
}
```

## Troubleshooting

### No EXIF Data Found

Photos without EXIF data will use the regular `subcategory` field instead of `subcategory_from_exif`. Consider adding:

```toml
[rules.photos_no_exif]
type = "extension"
extensions = ["jpg", "jpeg"]
category = "photos"
subcategory = "no_exif"  # Fallback subcategory
rule_priority = 55  # Lower than EXIF rule
```

### Pattern Not Matching

Test glob patterns:

```rust
use globset::Glob;

let pattern = "**/.cache/**";
let glob = Glob::new(pattern)?;
let matcher = glob.compile_matcher();

println!("Matches: {}", matcher.is_match("/home/user/.cache/file.txt"));
```

### Performance Issues

For large file sets:
1. Use batch classification: `classify_batch()`
2. Use limits: `get_unclassified_files(conn, Some(1000))`
3. Optimize rule order (high priority = evaluated first)
4. Cache rule engine instance

### Conflicting Rules

Rules are resolved by `rule_priority`. Check priorities:

```rust
for rule in rules.sorted_rules() {
    println!("{}: priority {}", rule.name, rule.rule_priority);
}
```

## Examples

See `/tests/integration/classify_test.rs` for complete examples of:
- Pattern-based classification
- Extension-based classification
- Size-based classification
- Age-based classification
- Rule priority resolution
- Batch operations

## API Reference

### Core Types

```rust
pub struct ClassificationRules
pub struct RuleEngine
pub struct InteractiveClassifier
pub struct ClassificationBatch
pub struct RuleMatch
pub struct ExifData
```

### Key Functions

```rust
// Loading rules
ClassificationRules::from_file(path) -> Result<Self>
ClassificationRules::from_toml(toml_str) -> Result<Self>

// Classification
RuleEngine::new(rules) -> Result<Self>
RuleEngine::classify(&self, file) -> Result<Option<RuleMatch>>
RuleEngine::classify_batch(&self, files) -> Result<Vec<(i64, Option<RuleMatch>)>>

// Interactive
InteractiveClassifier::new(engine) -> Self
InteractiveClassifier::classify_interactive(&self, files) -> Result<Vec<(i64, ClassificationResult)>>

// EXIF
extract_exif_data<P: AsRef<Path>>(path) -> Result<Option<ExifData>>
substitute_exif_pattern(pattern, exif) -> String
```

## Configuration Reference

See `/ordne.toml.example` for a production-ready configuration with:
- 20+ rule examples
- All rule types demonstrated
- Recommended priorities
- EXIF patterns
- Best practices

---

**Module Status**: In progress
**Test Coverage**: See CI
**Documentation**: In progress
