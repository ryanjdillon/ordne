# Policy Schema (Draft)

This document defines the draft policy format used to capture agent session outcomes and run them later in batch mode.

**File Locations (Merge Order)**
1. `~/.config/ordne/ordne.toml` (global defaults)
2. `<drive_or_project_root>/.ordne/ordne.toml` (drive/project override)
3. Explicit policy file passed to CLI/MCP

`ordne run-policy` applies classification rules to unclassified files in scope before creating plans.

**Top-Level Fields**
- `version`: Schema version string
- `name`: Human-readable policy name
- `description`: Short description of intent
- `scope`: Drive and path scoping rules
- `classification`: Rules and defaults
- `plans`: Migration intents
- `safety`: Guardrails and approvals
- `schedule`: Optional recurrence metadata

## Example

```toml
version = "0.1"
name = "weekly-archive-cleanup"
description = "Weekly cleanup + archive migration"

[scope]
include_drives = ["nas_main", "archive_usb"]
exclude_drives = ["backup_wd_2tb"]
include_paths = ["/mnt/nas/Photos", "/mnt/nas/Documents"]
exclude_paths = ["/mnt/nas/tmp", "/mnt/nas/Downloads"]

[classification]
default_priority = "normal"

[rules.trash]
type = "pattern"
patterns = ["**/node_modules/**"]
category = "trash"
priority = "trash"

[plans.delete_trash]
type = "delete-trash"
description = "Delete confirmed trash"
category_filter = "trash"

[plans.migrate_archives]
type = "migrate"
description = "Move archives to target"
source_drive = "nas_main"
target_drive = "archive_usb"
category_filter = "archive"

[plans.dedup_examples]
type = "dedup"
duplicate_group = 42
original_file = 1234

[safety]
require_approval = true
max_bytes_per_run = "50GB"
dry_run_only = false

[schedule]
cron = "0 3 * * 1"
timezone = "UTC"
```

## Field Details

**scope**
- `include_drives`: Only scan these drives
- `exclude_drives`: Skip these drives
- `include_paths`: Optional path allowlist
- `exclude_paths`: Optional path blocklist

**classification**
- `default_priority`: Fallback priority

**rules**
- Same structure as existing classification rules (`[rules.<name>]`)

**plans**
- `[plans.<name>]` tables
- `type`: One of `delete-trash`, `dedup`, `migrate`, `offload`
- `source_drive`: Optional source drive
- `target_drive`: Optional target drive
- `category_filter`: Optional category filter
- `duplicate_group`: Required for `dedup` plans
- `original_file`: Optional for `dedup` plans; required if no original is marked

**safety**
- `require_approval`: Blocks execution unless approved
- `max_bytes_per_run`: Cap for automated runs
- `dry_run_only`: Force dry-run

**schedule**
- `cron`: Cron expression for external schedulers
- `timezone`: Timezone string

## Scheduling Examples

### Cron (weekly, Monday 3am)

```cron
0 3 * * 1 ordne run-policy ~/.config/ordne/policies/weekly-archive-cleanup.toml --execute
```

### systemd timer

`~/.config/systemd/user/ordne-policy.service`
```ini
[Unit]
Description=Run ordne policy

[Service]
Type=oneshot
ExecStart=%h/.nix-profile/bin/ordne run-policy %h/.config/ordne/policies/weekly-archive-cleanup.toml --execute
```

`~/.config/systemd/user/ordne-policy.timer`
```ini
[Unit]
Description=Weekly ordne policy run

[Timer]
OnCalendar=Mon *-*-* 03:00:00
Persistent=true

[Install]
WantedBy=timers.target
```
