# Migration Module (Stream C)

The Migration Module is the most critical component of Ordne, responsible for safely executing file operations with comprehensive safety guarantees. This module prioritizes data safety above all else.

## Architecture

### Modules

- **planner.rs**: Creates migration plans from classified files
- **engine.rs**: Executes migration plans with safety checks
- **rollback.rs**: Reverses completed migration steps
- **space.rs**: Manages disk space and enforces 50% headroom rule
- **hash.rs**: Computes and verifies file hashes (Blake3/MD5)
- **rsync.rs**: Wrapper for rsync with safety flags
- **rclone.rs**: Wrapper for rclone for cloud storage

### Database Support

- **db/plans.rs**: Plan and step CRUD operations
- **db/audit.rs**: Complete audit log for all operations

## Safety Invariants (CRITICAL)

The migration module enforces these invariants at all times:

### 1. No File Deleted Without Verified Copy

Before any file is deleted:
1. Source file is hashed
2. File is copied to destination
3. Destination file is hashed
4. Only if hashes match is source deleted

**Implementation**: `engine::execute_move()` and `engine::execute_delete()`

### 2. 50% Free Space Rule

No migration batch can exceed 50% of available free space on the destination drive.

**Implementation**: `space::SpaceInfo::max_safe_write_bytes()`

### 3. Every Action Logged

Every file operation creates an audit log entry with:
- Timestamp
- Action type
- File ID
- Plan ID
- Drive ID
- Details
- Agent mode

**Implementation**: All operations in `engine.rs` call `db.log_audit()`

### 4. Source Hash Re-verification

Before any destructive operation (move/delete), the source file is re-hashed to ensure it hasn't changed during migration.

**Implementation**: `hash::verify_source_unchanged()`

### 5. Destination Hash Verification

After copying, the destination file is hashed and compared to the source hash.

**Implementation**: `hash::verify_destination()`

### 6. Dry-Run Default Mode

All migrations default to dry-run mode. The `--execute` flag must be explicitly passed to perform actual operations.

**Implementation**: `EngineOptions::default()` sets `dry_run: true`

### 7. Duplicate Deletion Safety

Duplicates are only deleted if the original file exists and its hash matches.

**Implementation**: `planner::create_dedup_plan()` records original file information

## Plan Types

### Delete Trash

Deletes files categorized as trash with Priority::Trash.

```rust
let plan_id = planner.create_delete_trash_plan(trash_files)?;
```

### Deduplication

Removes duplicate files while keeping one original.

```rust
let plan_id = planner.create_dedup_plan(duplicates, &original)?;
```

### Migrate to Target

Moves files to target drive for active use.

```rust
let plan_id = planner.create_migrate_plan(files, target_drive_id, target_mount)?;
```

### Offload

Moves low-priority files to backup/offload storage, then deletes source.

```rust
let plan_id = planner.create_offload_plan(files, offload_drive_id, offload_mount)?;
```

## Usage Example

```rust
use ordne_lib::*;

// Initialize database
let mut db = SqliteDatabase::open("ordne.db")?;
db.initialize()?;

// Create planner
let planner_opts = PlannerOptions {
    max_batch_size_bytes: None,
    enforce_space_limits: true,
    dry_run: false,
};
let mut planner = Planner::new(&mut db, planner_opts);

// Create migration plan
let plan_id = planner.create_migrate_plan(
    files,
    target_drive_id,
    "/mnt/target"
)?;

// Approve plan (required step)
planner.approve_plan(plan_id)?;

// Execute with safety checks
let engine_opts = EngineOptions {
    dry_run: false,
    verify_hashes: true,
    retry_count: 3,
    enforce_safety: true,
};
let mut engine = MigrationEngine::new(&mut db, engine_opts);

// Execute plan
engine.execute_plan(plan_id)?;

// If something went wrong, rollback
if needs_rollback {
    let mut rollback = RollbackEngine::new(&mut db, true);
    rollback.rollback_plan(plan_id)?;
}
```

## Workflow

1. **Planning Phase**
   - Analyze classified files
   - Calculate space requirements
   - Verify space availability (50% rule)
   - Create plan with ordered steps
   - Set plan status to Draft

2. **Approval Phase**
   - Review plan details
   - User explicitly approves plan
   - Status changes to Approved

3. **Execution Phase**
   - Pre-flight checks (space, drives online)
   - For each step in order:
     - Mark step InProgress
     - Compute source hash (if verify_hashes=true)
     - Execute operation (copy, move, delete, etc.)
     - Verify destination hash (if applicable)
     - Mark step Completed
     - Log audit entry
     - Update plan progress
   - Mark plan Completed

4. **Rollback (if needed)**
   - Iterate steps in reverse order
   - Restore each completed step
   - Verify restoration with hashes
   - Mark steps as RolledBack

## Safety Features

### Hash Verification

Blake3 is preferred for speed, MD5 is fallback:

```rust
let hash = hash::compute_blake3_hash(path)?;
hash::verify_destination(dest_path, &hash)?;
```

### Space Management

```rust
let space_info = space::get_free_space("/mnt/drive")?;
let max_safe = space_info.max_safe_write_bytes();

space::verify_sufficient_space("/mnt/drive", required_bytes)?;
```

### Atomic Operations

Each step is atomic - either fully completes or fully fails. There's no partial state.

### Audit Trail

Complete audit trail for forensics:

```rust
let audit_entries = db.get_audit_entries_for_plan(plan_id)?;
for entry in audit_entries {
    println!("{}: {} - {}",
        entry.timestamp,
        entry.action,
        entry.details.unwrap_or_default()
    );
}
```

## Testing

### Unit Tests

Each module has comprehensive unit tests:

```bash
cargo test --lib migrate
```

### Integration Tests

Full end-to-end scenarios:

```bash
cargo test --test integrate_test
```

### Safety Tests

Dedicated tests for safety invariants:

```bash
cargo test --test safety_test
```

### Property Tests

Proptest-based property testing:

```bash
cargo test --features proptest
```

## Error Handling

All operations return `Result<T>` with detailed error types:

- `OrdneError::HashMismatch`: Hash verification failed
- `OrdneError::InsufficientSpace`: Not enough space
- `OrdneError::SourceChanged`: Source file modified during migration
- `OrdneError::DestinationVerification`: Destination verification failed
- `OrdneError::DriveOffline`: Required drive not available
- `OrdneError::ExternalTool`: rsync/rclone failure

## Performance Considerations

- **Parallel Hashing**: Future enhancement for multi-threaded hashing
- **Batch Size**: Optimized to balance safety and performance
- **Rsync Flags**: Using `--checksum`, `--partial`, `--sparse` for efficiency
- **Space Checks**: Performed once per batch, not per file

## Limitations

- **Delete Rollback**: Cannot rollback completed delete operations (file is gone)
- **Platform**: Space checking currently Linux-only (uses `statvfs`)
- **Symlinks**: Unix-only support

## Future Enhancements

- [ ] Parallel migration of independent files
- [ ] Resume interrupted migrations
- [ ] Incremental hash verification (don't re-hash unchanged files)
- [ ] Compression during migration
- [ ] Encryption support
- [ ] Progress callbacks for UI integration
- [ ] Bandwidth limiting for network transfers
- [ ] Smart retry with exponential backoff

## Contributing

When modifying the migration module:

1. **Safety First**: Never compromise safety for performance
2. **Test Thoroughly**: Add tests for all new code paths
3. **Document Invariants**: Update docs if changing safety guarantees
4. **Audit Logging**: Log all operations
5. **Error Handling**: Never use `unwrap()` or `expect()`
6. **Review Required**: Migration changes require extra scrutiny

## License

MIT
