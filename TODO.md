# TODO

Planned work and known gaps to review.

**CLI**
- Add interactive duplicate resolution flow for dedup plan creation.

**MCP Server**
- Add MCP resources for real-time status updates.
- Improve query capabilities and progress reporting for long-running operations.
- Support batch classification and migration operations.

**Indexing**
- Incremental scanning (detect changed files only).
- Parallel hashing for large datasets.
- MIME type detection and richer metadata capture.
- Compressed file inspection.
- Remote filesystem support beyond rclone (NFS/CIFS).

**Media**
- EXIF/video metadata extraction improvements.
- Audio fingerprinting for music libraries.
- Media transcoding (out of scope for v1).

**Ops / Integrations**
- Scheduled re-scans (systemd timer / cron).
- ZFS integration (dataset awareness, post-migrate snapshots, pool health reporting).

**Release**
- Publish crates.io package.
- Publish prebuilt GitHub Releases binaries.
