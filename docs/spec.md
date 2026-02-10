# ordne — Safe File Deduplication, Classification & Migration

> *Carefully cut away the dead weight. Keep what matters.*

## Project Overview

**ordne** is an open-source Rust CLI tool for safely deduplicating, classifying, and restructuring large file collections. It builds a queryable index of your files, identifies duplicates and waste, helps you classify what to keep vs. archive vs. trash, and then executes verified migrations — never deleting a file until its copy is confirmed safe.

Designed for the common scenario: years of accumulated data across drives, full of duplicates and no coherent structure, that you want to clean up before migrating to new storage (ZFS, NAS, cloud, etc.).

**Language:** Rust (single static binary, no runtime deps)  
**Agent runtime:** Claude Code (for AI-assisted classification and migration)  
**License:** MIT  

### Design Principles

1. **Never lose data** — every destructive operation is hash-verified, logged, and reversible
2. **Index first, act later** — build a complete picture before changing anything
3. **Propose, then execute** — dry-run by default, explicit opt-in for changes
4. **Incremental** — works in batches, respects disk space constraints, resumable
5. **Composable** — wraps proven tools (rmlint, rsync, b3sum) rather than reimplementing them
5. **Well tested** — components are written to be tested well, and they are all well tested

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Claude Code Agent                       │
│  (proposes plans, executes approved steps, logs everything) │
└────────────┬──────────────────────────┬─────────────────────┘
             │                          │
     ┌───────▼───────┐         ┌───────▼────────┐
     │  Index DB     │         │  Migration     │
     │  (SQLite)     │         │  Engine        │
     │               │         │  (rsync +      │
     │  - file meta  │         │   verify)      │
     │  - hashes     │         │                │
     │  - classes    │         │                │
     │  - move log   │         │                │
     └───────┬───────┘         └───────┬────────┘
             │                         │
     ┌───────▼─────────────────────────▼─────────────────┐
     │                  CLI: ordne                       │
     │  Subcommands: scan, query, classify, plan,        │
     │               migrate, verify, rollback, status   │
     └───────────────────────────────────────────────────┘
```

---

## State & Configuration

### DB Location

`ordne` follows the XDG Base Directory specification:

```
Default:    $XDG_DATA_HOME/ordne/ordne.db
            (~/.local/share/ordne/ordne.db)

Override:   --db /path/to/ordne.db
            ORDNE_DB=/path/to/ordne.db

Config:     $XDG_CONFIG_HOME/ordne/ordne.toml
            (~/.config/ordne/ordne.toml)
```

The DB is a single SQLite file — portable, backupable, inspectable with any SQLite client. If you want to start fresh, delete the file. If you want to move the project state to another machine, copy the file.

For users managing multiple independent cleanup projects (e.g. "my NAS" vs "my laptop"), separate DB files can be used via `--db`.

### NixOS & Installation

```nix
# flake.nix (for NixOS / nix users)
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    ordne.url = "github:youruser/ordne";
  };

  outputs = { self, nixpkgs, ordne, ... }: {
    # Add to system packages or home-manager
    # ordne.packages.${system}.default

    # Or use the overlay
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        ({ pkgs, ... }: {
          environment.systemPackages = [
            ordne.packages.x86_64-linux.default
            pkgs.rmlint    # available in nixpkgs
            pkgs.rsync     # available in nixpkgs
            pkgs.rclone    # available in nixpkgs (cloud backends)
          ];
        })
      ];
    };
  };
}
```

```nix
# For home-manager users
{ pkgs, inputs, ... }: {
  home.packages = [
    inputs.ordne.packages.${pkgs.system}.default
  ];

  # Optional: manage ordne config declaratively
  xdg.configFile."ordne/ordne.toml".source = ./ordne.toml;
}
```

**Non-NixOS installation:**

```bash
# From crates.io
cargo install ordne

# From pre-built binaries (GitHub Releases)
curl -sSL https://github.com/youruser/ordne/releases/latest/download/ordne-x86_64-linux | install -m 755 /dev/stdin ~/.local/bin/ordne

# System deps (Debian/Ubuntu)
sudo apt install rmlint rsync rclone

# System deps (Arch)
sudo pacman -S rmlint rsync rclone

# System deps (macOS)
brew install rmlint rsync rclone
```

The Nix flake should also provide a dev shell with all dependencies (rmlint, rsync, rclone, b3sum) for contributors.

---

## Phase 1: Indexing

### 1.1 Tools

**Primary: `rmlint`** for deduplication detection.
- Outputs `rmlint.json` with full metadata for every file including duplicate groups
- Handles hardlinks, symlinks, empty dirs, partial hashing
- Supports `--rank-by` for choosing which duplicate to keep
- Supports `--replay` to re-filter previous results without re-scanning
- Can use `--with-metadata-cache` (SQLite) for large datasets

**Secondary: custom SQLite ingestion** to parse rmlint output + enrich with classification metadata.

**For old backup drives:** Run rmlint with the NAS as the "tagged" (preferred) path and each backup drive as secondary. This finds files that exist on backups but are missing from the NAS.

### 1.2 Schema

```sql
-- Drive / volume tracking
CREATE TABLE drives (
    id              INTEGER PRIMARY KEY,
    label           TEXT NOT NULL UNIQUE,       -- user-friendly name: "nas_main", "backup_wd_2tb"
    device_id       TEXT,                       -- /dev/disk/by-id/... (stable across reboots)
    device_path     TEXT,                       -- /dev/sda1 (may change)
    uuid            TEXT,                       -- filesystem UUID (blkid)
    mount_path      TEXT,                       -- /mnt/nas, /mnt/backup
    fs_type         TEXT,                       -- ext4, btrfs, zfs, ntfs, etc.
    total_bytes     INTEGER,
    role            TEXT DEFAULT 'source',
    -- 'source'    = being indexed/cleaned
    -- 'target'    = migration destination (e.g. ZFS pool)
    -- 'backup'    = old backup drive (read-only reference)
    -- 'offload'   = temporary holding (spare drive, cloud)
    is_online       BOOLEAN DEFAULT 1,          -- can be toggled when drive disconnected
    is_readonly     BOOLEAN DEFAULT 0,
    -- Cloud/remote backends (via rclone)
    backend         TEXT DEFAULT 'local',   -- 'local' or 'rclone'
    rclone_remote   TEXT,                   -- e.g. 'b2:my-archive', 'gdrive:backup'
    scanned_at      TEXT,
    added_at        TEXT DEFAULT (datetime('now'))
);

CREATE TABLE files (
    id              INTEGER PRIMARY KEY,
    drive_id        INTEGER NOT NULL REFERENCES drives(id),
    path            TEXT NOT NULL,               -- relative to drive mount_path
    abs_path        TEXT NOT NULL,               -- full absolute path at index time
    filename        TEXT NOT NULL,
    extension       TEXT,
    size_bytes      INTEGER NOT NULL,
    md5_hash        TEXT,               -- NULL until hashed (MD5 default, universal across local/S3/GDrive)
    blake3_hash     TEXT,               -- optional, opt-in via --algorithm blake3
    created_at      TEXT,               -- ISO 8601
    modified_at     TEXT,               -- ISO 8601
    inode           INTEGER,
    device_num      INTEGER,            -- st_dev (for cross-device detection)
    nlinks          INTEGER,            -- hardlink count
    mime_type       TEXT,
    is_symlink      BOOLEAN DEFAULT 0,
    symlink_target  TEXT,
    git_remote_url  TEXT,               -- extracted from .git/config for repos

    -- Classification
    category        TEXT,               -- 'archive', 'active', 'backup', 'trash', 'unknown'
    subcategory     TEXT,               -- 'photos', 'documents', 'projects', 'media', etc.
    target_path     TEXT,               -- where this file should end up
    target_drive_id INTEGER REFERENCES drives(id),  -- which drive it should end up on
    priority        TEXT DEFAULT 'normal', -- 'critical', 'normal', 'low', 'trash'

    -- Dedup
    duplicate_group INTEGER,            -- NULL if unique, group ID if duplicate
    is_original     BOOLEAN DEFAULT 0,  -- TRUE if this is the "keep" copy
    rmlint_type     TEXT,               -- rmlint lint type (e.g. 'duplicate_file')

    -- Migration tracking
    status          TEXT DEFAULT 'indexed',
    -- 'indexed' -> 'classified' -> 'planned' -> 'migrating' -> 'verified' -> 'source_removed'
    migrated_to     TEXT,               -- actual destination path after move
    migrated_to_drive INTEGER REFERENCES drives(id),
    migrated_at     TEXT,
    verified_hash   TEXT,               -- hash of file at destination
    error           TEXT,               -- any error encountered

    indexed_at      TEXT DEFAULT (datetime('now')),

    UNIQUE(drive_id, path)              -- same relative path on same drive = same entry
);

CREATE TABLE duplicate_groups (
    group_id        INTEGER PRIMARY KEY,
    hash            TEXT NOT NULL,
    file_count      INTEGER,
    total_waste_bytes INTEGER,          -- (file_count - 1) * size
    original_id     INTEGER REFERENCES files(id),
    -- Cross-drive awareness
    drives_involved TEXT,               -- JSON array of drive_ids, e.g. [1, 3]
    cross_drive     BOOLEAN DEFAULT 0,  -- TRUE if duplicates span multiple drives
    resolution      TEXT                -- 'pending', 'auto_resolved', 'user_resolved'
);

CREATE TABLE migration_plans (
    id              INTEGER PRIMARY KEY,
    created_at      TEXT DEFAULT (datetime('now')),
    description     TEXT,
    source_drive_id INTEGER REFERENCES drives(id),
    target_drive_id INTEGER REFERENCES drives(id),
    status          TEXT DEFAULT 'draft',
    -- 'draft', 'approved', 'in_progress', 'completed', 'aborted'
    total_files     INTEGER,
    total_bytes     INTEGER,
    completed_files INTEGER DEFAULT 0,
    completed_bytes INTEGER DEFAULT 0
);

CREATE TABLE migration_steps (
    id              INTEGER PRIMARY KEY,
    plan_id         INTEGER REFERENCES migration_plans(id),
    file_id         INTEGER REFERENCES files(id),
    action          TEXT NOT NULL,
    -- 'move', 'copy', 'delete', 'hardlink', 'symlink'
    source_path     TEXT NOT NULL,
    source_drive_id INTEGER REFERENCES drives(id),
    dest_path       TEXT,               -- NULL for 'delete'
    dest_drive_id   INTEGER REFERENCES drives(id),
    status          TEXT DEFAULT 'pending',
    -- 'pending', 'in_progress', 'completed', 'failed', 'rolled_back'
    pre_hash        TEXT,               -- hash before action
    post_hash       TEXT,               -- hash after action (at dest)
    executed_at     TEXT,
    error           TEXT,
    step_order      INTEGER             -- execution order within plan
);

CREATE TABLE audit_log (
    id              INTEGER PRIMARY KEY,
    timestamp       TEXT DEFAULT (datetime('now')),
    action          TEXT NOT NULL,
    file_id         INTEGER,
    plan_id         INTEGER,
    drive_id        INTEGER,
    details         TEXT,               -- JSON blob with context
    agent_mode      TEXT                -- 'auto' or 'manual'
);

-- Useful indexes
CREATE INDEX idx_files_hash ON files(md5_hash);
CREATE INDEX idx_files_status ON files(status);
CREATE INDEX idx_files_category ON files(category);
CREATE INDEX idx_files_duplicate_group ON files(duplicate_group);
CREATE INDEX idx_files_extension ON files(extension);
CREATE INDEX idx_files_size ON files(size_bytes);
CREATE INDEX idx_files_drive ON files(drive_id);
CREATE INDEX idx_migration_steps_plan ON migration_steps(plan_id, step_order);
```

### Multi-Drive Semantics

The `drives` table enables several important workflows:

**Cross-drive duplicate awareness:**
Files duplicated *across* drives are intentional backups; files duplicated *within* a drive are waste. The `duplicate_groups.cross_drive` flag distinguishes these:

```sql
-- Waste: duplicates on the same drive
SELECT dg.*, COUNT(*) as copies
FROM duplicate_groups dg
JOIN files f ON f.duplicate_group = dg.group_id
WHERE dg.cross_drive = 0
GROUP BY dg.group_id;

-- Expected: files that exist on both NAS and backup
SELECT f1.path as nas_path, f2.path as backup_path
FROM files f1
JOIN files f2 ON f1.md5_hash = f2.md5_hash
JOIN drives d1 ON f1.drive_id = d1.id
JOIN drives d2 ON f2.drive_id = d2.id
WHERE d1.role = 'source' AND d2.role = 'backup';

-- Missing from backup: files on NAS with no copy on any backup drive
SELECT f.path, f.size_bytes
FROM files f
JOIN drives d ON f.drive_id = d.id
WHERE d.role = 'source'
  AND f.md5_hash NOT IN (
    SELECT f2.md5_hash FROM files f2
    JOIN drives d2 ON f2.drive_id = d2.id
    WHERE d2.role = 'backup'
  );
```

**Offline drives:** When a backup drive is disconnected, set `is_online = 0`. ordne won't try to access its files but retains the index for cross-referencing. When reconnected, a quick re-scan checks for changes.

**Drive registration:**

```bash
# Register the NAS main drive
ordne drive add nas_main /mnt/nas --role source

# Register backup drive (read-only)
ordne drive add backup_wd_2tb /mnt/backup --role backup --readonly

# Register the ZFS target (once set up)
ordne drive add zfs_mirror /zfs-pool --role target

# Register cloud backends via rclone (configure remotes first with `rclone config`)
ordne drive add s3_archive --rclone b2:my-archive-bucket --role offload
ordne drive add gdrive_photos --rclone gdrive:Photos --role offload

# List registered drives with space info
ordne drive list

# Mark drive offline when disconnected
ordne drive offline backup_wd_2tb

# Device info is captured automatically for local drives via:
#   - /dev/disk/by-id/*     (stable device identifier)
#   - blkid                  (filesystem UUID)
#   - findmnt / mount        (mount point, fs type)
#   - lsblk                  (size, model, serial)
# For rclone drives, metadata comes from rclone about <remote>:
```

### 1.3 Scan Procedure

```bash
# Step 1: Run rmlint with JSON + checksum cache
rmlint \
  --types="duplicates,emptydirs,emptyfiles,badlinks" \
  --algorithm=md5 \
  --output=json:rmlint_results.json \
  --output=sh:rmlint_cleanup.sh \
  --rank-by="pOma" \
  --progress \
  --xattr-write \
  /path/to/nas/data

# Step 2: Ingest into SQLite
ordne ingest rmlint_results.json --db index.db

# Step 3 (optional): Scan backup drive for cross-reference
rmlint \
  --types="duplicates" \
  --algorithm=md5 \
  --output=json:backup_cross.json \
  --rank-by="pOma" \
  //path/to/nas/data \    # tagged original (preferred)
  /mnt/backup_drive/      # secondary
  
ordne ingest-backup backup_cross.json --db index.db --drive-label "old_wd_2tb"

# Step 4 (optional): Scan rclone remote
# Uses rclone lsjson which returns MD5 checksums from the remote
ordne scan my_s3_bucket    # drive registered as rclone backend
```

### 1.4 Hashing Strategy

**Default: MD5.** Chosen for universality — rmlint, rclone, S3 ETags, and Google Drive all support MD5 natively. No hash translation layer needed across local and remote backends. For natural file deduplication, MD5 collision risk is negligible (especially combined with file size matching).

**Optional: blake3.** Available via `--algorithm blake3` for local-only workflows. blake3 is ~3-5x faster than MD5 on modern CPUs (SIMD-optimized), which matters for multi-TB datasets on fast storage. Cannot be used for rclone-backed drives (remote APIs don't support blake3).

Given ~2.5TB, full MD5 hashing is feasible (MD5 runs at ~500MB/s+ on modern hardware; disk I/O is the bottleneck on spinning drives, not hashing).

However, rmlint already optimizes this:
1. Group by file size (eliminates most files immediately)
2. Hash first few KB of remaining candidates
3. Full hash only for files that still match

This means only the ~100GB+ of actual duplicates need full hashing, which should take under an hour on a spinning drive.

---

## Phase 2: Classification

### 2.1 Automatic Rules

The first pass applies deterministic rules. These are configurable in a TOML file:

```toml
# ordne.toml — classification rules

[[rules]]
match = "*/node_modules/*"
category = "trash"
priority = "low"
reason = "regenerable dependency"

[[rules]]
match = "*/cache/*"
category = "trash"
reason = "cache directory"

[[rules]]
match = "*/.git/objects/*"
category = "trash"
priority = "low"
reason = "git objects (restorable via git fetch from remote)"

[[rules]]
match = "*/.git/pack/*"
category = "trash"
priority = "low"
reason = "git pack data (restorable via git fetch from remote)"

[[rules]]
match = "*/Downloads/*"
category = "staging"
reason = "downloads folder"

[[rules]]
match = "*/Photos/*"
category = "archive"
subcategory = "photos"
priority = "critical"

[[rules]]
match = "*/Documents/*"
category = "archive"
subcategory = "documents"
priority = "critical"

# By extension
[[rules]]
extensions = [".jpg", ".jpeg", ".png", ".heic", ".raw", ".cr2"]
category = "archive"
subcategory = "photos"

[[rules]]
extensions = [".mp4", ".mkv", ".avi", ".mov"]
category = "archive"
subcategory = "media"

[[rules]]
extensions = [".tmp", ".log", ".swp", ".DS_Store", "Thumbs.db"]
category = "trash"

[[rules]]
extensions = [".iso", ".img"]
category = "backup"
subcategory = "disk_images"

# By age
[[rules]]
older_than = "5y"
not_accessed_since = "2y"
suggest_category = "archive"
reason = "old and unused"

# By size
[[rules]]
larger_than = "1GB"
flag = "review"
reason = "large file, worth checking"

# Duplicate handling
[[rules]]
duplicate = true
is_original = false
category = "trash"
reason = "duplicate (non-original copy)"
```

### 2.2 Agent-Assisted Classification

For files that don't match any rule (status = 'unknown'), the agent presents batches for review:

```
=== Classification Review (batch 1/47) ===

Group: /nas/old_stuff/projects/ (23 files, 450MB)
  Mostly: .py, .js, .json files
  Last modified: 2021-03-15
  Contains: README.md, package.json, src/

  Suggested: archive/projects    [A]ccept  [M]odify  [S]kip  [T]rash

Group: /nas/misc/random_backup_2020/ (156 files, 2.1GB)
  Mostly: mixed (.doc, .pdf, .jpg)
  Overlaps 78% with /nas/Documents/ (by hash)
  Last modified: 2020-06-01

  Suggested: trash (duplicates)   [A]ccept  [M]odify  [S]kip  [V]iew details
```

### 2.3 Target Hierarchy

```
/zfs-pool/
├── archive/               # Long-term storage, rarely modified
│   ├── photos/            # Organized by year/month
│   ├── documents/         # Personal docs, tax records, etc.
│   ├── projects/          # Completed/archived code projects
│   ├── media/             # Movies, music, etc.
│   └── disk-images/       # ISOs, VM images
├── active/                # Current working files
│   ├── projects/          # Active code/work projects
│   └── documents/         # Active documents
├── backup/                # Rolling backups (with retention)
│   └── snapshots/         # ZFS snapshots handle this natively
└── incoming/              # Staging area for new/unsorted files
```

---

## Phase 3: Migration

### 3.1 Pre-Migration Steps (before ZFS is ready)

Since ZFS isn't set up yet, the initial work is:

1. **Delete trash** — Remove confirmed junk (caches, node_modules, .tmp files, confirmed duplicate non-originals)
2. **Offload to cloud** — Move cloud-eligible data (photos for Google Photos/Backblaze, documents for cloud backup)
3. **Offload to spare local drives** — Move data temporarily to free up NAS drive space
4. **Set up ZFS mirror** — With the 2× 4TB drives
5. **Migrate cleaned data to ZFS** — Final structured move

### 3.2 Migration Engine

```rust
// Pseudocode for the core migration loop

fn execute_plan(db: &Database, plan_id: i64, mode: Mode) -> Result<()> {
    let plan = db.get_plan(plan_id)?;
    let steps = db.get_steps(plan_id, Status::Pending)?;

    for batch in steps.chunks(BATCH_SIZE) {
        // Pre-flight checks
        let free_space = get_free_space(&plan.dest_device)?;
        let batch_size: u64 = batch.iter().map(|s| s.file_size).sum();

        if free_space < batch_size * 3 / 2 {  // 50% headroom
            warn!("Insufficient space ({} free, {} needed), pausing", free_space, batch_size);
            break;
        }

        if mode == Mode::Propose {
            print_batch_summary(batch);
            if !confirm("Execute this batch?")? {
                continue;
            }
        }

        for step in batch {
            match execute_step(db, step) {
                Ok(()) => {},
                Err(e) => {
                    log_error(db, step, &e)?;
                    if mode != Mode::Auto {
                        match prompt_action("Step failed. [R]etry [S]kip [A]bort?")? {
                            Action::Retry => { execute_step(db, step)?; },
                            Action::Skip  => continue,
                            Action::Abort => return Ok(()),
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn execute_step(db: &Database, step: &MigrationStep) -> Result<()> {
    // 1. Pre-check: source still exists and matches index
    if !Path::new(&step.source_path).exists() {
        return Err(anyhow!("Source file missing: {}", step.source_path));
    }

    let current_hash = hash_file(&step.source_path)?;
    if current_hash != step.pre_hash {
        return Err(anyhow!("Source file changed since indexing"));
    }

    // 2. Ensure destination directory exists
    if let Some(parent) = Path::new(&step.dest_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // 3. Copy with rsync (local) or rclone (remote)
    let dest_drive = db.get_drive(step.dest_drive_id)?;
    match dest_drive.backend.as_str() {
        "local" => rsync(&step.source_path, &step.dest_path, &RsyncOpts {
            checksum: true,
            partial: true,      // resumable
            sparse: true,        // handle sparse files
            preserve_all: true,  // permissions, times, owner, group, xattrs
        })?,
        "rclone" => rclone_copy(&step.source_path, &step.dest_path, &dest_drive, &RcloneOpts {
            checksum: true,
            progress: true,
        })?,
        _ => return Err(anyhow!("Unknown backend: {}", dest_drive.backend)),
    };

    // 4. Verify destination
    let dest_hash = hash_file(&step.dest_path)?;
    if dest_hash != step.pre_hash {
        fs::remove_file(&step.dest_path)?;
        return Err(anyhow!("Destination hash mismatch after copy"));
    }

    // 5. Record success
    db.update_step(step.id, Status::Completed, &dest_hash)?;
    db.log_audit(AuditAction::Migrated, step.file_id, &step)?;

    // 6. Remove source (only for 'move' actions)
    if step.action == Action::Move {
        fs::remove_file(&step.source_path)?;
        db.update_file(step.file_id, FileStatus::SourceRemoved)?;
    }

    Ok(())
}
```

### 3.3 Safety Invariants

These must ALWAYS hold:

1. **No file is deleted unless its hash-verified copy exists elsewhere** (on another drive, in cloud, or at its new location)
2. **No batch exceeds 50% of remaining free space** on the destination
3. **Every action is logged** in `audit_log` with enough detail to reverse it
4. **Source file hash is re-verified** immediately before any move/delete (guards against files changed between indexing and migration)
5. **Destination hash is verified** after every copy, before any source deletion
6. **Dry-run is the default mode** — must explicitly opt into destructive operations
7. **Duplicate deletion only happens for files where `is_original` is set on another copy in the group** (i.e., we never delete the last copy)

### 3.4 Space Management

The critical constraint is that you're working on a nearly-full drive:

```
Available space budget:
  Start: ~2.5TB used on NAS drive (assume ~3TB capacity = ~500GB free?)
  
  Phase A: Delete trash → frees maybe 50-200GB
  Phase B: Delete confirmed duplicates → frees ~100GB+
  Phase C: Offload to cloud/spare drives → frees variable
  Phase D: Now have enough room to work with
  Phase E: Set up ZFS, begin structured migration
```

The agent should track space continuously:

```rust
fn space_report(db: &Database, config: &Config) -> Result<SpaceReport> {
    Ok(SpaceReport {
        nas_free: get_free_space(&config.nas_path)?,
        nas_used: get_used_space(&config.nas_path)?,
        zfs_free: config.zfs_path.as_ref().map(|p| get_free_space(p)).transpose()?,
        spare_free: config.spare_drives.iter()
            .map(|d| Ok((d.label.clone(), get_free_space(&d.path)?)))
            .collect::<Result<_>>()?,
        pending_deletes_bytes: db.query_sum(
            "SELECT SUM(size_bytes) FROM files WHERE category='trash' AND status='classified'"
        )?,
        pending_moves_bytes: db.query_sum(
            "SELECT SUM(size_bytes) FROM migration_steps WHERE status='pending'"
        )?,
    })
}
```

---

## Phase 4: Backup Drive Cross-Reference

Since backup drives are now first-class in the `drives` table, cross-referencing is a natural query rather than a separate workflow:

### Process

1. Mount old backup drive read-only: `mount -o ro /dev/sdX /mnt/backup`
2. Register it: `ordne drive add old_wd_2tb /mnt/backup --role backup --readonly`
3. Scan it: `ordne scan old_wd_2tb`
4. rmlint runs across both drives, dedup groups automatically span drives
5. Query for files unique to backup (not on NAS)
6. Agent presents unique files for review — recover or ignore

### Queries

```sql
-- Files on backup that aren't on NAS at all
SELECT f.abs_path, f.size_bytes, f.modified_at
FROM files f
JOIN drives d ON f.drive_id = d.id
WHERE d.role = 'backup'
  AND f.md5_hash NOT IN (
    SELECT f2.md5_hash FROM files f2
    JOIN drives d2 ON f2.drive_id = d2.id
    WHERE d2.role = 'source'
  )
ORDER BY f.size_bytes DESC;

-- Files on backup that match NAS files we marked as trash
-- (might want to reconsider the trash classification)
SELECT f_backup.abs_path as backup_path,
       f_nas.abs_path as nas_path,
       f_nas.category
FROM files f_backup
JOIN drives d_backup ON f_backup.drive_id = d_backup.id
JOIN files f_nas ON f_nas.md5_hash = f_backup.md5_hash
JOIN drives d_nas ON f_nas.drive_id = d_nas.id
WHERE d_backup.role = 'backup'
  AND d_nas.role = 'source'
  AND f_nas.category = 'trash';

-- Integrity check: files that should match but don't (bit rot detection)
SELECT f1.abs_path, f2.abs_path,
       f1.md5_hash as nas_hash, f2.md5_hash as backup_hash
FROM files f1
JOIN files f2 ON f1.path = f2.path  -- same relative path
JOIN drives d1 ON f1.drive_id = d1.id
JOIN drives d2 ON f2.drive_id = d2.id
WHERE d1.role = 'source' AND d2.role = 'backup'
  AND f1.size_bytes = f2.size_bytes   -- same size
  AND f1.md5_hash != f2.md5_hash; -- but different hash = bit rot
```

---

## CLI Design (`ordne`)

```
ordne scan [paths...]          Scan paths with rmlint, ingest into DB
ordne drive add <label> <mount_path>   Register a new drive
    --role <source|target|backup|offload>
    --readonly                          Mark as read-only
    --rclone <remote:path>              Use rclone backend (e.g. b2:bucket, gdrive:folder)
ordne drive list                        Show all drives with space info
ordne drive remove <label>              Unregister (does not touch files)
ordne drive online <label>              Mark drive as connected
ordne drive offline <label>             Mark drive as disconnected
ordne drive info <label>                Detailed info (device-by-id, UUID, fs, etc.)

ordne scan <drive_label>                Scan a drive with rmlint, ingest into DB
    --rescan                            Force full rescan (ignore cache)
ordne scan --all                        Scan all online drives

ordne status                            Show overall progress dashboard
ordne status --space                    Show space usage across all drives

ordne query duplicates                  List duplicate groups
    --min-size <bytes>                  Filter by minimum size
    --sort-by waste|count|size          Sort order
    --same-drive                        Only within-drive duplicates (waste)
    --cross-drive                       Only cross-drive duplicates (backups)
    --drive <label>                     Filter to specific drive
ordne query unclassified                List files needing classification
    --drive <label>
    --limit <n>                         Batch size
ordne query category <n>             List files in a category
ordne query large-files                 Files over threshold
    --min-size <bytes>
ordne query backup-unique <drive_label> Files only on backup, not on source

ordne classify                          Interactive classification session
    --auto                              Apply rules only, no prompts
    --rules <file.toml>                 Custom rules file
    --drive <label>                     Classify files on specific drive
ordne classify <file_id> <category> [subcategory]
                                        Classify a single file

ordne plan create                       Generate migration plan from classifications
    --phase <delete_trash|dedup|migrate|offload>
    --source <drive_label>
    --target <drive_label>
    --batch-size <n>                    Files per batch
    --max-bytes <bytes>                 Max data per plan
ordne plan show <plan_id>               Display plan details
ordne plan approve <plan_id>            Mark plan as approved

ordne migrate <plan_id>                 Execute an approved plan
    --dry-run                           Show what would happen (default)
    --execute                           Actually do it
    --auto                              Don't prompt per batch
    --io-limit <MB/s>                   Throttle I/O
ordne migrate --rollback <plan_id> [step_id]
                                        Undo a migration step

ordne verify <plan_id>                  Re-verify all completed steps
ordne verify --full                     Re-hash everything in the DB

ordne export                            Export index/plans as JSON for inspection
ordne report                            Generate summary report (space saved, files moved, etc.)

---

## MCP Server (`ordne-mcp`)

The MCP server is a separate binary in the same workspace that exposes ordne's functionality to AI agents. It links the ordne library directly (no CLI subprocess shelling) for type-safe access to the DB and operations.

### Architecture

```
┌──────────────────────┐     stdio/SSE      ┌─────────────────────┐
│  Claude Code /       │◄──────────────────►│   ordne-mcp          │
│  Claude Desktop /    │    JSON-RPC         │   (MCP server bin)   │
│  Any MCP client      │                    │                      │
└──────────────────────┘                    │   ┌──────────────┐   │
                                            │   │ ordne (lib)  │   │
                                            │   │  - db/       │   │
                                            │   │  - index/    │   │
                                            │   │  - classify/ │   │
                                            │   │  - migrate/  │   │
                                            │   └──────┬───────┘   │
                                            │          │           │
                                            │   ┌──────▼───────┐   │
                                            │   │  SQLite DB    │   │
                                            │   │  (ordne.db)   │   │
                                            │   └──────────────┘   │
                                            └─────────────────────┘
```

### Workspace Layout

```toml
# Cargo.toml (workspace root)
[workspace]
members = ["crates/ordne", "crates/ordne-mcp"]

[workspace.dependencies]
rusqlite = { version = "0.31", features = ["bundled"] }
blake3 = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
tokio = { version = "1", features = ["full"] }
```

```toml
# crates/ordne/Cargo.toml — CLI + library
[lib]
name = "ordne_lib"

[[bin]]
name = "ordne"

[dependencies]
# ... (as previously specified)
```

```toml
# crates/ordne-mcp/Cargo.toml — MCP server
[[bin]]
name = "ordne-mcp"

[dependencies]
ordne_lib = { path = "../ordne" }
rmcp = { version = "0.12", features = ["server"] }
rmcp-macros = "0.12"
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
schemars = "0.8"
```

### MCP Tools Exposed

```rust
use rmcp::{tool, tool_router, ServerHandler, model::*};

#[derive(Clone)]
pub struct OrdneServer {
    db: Arc<Database>,
    config: Arc<Config>,
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl OrdneServer {
    // === Status & Discovery ===

    #[tool(description = "Get overall status: file counts, space usage, progress per drive")]
    async fn status(&self) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "List registered drives with device info, space, and online status")]
    async fn drive_list(&self) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Get free/used space on a specific drive or all drives")]
    async fn space_check(
        &self,
        #[tool(param)] #[schemars(description = "Drive label, or 'all'")] 
        drive: String,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Indexing ===

    #[tool(description = "Register a new drive and scan it. Returns scan progress.")]
    async fn drive_add(
        &self,
        #[tool(param)] label: String,
        #[tool(param)] mount_path: String,
        #[tool(param)] #[schemars(description = "source | target | backup | offload")]
        role: String,
        #[tool(param)] #[schemars(description = "Mount read-only (default true for backup)")]
        readonly: Option<bool>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Trigger a scan/rescan of a registered drive")]
    async fn scan(
        &self,
        #[tool(param)] drive_label: String,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Querying ===

    #[tool(description = "Query duplicate file groups. Returns groups sorted by wasted space.")]
    async fn query_duplicates(
        &self,
        #[tool(param)] #[schemars(description = "Minimum file size in bytes")] 
        min_size: Option<u64>,
        #[tool(param)] #[schemars(description = "Filter to specific drive")] 
        drive: Option<String>,
        #[tool(param)] #[schemars(description = "Only within-drive dupes (true) or cross-drive too")]
        same_drive_only: Option<bool>,
        #[tool(param)] limit: Option<u32>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Query files needing classification")]
    async fn query_unclassified(
        &self,
        #[tool(param)] drive: Option<String>,
        #[tool(param)] limit: Option<u32>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Query files by category, extension, size, age, or path pattern")]
    async fn query_files(
        &self,
        #[tool(param)] category: Option<String>,
        #[tool(param)] extension: Option<String>,
        #[tool(param)] min_size: Option<u64>,
        #[tool(param)] path_contains: Option<String>,
        #[tool(param)] drive: Option<String>,
        #[tool(param)] limit: Option<u32>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Find files unique to a backup drive (not present on source drives)")]
    async fn query_backup_unique(
        &self,
        #[tool(param)] backup_drive: String,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Classification ===

    #[tool(description = "Run auto-classification rules on unclassified files")]
    async fn classify_auto(
        &self,
        #[tool(param)] drive: Option<String>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Classify a specific file or set of files")]
    async fn classify(
        &self,
        #[tool(param)] #[schemars(description = "File ID or comma-separated IDs")]
        file_ids: String,
        #[tool(param)] category: String,
        #[tool(param)] subcategory: Option<String>,
        #[tool(param)] priority: Option<String>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Classify all files matching a path glob pattern")]
    async fn classify_pattern(
        &self,
        #[tool(param)] #[schemars(description = "Glob pattern, e.g. '*/node_modules/*'")] 
        pattern: String,
        #[tool(param)] category: String,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Migration Planning & Execution ===

    #[tool(description = "Create a migration plan. Returns plan summary for review. Does NOT execute.")]
    async fn plan_create(
        &self,
        #[tool(param)] #[schemars(description = "What to plan: 'delete_trash', 'dedup', 'migrate_to_target', 'offload'")]
        phase: String,
        #[tool(param)] source_drive: Option<String>,
        #[tool(param)] target_drive: Option<String>,
        #[tool(param)] #[schemars(description = "Max files per plan")]
        batch_size: Option<u32>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Show details of a migration plan")]
    async fn plan_show(
        &self,
        #[tool(param)] plan_id: i64,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Approve a plan for execution")]
    async fn plan_approve(
        &self,
        #[tool(param)] plan_id: i64,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Execute an approved plan. Performs verified moves/deletes.")]
    async fn migrate_execute(
        &self,
        #[tool(param)] plan_id: i64,
        #[tool(param)] #[schemars(description = "Actually perform changes (false = dry-run)")]
        execute: bool,
        #[tool(param)] #[schemars(description = "IO throttle in MB/s (0 = unlimited)")]
        io_limit_mbps: Option<u32>,
    ) -> Result<CallToolResult, McpError> { ... }

    #[tool(description = "Rollback a migration step or entire plan")]
    async fn rollback(
        &self,
        #[tool(param)] plan_id: i64,
        #[tool(param)] step_id: Option<i64>,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Verification ===

    #[tool(description = "Re-verify hashes of migrated files")]
    async fn verify(
        &self,
        #[tool(param)] plan_id: Option<i64>,
        #[tool(param)] #[schemars(description = "Re-hash everything in the DB")]
        full: Option<bool>,
    ) -> Result<CallToolResult, McpError> { ... }

    // === Reporting ===

    #[tool(description = "Generate a summary report: space saved, files moved, errors, etc.")]
    async fn report(&self) -> Result<CallToolResult, McpError> { ... }
}
```

### MCP Server Configuration

```json
// Claude Code MCP config (~/.config/claude-code/mcp.json or project .mcp.json)
{
  "mcpServers": {
    "ordne": {
      "command": "ordne-mcp",
      "args": ["--db", "/home/user/.local/share/ordne/ordne.db"],
      "env": {
        "RUST_LOG": "info"
      }
    }
  }
}
```

```nix
# NixOS: declare the MCP server config alongside the package
{ pkgs, inputs, ... }: {
  home.packages = [
    inputs.ordne.packages.${pkgs.system}.default  # provides both ordne and ordne-mcp
  ];

  # Claude Code MCP config
  xdg.configFile."claude-code/mcp.json".text = builtins.toJSON {
    mcpServers.ordne = {
      command = "${inputs.ordne.packages.${pkgs.system}.default}/bin/ordne-mcp";
      args = ["--db" "/home/user/.local/share/ordne/ordne.db"];
    };
  };
}
```

### Why Library + CLI + MCP (not just MCP)

The three-layer approach matters:

1. **`ordne_lib`** (library) — All logic. Testable, no I/O assumptions.
2. **`ordne`** (CLI binary) — For direct human use, scripting, cron jobs. Works without any AI agent.
3. **`ordne-mcp`** (MCP binary) — Thin adapter exposing the library as MCP tools. The agent gets structured JSON responses, not CLI text parsing.

This means `ordne` is useful standalone for anyone who doesn't want AI-assisted workflows, and the MCP server is a bonus for those who do. It also means the CLI and MCP server always agree because they share the same library code and DB.

---

## Claude Code Integration

### Agent Workflow

When invoked via Claude Code, the agent should:

1. **Start by reading the DB status** — `ordne status`
2. **Identify the current phase** and what's actionable
3. **Propose next steps** as concrete commands
4. **Wait for approval** (unless `--auto` flag)
5. **Execute and report** results
6. **Handle errors** gracefully — never leave the system in an inconsistent state

### Example Session

```
> What's the current state of the migration?

Agent: Let me check.
$ ordne status

📊 Migration Status:
  Files indexed:    847,293
  Total size:       2.38 TB
  Duplicates:       12,847 files in 4,231 groups (142 GB waste)
  Classified:       621,000 / 847,293 (73%)
  Unclassified:     226,293
  Migrated:         0

  Space:
    NAS free:       487 GB
    ZFS:            not set up

  Suggested next action: Classify remaining files, then delete trash (est. 89 GB recoverable)

> Let's classify and then clean up trash

Agent: I'll run auto-classification first with the rules, then show you what's left.
$ ordne classify --auto
  → Classified 198,421 files by rules
  → 27,872 files still unclassified

$ ordne classify --limit 20
  [interactive session: presents groups for review]

Agent: Auto-classification done. 89.2 GB of trash identified. Here's the breakdown:
  - node_modules:     34.1 GB (1,203 directories)
  - cache files:      22.7 GB
  - .tmp/.log:        8.4 GB
  - duplicate copies: 24.0 GB (non-original duplicates that have verified originals)

Shall I create a deletion plan? This will NOT delete anything yet.
$ ordne plan create --phase A
$ ordne plan show 1

> Looks good, go ahead

$ ordne migrate 1 --execute
  [batch 1/34: 2.6 GB] ✓
  [batch 2/34: 2.8 GB] ✓
  ...
```

---

## Implementation Stack

| Component            | Technology                                                                 | Why                                                                 |
|-----------           |-----------                                                                 |-----                                                                |
| CLI & orchestration  | Rust (`clap` for CLI, `rusqlite` for DB)                                   | Single binary, no runtime deps, type-safe error handling            |
| MCP server           | Rust (`rmcp` — official MCP SDK)                                           | Same workspace, links library directly, stdio transport             |
| Index DB             | SQLite (via `rusqlite`)                                                    | No dependencies, portable, queryable                                |
| Dedup scanning       | `rmlint` (external, invoked via CLI)                                       | Best-in-class, handles edge cases, JSON output                      |
| Hashing              | MD5 default (via `md5` crate); blake3 opt-in (via `blake3` crate)          | MD5 universal across local/S3/GDrive; blake3 faster for local-only  |
| File operations      | `rsync` (local), `rclone` (remote) for copies; `std::fs` for local deletes | rsync for local, rclone for cloud — both support verified transfers |
| Cloud backends       | `rclone` (external)                                                        | 70+ backends: S3, Google Drive, Dropbox, etc.                       |
| EXIF metadata        | `kamadak-exif` crate                                                       | Photo reorganization by date                                        |
| Classification rules | TOML config (via `serde` + `toml` crate)                                   | Rust-native config format, human-editable                           |
| Agent interface      | CLI (Claude Code invokes `ordne` commands)                                 | Simple, debuggable, no extra server needed                          |
| Progress/reporting   | `indicatif` for progress bars, `comfy-table` for tables                    | Standard Rust terminal UI crates                                    |
| JSON parsing         | `serde_json`                                                               | Parse rmlint output, export reports                                 |
| Path matching        | `globset` or `glob` crate                                                  | For classification rule patterns                                    |

### Rust Crate Dependencies

```toml
[dependencies]
clap = { version = "4", features = ["derive"] }
rusqlite = { version = "0.31", features = ["bundled"] }  # bundles SQLite, no system dep
md-5 = "0.10"               # MD5 hashing (default)
blake3 = "1"                 # optional fast hashing for local-only
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
indicatif = "0.17"
comfy-table = "7"
globset = "0.4"
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"                # ergonomic error handling
thiserror = "1"              # typed errors for library code
walkdir = "2"                # recursive directory traversal
dialoguer = "0.11"           # interactive prompts
console = "0.15"             # terminal styling
kamadak-exif = "0.5"         # EXIF metadata for photo reorganization
```

### External Dependencies

```
# Required
rmlint        # apt install rmlint / brew install rmlint
rsync         # apt install rsync (usually pre-installed)

# Required for cloud backends
rclone        # apt install rclone / brew install rclone
              # configured via `rclone config` for S3, Google Drive, etc.

# Optional
b3sum         # cargo install b3sum (only needed if using --algorithm blake3)
```

### Build & Distribution

```bash
# Build
cargo build --release

# Install from crates.io (once published)
cargo install ordne

# Or via pre-built binaries (GitHub Releases, CI builds for linux/mac/windows)
# Single static binary, no runtime dependencies
```

---

## Project Structure

```
ordne/
├── Cargo.toml                      # Workspace root
├── LICENSE                         # MIT
├── README.md
├── flake.nix                       # Nix flake (build, dev shell, NixOS module)
├── flake.lock
├── ordne.toml.example              # Default classification rules
├── crates/
│   ├── ordne/                      # CLI + library crate
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs             # CLI entry point (clap)
│   │   │   ├── lib.rs              # Library root (pub API for ordne-mcp)
│   │   │   ├── cli/
│   │   │   │   ├── mod.rs          # Subcommand dispatch
│   │   │   │   ├── drive.rs        # drive add/list/remove/online/offline
│   │   │   │   ├── scan.rs         # scan <drive>
│   │   │   │   ├── query.rs        # query duplicates/unclassified/category
│   │   │   │   ├── classify.rs     # classify (auto + interactive)
│   │   │   │   ├── plan.rs         # plan create/show/approve
│   │   │   │   ├── migrate.rs      # migrate, rollback
│   │   │   │   ├── verify.rs       # verify plans/full
│   │   │   │   └── status.rs       # status dashboard
│   │   │   ├── db/
│   │   │   │   ├── mod.rs          # Database connection, migrations
│   │   │   │   ├── schema.rs       # Table creation, schema versioning
│   │   │   │   ├── drives.rs       # Drive CRUD, device discovery
│   │   │   │   ├── files.rs        # File CRUD operations
│   │   │   │   ├── duplicates.rs   # Duplicate group queries
│   │   │   │   ├── plans.rs        # Migration plan CRUD
│   │   │   │   └── audit.rs        # Audit log operations
│   │   │   ├── index/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── rmlint.rs       # Parse rmlint JSON output
│   │   │   │   ├── scanner.rs      # Filesystem walk + metadata collection
│   │   │   │   ├── hasher.rs       # MD5/blake3 hashing (for verification)
│   │   │   │   └── device.rs       # /dev/disk/by-id, blkid, lsblk queries
│   │   │   ├── classify/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── rules.rs        # TOML rule parsing + matching engine
│   │   │   │   └── interactive.rs  # TUI for manual classification
│   │   │   ├── migrate/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── engine.rs       # Core migrate loop (batch, verify, delete)
│   │   │   │   ├── rsync.rs        # rsync wrapper
│   │   │   │   ├── space.rs        # Free space checking per drive
│   │   │   │   └── rollback.rs     # Undo operations
│   │   │   └── util/
│   │   │       ├── mod.rs
│   │   │       ├── format.rs       # Human-readable sizes, durations
│   │   │       └── progress.rs     # Progress bar helpers
│   │   └── tests/
│   │       ├── integration/
│   │       │   ├── scan_test.rs
│   │       │   ├── classify_test.rs
│   │       │   └── migrate_test.rs
│   │       └── fixtures/           # Test directory trees
│   │
│   └── ordne-mcp/                  # MCP server crate
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs             # MCP server entry point (stdio transport)
│           ├── tools.rs            # #[tool] implementations wrapping ordne_lib
│           └── resources.rs        # MCP resources (DB status, drive info)
└── docs/
    ├── architecture.md
    └── nixos-setup.md
```

---

## Risk Mitigation

| Risk | Mitigation |
|------|-----------|
| Hash collision (false duplicate) | MD5 collision probability is negligible for natural files (~1 in 2^128); combined with size matching it's effectively zero. For paranoid verification, use `--verify` for byte-for-byte comparison |
| Drive failure during migration | All operations are atomic per-file; audit log allows recovery; ZFS will provide redundancy once set up |
| Running out of space mid-migration | Space check before every batch; conservative 50% headroom; trash deletion happens first |
| Wrong file classified as trash | Dry-run default; interactive review for anything ambiguous; audit log for undo |
| File changed between index and migration | Re-hash verification immediately before any move/delete |
| Interrupted migration | Resumable: steps have status tracking; rsync `--partial` handles interrupted copies |
| Old backup drive has bit rot | Compare hashes of files that should match; flag discrepancies for review |

---

## Resolved Design Decisions

1. **Cloud backends → rclone.** ordne uses rclone as its universal cloud abstraction. Users configure remotes via `rclone config` (S3-compatible, Google Drive, Dropbox, etc.), then register them as drives in ordne (`ordne drive add gcloud rclone://gdrive:backup --role offload`). This gives 70+ backends for free without ordne implementing any cloud API directly.

2. **Photo reorganization → EXIF-aware rules.** The rules engine supports reorganization patterns:
   ```toml
   [[rules]]
   extensions = [".jpg", ".jpeg", ".png", ".heic", ".raw", ".cr2"]
   category = "archive"
   subcategory = "photos"
   reorganize.pattern = "{exif_year}/{exif_month}/{filename}"
   reorganize.fallback = "unsorted/{filename}"
   ```
   Uses the `kamadak-exif` crate for metadata extraction. Same pattern can apply to videos, documents, etc. using file creation date as fallback when format-specific metadata isn't available.

3. **Git repos → strip objects, keep config.** By default, `.git/objects/` and `.git/pack/` are removed (the heavy parts). The remaining `.git/` (config, HEAD, refs) is tiny and allows in-place restore via `git fetch && git checkout .` — no re-clone needed. Remote URLs are also captured in the ordne DB. Specific repos can override this to preserve full history for cases where the remote may no longer exist:
   ```toml
   [[rules]]
   match = "*/.git/objects/*"
   action = "strip_git"
   category = "trash"
   reason = "git objects (restorable via git fetch)"

   # Override: keep full history for irreplaceable repos
   [[rules]]
   match = "*/my-irreplaceable-project/.git/*"
   action = "keep"
   priority = "critical"
   ```

4. **Media transcoding** — Out of scope for initial release. Future feature.

5. **Hashing → MD5 default.** MD5 is universally supported (rmlint, rclone, S3 ETags, Google Drive). For natural file dedup, collision risk is negligible. blake3 available as opt-in (`--algorithm blake3`) for local-only workflows. Single hash type per DB to keep dedup comparison simple.

6. **ZFS integration** — Out of initial scope, but the architecture is designed so it slots in cleanly later. See "Future: ZFS Integration" below.

7. **Scheduled re-scans** — Future feature. Trivial to add as a systemd timer once the core tool works. Use case: periodic `ordne scan --all && ordne classify --auto && ordne report` to catch organizational drift over time.

---

## Future: ZFS Integration

> Not in scope for v1, but the architecture should not make this hard to add.

Once data is migrated to a ZFS mirror, ZFS native snapshotting replaces manual backup copies entirely:

- **sanoid** / **znapzend** for automated snapshot scheduling (hourly, daily, weekly)
- Retention policies ordne old snapshots (e.g. keep 24 hourly, 30 daily, 12 monthly)
- **syncoid** replicates snapshots to another machine/drive for offsite backup

This means ordne's `backup` drive role becomes less relevant post-ZFS — point-in-time recovery is handled by snapshots, not file copies.

**What ordne needs to accommodate:**
- The `drives` table already supports a `target` role for the ZFS pool
- Migration plans already track source → target with verification
- Adding a `zfs_dataset` field to `drives` and a `post_migrate_snapshot` option to plans is a small schema addition later
- The report/status commands could surface ZFS pool health and snapshot status alongside ordne's own data

**Design principle:** ordne owns the *organizational* layer (what files go where, dedup, classification). ZFS owns the *durability* layer (redundancy, snapshots, scrubbing). They don't overlap, so integration is additive not refactoring.
