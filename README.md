<h1 align="center">
  <img src="assets/ordne.svg" alt="ordne logo" width="40" height="40" style="vertical-align: middle;">
  ordne
</h1>

<p align="center"><em>order for the digital hoarder.</em></p>

**ordne** is an open-source Rust CLI tool and MCP server for safely deduplicating, classifying, and restructuring large file collections. It builds a queryable index of your files, identifies duplicates and waste, helps you classify what to keep vs. archive vs. trash, and then executes verified migrations — never deleting a file until its copy is confirmed safe.

Designed for the common scenario: years of accumulated data across drives, full of duplicates and no coherent structure, that you want to clean up and organize before migrating to new storage.

## Features

- **Safe by design** - Hash-verified operations, audit logging, explicit `--dry-run` / `--execute`
- **Multi-drive aware** - Track files across multiple drives and cloud backends
- **Intelligent classification** - Rule-based + AI-assisted file organization
- **Incremental migration** - Resumable, space-aware batch operations
- **Cloud integration** - rclone support for 70+ backends (S3, Google Drive, etc.)
- **MCP server** - Native Model Context Protocol server for agent-driven workflows

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
ordne drive add nas_main /mnt/nas --role source

# Scan for files and duplicates
ordne scan nas_main

# View status
ordne status

# Classify files
ordne classify --auto

 # Create and execute migration plans
ordne plan create delete-trash
ordne migrate <plan_id> --execute
```

## MCP Server (Quick Setup)

1. Build the MCP binary:

```bash
cargo build --release -p ordne-mcp
```

2. Run it with your database path:

```bash
ordne-mcp --db ~/.local/share/ordne/ordne.db
```

3. Configure your MCP client (example for Claude Code):

```json
{
  "mcpServers": {
    "ordne": {
      "command": "ordne-mcp",
      "args": ["--db", "/path/to/ordne.db"]
    }
  }
}
```

For full details, see `docs/mcp-server.md`.

## Example Workflow: Clean Up One or More Drives

1. Register each drive with a role (source, backup, target):

```bash
ordne drive add photos_raid /mnt/photos --role source
ordne drive add archive_usb /mnt/archive --role backup
ordne drive add new_nas /mnt/new_nas --role target
```

2. Scan all drives to build the index:

```bash
ordne scan --all
```

3. Review duplicates and unique backups:

```bash
ordne query duplicates
ordne query backup-unique
```

4. Classify and plan deletions:

```bash
ordne classify --auto
ordne plan create delete-trash
ordne plan show <plan_id>
ordne plan approve <plan_id>
```

5. Execute the plan after review (explicit dry-run or execute required):

```bash
ordne migrate <plan_id> --execute
```

6. Verify and generate a final report:

```bash
ordne verify --drive new_nas
ordne report
```

## Documentation

See `docs/cli.md` for CLI usage details.
See `docs/api.md` for the Rust API reference and publishing workflow.
See [docs/spec.md](docs/spec.md) for complete architecture and design documentation.

## License

MIT
