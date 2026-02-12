# CLI Reference

`ordne` is a Rust CLI for indexing drives, finding duplicates, classifying files, and executing migration plans.

**Global Options**
- `--db <path>`: Path to the ordne database file
- `-v`, `--verbose`: Enable verbose output
- `-q`, `--quiet`: Suppress non-error output

**Help**
- `ordne --help`
- `ordne <command> --help`

**Drive Management**
```bash
ordne drive add <label> <path> --role <source|target|backup|offload> [--rclone]
ordne drive list
ordne drive info <label>
ordne drive online <label>
ordne drive offline <label>
ordne drive remove <label>
```

**Scanning**
```bash
ordne scan <drive_label> [path]
ordne scan --all
```

**Dedup Refresh**
```bash
ordne dedup refresh --drive <label> [--algorithm blake3|md5] [--rehash]
```

**rmlint Import**
```bash
ordne rmlint import <path> [--no-classify]
```

**Status**
```bash
ordne status [--space]
```

**Queries**
```bash
ordne query duplicates [--drive <label>]
ordne query unclassified [--limit <n>]
ordne query category <name>
ordne query large-files [--min-size <size>] [--limit <n>]
ordne query backup-unique
```

**Classification**
```bash
ordne classify [--config <path>] [--auto]
```

**Plans**
```bash
ordne plan create delete-trash [--category-filter <name>] [--source-drive <label>]
ordne plan create dedup --duplicate-group <id> [--original-file <id>]
ordne plan create migrate --target-drive <label> --category-filter <name> [--source-drive <label>]
ordne plan create offload --target-drive <label> --category-filter <name> [--source-drive <label>]
ordne plan list [status]
ordne plan show <id>
ordne plan approve <id>
```

Notes:
- `dedup`, `migrate`, and `offload` require additional flags as shown above.

**Migrate / Rollback**
```bash
ordne migrate <plan_id> --dry-run
ordne migrate <plan_id> --execute
ordne rollback <plan_id>
```

**Verify / Report**
```bash
ordne verify [--drive <label>]
ordne report
```

**Export**
```bash
ordne export <json|csv> [-o <path>]
```

**Policy (Draft)**
```bash
ordne policy validate <path>
ordne policy show <path>
ordne policy apply <path> [--dry-run|--execute]
```

Notes:
- `policy apply` creates plans; with `--dry-run` or `--execute` it will run them.

**Run Policy**
```bash
ordne run-policy <path> --dry-run
ordne run-policy <path> --execute
```

Notes:
- `run-policy` applies classification rules to unclassified files in scope before creating plans.
