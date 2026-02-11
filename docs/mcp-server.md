# MCP Server

The ordne MCP (Model Context Protocol) server exposes ordne functionality to AI agents like Claude Code through a standardized protocol.

## Status

**Current State:** ✅ Compiling and functional
**Tools Implemented:** 23 working
**Last Updated:** 2026-02-11

## Architecture

- **Crate:** `crates/ordne-mcp/`
- **Transport:** stdio (stdin/stdout)
- **Protocol:** MCP 0.12
- **Concurrency:** Thread-safe database access via `Arc<Mutex<SqliteDatabase>>`
- **Async Runtime:** tokio

## Available Tools

### Status & Information (3 tools)
- ✅ `status` - System status overview with drive, file, and plan statistics
- ✅ `drive_list` - List all registered drives with online/offline status
- ✅ `space_check` - Check available space on drives

### Indexing (2 tools)
- ✅ `drive_add` - Register a new drive (local or rclone remote)
- ✅ `scan` - Scan files on a drive or all drives

### Querying (4 tools)
- ✅ `query_unclassified` - List files needing classification
- ✅ `query_duplicates` - Find duplicate file groups
- ✅ `query_files` - Query files by category, extension, size, or path pattern
- ✅ `query_backup_unique` - Find files unique to backup drives

### Classification (3 tools)
- ✅ `classify_auto` - Auto-classify with rules from file
- ✅ `classify` - Manually classify files by ID
- ✅ `classify_pattern` - Classify files matching a glob pattern

### Migration Planning (4 tools)
- ✅ `plan_create` - Create migration plan
- ✅ `plan_show` - Show plan details
- ✅ `plan_approve` - Approve a plan for execution

### Execution (2 tools)
- ✅ `migrate_execute` - Execute approved migration plan
- ✅ `rollback` - Rollback a completed migration plan

### Verification (2 tools)
- ✅ `verify` - Verify file hashes on a drive
- ✅ `report` - Generate status report

### Policy (3 tools)
- ✅ `policy_validate` - Validate a policy file
- ✅ `policy_show` - Show a policy file
- ✅ `policy_apply` - Apply a policy to create plans (and optionally execute)

All listed tools are implemented. Planned MCP work is tracked in `TODO.md`.

## Configuration

### Claude Code Integration

Add to `~/.config/claude-code/mcp.json`:

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

### NixOS Integration

The MCP server is included in the flake outputs:

```nix
{
  packages.x86_64-linux.ordne-mcp = pkgs.rustPlatform.buildRustPackage {
    pname = "ordne-mcp";
    # ...
  };
}
```

## Usage Examples

### From Claude Code

```
Use the ordne MCP server to:
- Check system status
- Scan a new drive
- Find duplicate files
- Create and execute migration plans
```

### Direct Invocation

```bash
# Run MCP server (stdio mode)
ordne-mcp --db ~/.local/share/ordne/ordne.db

# With custom database path
ordne-mcp --db /custom/path/ordne.db
```

## Tool Details

### Status Tools

**status**
- Returns: System overview with drive count, file count, duplicate statistics, classification status, and active plans
- Parameters: None

**drive_list**
- Returns: Array of drives with label, role, backend type, online status, mount path, and capacity
- Parameters: None

**space_check**
- Returns: Space usage per drive with used/available/total bytes
- Parameters: None

### Indexing Tools

**drive_add**
- Parameters: `label` (string), `mount_path` (string), `role` (source/target/backup/offload), `backend` (local/rclone)
- Returns: Drive ID and registration confirmation

**scan**
- Parameters: `drive_label` (optional), `scan_all` (boolean)
- Returns: Files scanned, directories scanned, bytes scanned

### Query Tools

**query_duplicates**
- Parameters: `min_size_bytes` (optional), `limit` (optional)
- Returns: Array of duplicate groups with file count, total size, and file lists

**query_files**
- Parameters: `category`, `extension`, `min_size`, `max_size`, `path_pattern`, `limit`
- Returns: Array of files matching criteria

**query_backup_unique**
- Parameters: `backup_drive` (string)
- Returns: Files that exist only on the backup drive

### Classification Tools

**classify_pattern**
- Parameters: `pattern` (glob), `category`, `subcategory`, `priority`
- Returns: Number of files classified

### Migration Tools

**plan_create**
- Parameters: `phase` (plan type), `source_drive` (optional), `target_drive` (optional), `category_filter` (optional), `duplicate_group` (optional), `original_file` (optional), `batch_size` (optional)
- Returns: Plan ID and status (draft)

**plan_show**
- Parameters: `plan_id` (i64)
- Returns: Plan details with status, file counts, byte counts, and up to 50 steps

**plan_approve**
- Parameters: `plan_id` (i64)
- Returns: Confirmation of approval

**migrate_execute**
- Parameters: `plan_id` (i64), `execute` (boolean, default true for dry-run)
- Returns: Execution results with completed files/bytes

**rollback**
- Parameters: `plan_id` (i64)
- Returns: Rollback confirmation

### Policy Tools

**policy_apply**
- Parameters: `path` (string), `dry_run` (boolean, optional), `execute` (boolean, optional)
- Returns: Plan IDs created; executes plans when `dry_run` or `execute` is true

## Implementation Notes

### Database Access Pattern

The server uses interior mutability with `Arc<Mutex<SqliteDatabase>>` to enable concurrent tool execution:

```rust
fn with_db<F, R>(&self, f: F) -> R
where
    F: FnOnce(&SqliteDatabase) -> R,
{
    let db = self.db.lock().unwrap();
    f(&*db)
}
```

### Error Handling

All tools return `Result<String, String>` where:
- `Ok(String)` - JSON-formatted success response
- `Err(String)` - Human-readable error message

### JSON Schema

All tool parameters are decorated with `#[derive(JsonSchema)]` for automatic schema generation via the `schemars` crate.
Planned MCP work is tracked in `TODO.md`.

## Testing

The MCP server can be tested using the MCP inspector:

```bash
npx @modelcontextprotocol/inspector ordne-mcp --db test.db
```

## Dependencies

- `rmcp` 0.12 - MCP protocol implementation
- `tokio` - Async runtime
- `schemars` - JSON schema generation
- `rusqlite` - Database access
- `ordne` (local) - Core library

## See Also

- [MCP Specification](https://spec.modelcontextprotocol.io/)
- [ordne CLI Documentation](../README.md)
- [Project Specification](spec.md)
