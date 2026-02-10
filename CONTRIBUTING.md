# Contributing to Prune

## Quick Start

```bash
# Clone and enter
git clone <repo>
cd prune

# Enter Nix dev shell (has all tools)
nix develop

# Build
cargo build

# Run tests
cargo test

# Run
cargo run -- --help
```

## Architecture Overview

Prune is a Rust workspace with:
- **Library crate** (`crates/prune`) - All logic
- **CLI binary** - User interface
- **Database** - SQLite via rusqlite
- **External tools** - rmlint, rsync, rclone

## Development Workflow

### Building

```bash
cargo build                 # Debug build
cargo build --release       # Release build
cargo check                 # Fast compile check
cargo clippy                # Lints
```

### Testing

```bash
cargo test                  # Run all tests
cargo test db::             # Test specific module
cargo test -- --nocapture   # Show println! output
```

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- No unsafe code without documentation
- Document public APIs

## Project Structure

```
crates/prune/src/
├── lib.rs              # Public API exports
├── main.rs             # CLI entry point
├── error.rs            # Error types (thiserror)
├── config.rs           # XDG configuration
└── db/
    ├── mod.rs          # Database trait + types
    └── schema.rs       # SQLite schema

tests/
└── common/
    └── mod.rs          # Test helpers
```

## Database Design

### Core Tables
- `drives` - Track storage locations (local/cloud)
- `files` - File metadata and classification
- `duplicate_groups` - Deduplication tracking
- `migration_plans` - Migration orchestration
- `migration_steps` - Individual operations
- `audit_log` - Complete audit trail

### Key Types
- `Drive` - Storage location with role and backend
- `File` - Complete file metadata
- `MigrationPlan` - Coordinated file operations
- `AuditLogEntry` - Audit record

## Adding New Features

### 1. Database Changes
Edit `src/db/schema.rs` and increment `SCHEMA_VERSION`.

### 2. New Types
Add to `src/db/mod.rs` and export in `src/lib.rs`.

### 3. CLI Commands
Add subcommand to `src/main.rs`.

### 4. Tests
Add test module in `tests/` or inline tests.

## Testing Philosophy

- **Unit tests** - Inline with `#[cfg(test)]`
- **Integration tests** - In `tests/` directory
- **Use test helpers** - `tests/common/mod.rs` provides fixtures
- **Temporary databases** - Always use temp DB for tests

Example test:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drive_creation() {
        let mut db = SqliteDatabase::open_in_memory().unwrap();
        db.initialize().unwrap();

        // Test code here
    }
}
```

## Common Tasks

### Add a new table

1. Edit `src/db/schema.rs` - Add CREATE TABLE
2. Add type in `src/db/mod.rs`
3. Add CRUD methods to Database trait
4. Implement in SqliteDatabase
5. Write tests

### Add a CLI command

1. Add variant to Commands enum in `src/main.rs`
2. Implement handler
3. Update --help text
4. Add integration test

### Add an error type

1. Add variant to PruneError in `src/error.rs`
2. Use thiserror attributes
3. Document when it occurs

## Dependencies

### Core
- `rusqlite` - SQLite (bundled, no system dep)
- `clap` - CLI argument parsing
- `serde` / `serde_json` - Serialization
- `chrono` - Date/time handling
- `thiserror` - Error types
- `anyhow` - Error handling in applications

### External Tools
- `rmlint` - Duplicate detection
- `rsync` - Local file operations
- `rclone` - Cloud backend integration

## Safety Invariants

These must ALWAYS hold:

1. Never delete a file without verified backup
2. Re-verify hashes before destructive operations
3. Respect space constraints (50% headroom)
4. Log all destructive operations to audit_log
5. Dry-run is default mode

## Commit Guidelines

- Logical, rebase-able commits
- Clear commit messages
- Run tests before committing
- Run `cargo fmt` and `cargo clippy`

## Nix Development

The flake provides:
- Rust toolchain (stable, latest)
- All external dependencies
- Development tools

```bash
nix develop              # Enter shell
nix flake check          # Verify flake
nix build                # Build package
```

## Questions?

See `docs/spec.md` for complete architecture.
See `PHASE1_COMPLETE.md` for foundation details.
