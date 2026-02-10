# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development

```bash
cargo build                     # Debug build
cargo build --release           # Release build
cargo check                     # Fast type checking
cargo clippy                    # Lint (fix all warnings)
cargo fmt                       # Format (required before commit)
```

### Testing

```bash
cargo test                              # All tests
cargo test -p ordne                     # Library/CLI crate only
cargo test -p ordne-mcp                 # MCP crate only
cargo test db::                         # Tests matching module path
cargo test test_drive_crud              # Single test by name
cargo test -- --nocapture               # Show println! output
```

### Running

```bash
cargo run -- --help                     # CLI help
cargo run -- status                     # Example command
cargo run -- --db /path/to/db status    # Custom DB path
ORDNE_DB=/path/to/db cargo run -- status  # Env var override
```

### Nix

```bash
nix develop                     # Dev shell with all deps (Rust, rmlint, rsync, rclone)
nix build                       # Build package
```

## Architecture

Cargo workspace with two crates:

- **`crates/ordne`** — Library (`ordne_lib`) + CLI binary. All domain logic lives in the library; the binary is a thin clap wrapper.
- **`crates/ordne-mcp`** — MCP server binary exposing the same library logic as AI-callable tools via `rmcp`.

### Module Layout (`crates/ordne/src/`)

| Module | Purpose |
|---|---|
| `db/` | SQLite data layer — `Database` trait, `SqliteDatabase` impl, schema, per-table CRUD modules (`drives`, `files`, `duplicates`, `plans`, `audit`) |
| `index/` | File discovery — device detection (`/dev/disk/by-id`, `blkid`, `lsblk`), filesystem scanning (`walkdir`), MD5/blake3 hashing, rmlint JSON ingestion |
| `classify/` | Rule engine — TOML-based glob/extension/age/size rules (`rules.rs`), interactive TUI classification (`interactive.rs`) |
| `migrate/` | Migration engine — plan generation (`planner.rs`), execution loop (`engine.rs`), rsync/rclone wrappers, hash verification, space management, rollback |
| `cli/` | Command handlers — one file per subcommand, `Commands` enum in `mod.rs` dispatches to each |
| `config.rs` | XDG Base Directory config, `ORDNE_DB` env var, `--db` flag resolution |
| `error.rs` | `OrdneError` enum (thiserror) and `Result<T>` alias |

### Database

Single SQLite file (rusqlite with bundled feature). 7 tables: `drives`, `files`, `duplicate_groups`, `migration_plans`, `migration_steps`, `audit_log`, `schema_version`. Schema defined in `db/schema.rs`. Extended query methods live in per-table modules (`db/drives.rs`, `db/files.rs`, etc.) separate from the core `Database` trait.

### Key Patterns

- **`Database` trait** (`db/mod.rs:341`) — core CRUD interface; `SqliteDatabase` implements it. Tests use `SqliteDatabase::open_in_memory()`.
- **Extension traits** — `AuditDatabase` and `PlansDatabase` add methods to `SqliteDatabase` beyond the base trait.
- **External tool wrapping** — rsync and rclone are invoked as subprocesses (`migrate/rsync.rs`, `migrate/rclone.rs`), not linked.
- **Dry-run default** — migration `--execute` flag is required for real operations; omitting it shows what would happen.
- **Classification rules** — parsed from TOML (`ordne.toml`), matched via `globset`. See `ordne.toml.example` for the full rule schema.

### Test Infrastructure

Integration tests in `tests/integration/` with shared fixtures in `tests/common/mod.rs`:
- `TestFixture` — temp directory + initialized SQLite database
- `create_test_drive()`, `create_test_file()`, `create_test_file_with_hash()`, `create_test_duplicate_group()` — helper functions that insert directly via `Connection`

### Safety Invariants

These must always hold and are tested in `tests/integration/safety_test.rs`:
1. No file deleted without verified backup
2. Re-verify hashes before destructive operations
3. Respect 50% space headroom
4. All destructive operations logged to `audit_log`
5. Dry-run is the default mode

### External Tool Dependencies

`rmlint` (duplicate detection), `rsync` (local file ops), `rclone` (cloud backends). All available in the Nix dev shell.
