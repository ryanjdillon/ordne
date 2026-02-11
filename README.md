<h1>
  <img src="assets/ordne.svg" alt="ordne logo" width="72" style="vertical-align: middle;">
  ordne
</h1>

<p><em>order for the digital hoarder.</em></p>

**ordne** is a Rust CLI and MCP server for indexing, deduplicating, classifying, and safely migrating large file collections.

Built for messy multi-drive archives you want to clean up and organize without risking data loss.

**Features**

- **Safe operations** - Hash-verified moves, explicit `--dry-run` / `--execute`
- **Multi-drive index** - Local and rclone-backed drives
- **Classification** - Rules + interactive review
- **Migrations** - Resumable, space-aware batches
- **MCP server** - Native Model Context Protocol integration
- **Dependencies** - [Rust](https://github.com/rust-lang/rust), [clap](https://github.com/clap-rs/clap), [rusqlite](https://github.com/rusqlite/rusqlite), [tokio](https://github.com/tokio-rs/tokio), [rmcp](https://github.com/modelcontextprotocol/rmcp), [rclone](https://github.com/rclone/rclone), [rsync](https://github.com/WayneD/rsync)

**Roadmap**

- Policy-driven recurring workflows (cron/systemd)
- Incremental scanning and parallel hashing
- MCP status streaming and batch operations
- See `TODO.md` for the full list

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
See `docs/policy.md` for the draft policy schema.
See [docs/spec.md](docs/spec.md) for complete architecture and design documentation.

## License

MIT
