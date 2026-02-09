# prune

> *Carefully cut away the dead weight. Keep what matters.*

**prune** is an open-source Rust CLI tool for safely deduplicating, classifying, and restructuring large file collections. It builds a queryable index of your files, identifies duplicates and waste, helps you classify what to keep vs. archive vs. trash, and then executes verified migrations — never deleting a file until its copy is confirmed safe.

Designed for the common scenario: years of accumulated data across drives, full of duplicates and no coherent structure, that you want to clean up before migrating to new storage (ZFS, NAS, cloud, etc.).

## Features

- **Safe by design** - Hash-verified operations, audit logging, dry-run default
- **Multi-drive aware** - Track files across multiple drives and cloud backends
- **Intelligent classification** - Rule-based + AI-assisted file organization
- **Incremental migration** - Resumable, space-aware batch operations
- **Cloud integration** - rclone support for 70+ backends (S3, Google Drive, etc.)

## Installation

### From source

```bash
cargo build --release
```

### With Nix

```bash
nix develop  # Enter dev shell
nix build    # Build the package
```

## Quick Start

```bash
# Register a drive
prune drive add nas_main /mnt/nas --role source

# Scan for files and duplicates
prune scan nas_main

# View status
prune status

# Classify files
prune classify --auto

# Create and execute migration plans
prune plan create --phase delete_trash
prune migrate <plan_id> --execute
```

## Documentation

See [docs/spec.md](docs/spec.md) for complete architecture and design documentation.

## License

MIT
