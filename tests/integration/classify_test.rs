use ordne_lib::classify::{ClassificationRules, RuleEngine};
use ordne_lib::db::{File, FileStatus, Priority};
use chrono::{Utc, Duration};

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
        status: FileStatus::Indexed,
        migrated_to: None,
        migrated_to_drive: None,
        migrated_at: None,
        verified_hash: None,
        error: None,
        indexed_at: Utc::now(),
    }
}

#[test]
fn test_classify_node_modules() {
    let toml = r#"
        [rules.node_modules]
        type = "pattern"
        patterns = ["**/node_modules/**"]
        category = "trash"
        priority = "trash"
        rule_priority = 100
    "#;

    let rules = ClassificationRules::from_toml(toml).unwrap();
    let engine = RuleEngine::new(rules).unwrap();

    let file = create_test_file("project/node_modules/package/index.js", Some("js"), 1024);
    let result = engine.classify(&file).unwrap();

    assert!(result.is_some());
    let matched = result.unwrap();
    assert_eq!(matched.category, "trash");
    assert_eq!(matched.priority, Priority::Trash);
}

#[test]
fn test_classify_by_extension() {
    let toml = r#"
        [rules.photos]
        type = "extension"
        extensions = ["jpg", "jpeg", "png"]
        category = "photos"
        priority = "normal"
        rule_priority = 60
    "#;

    let rules = ClassificationRules::from_toml(toml).unwrap();
    let engine = RuleEngine::new(rules).unwrap();

    let photo = create_test_file("vacation/photo.jpg", Some("jpg"), 2048576);
    let result = engine.classify(&photo).unwrap();

    assert!(result.is_some());
    let matched = result.unwrap();
    assert_eq!(matched.category, "photos");
}

#[test]
fn test_classify_by_size() {
    let toml = r#"
        [rules.large_files]
        type = "size"
        min_bytes = 1073741824
        category = "large"
        priority = "low"
        rule_priority = 80
    "#;

    let rules = ClassificationRules::from_toml(toml).unwrap();
    let engine = RuleEngine::new(rules).unwrap();

    let small_file = create_test_file("small.txt", Some("txt"), 1024);
    let large_file = create_test_file("movie.mp4", Some("mp4"), 2147483648);

    assert!(engine.classify(&small_file).unwrap().is_none());

    let result = engine.classify(&large_file).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "large");
}

#[test]
fn test_classify_by_age() {
    let toml = r#"
        [rules.old_files]
        type = "age"
        older_than_days = 7
        category = "old"
        priority = "low"
        rule_priority = 30
    "#;

    let rules = ClassificationRules::from_toml(toml).unwrap();
    let engine = RuleEngine::new(rules).unwrap();

    let file = create_test_file("old_doc.pdf", Some("pdf"), 1024);
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

    let file = create_test_file("readme.txt", Some("txt"), 1024);
    let result = engine.classify(&file).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().category, "text_files");
}

#[test]
fn test_batch_classification() {
    let toml = r#"
        [rules.photos]
        type = "extension"
        extensions = ["jpg"]
        category = "photos"

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
    assert_eq!(results[0].1.as_ref().unwrap().category, "photos");

    assert!(results[1].1.is_some());
    assert_eq!(results[1].1.as_ref().unwrap().category, "documents");

    assert!(results[2].1.is_none());
}
