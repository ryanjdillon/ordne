# Classification Module

Automatic and interactive file classification based on configurable rules.

## Features

- **Multiple Rule Types**: Pattern, extension, size, age, duplicate detection
- **EXIF Support**: Organize photos by date/camera from EXIF metadata
- **Priority System**: Configurable rule evaluation order
- **Interactive UI**: Terminal-based batch review with dialoguer
- **Batch Operations**: Efficient bulk classification
- **Database Integration**: Direct SQL operations for performance

## Quick Example

```rust
use ordne_lib::classify::{ClassificationRules, RuleEngine};

let toml = r#"
    [rules.photos]
    type = "extension"
    extensions = ["jpg", "jpeg"]
    category = "photos"
    subcategory_from_exif = "{exif_year}/{exif_month}"
"#;

let rules = ClassificationRules::from_toml(toml)?;
let engine = RuleEngine::new(rules)?;

let file = get_file_from_db()?;
if let Some(classification) = engine.classify(&file)? {
    println!("File {} → {}", file.path, classification.category);
}
```

## Module Structure

- `rules.rs` - Rules engine and EXIF extraction
- `interactive.rs` - Terminal UI for interactive classification
- `mod.rs` - Public API exports

## See Also

- `/classification.toml.example` - Complete configuration example
- `/docs/CLASSIFICATION_GUIDE.md` - Detailed usage guide
- `/tests/integration/classify_test.rs` - Integration test examples
