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
ordne plan create <plan_type> [source_drive] [target_drive] [category_filter]
ordne plan list [status]
ordne plan show <id>
ordne plan approve <id>
```

Notes:
- `plan create` currently supports `delete-trash` only.
- `dedup`, `migrate`, and `offload` plan creation are planned.
- Positional arguments after `<plan_type>` are reserved; only the `category_filter` is used by `delete-trash` today.

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
