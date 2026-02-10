# API Reference

The Rust API reference for `ordne_lib` (and workspace crates) is generated with rustdoc.

**Generate Locally**
```bash
cargo doc --workspace --no-deps
```

Open the generated docs:
- `target/doc/ordne_lib/index.html`
- `target/doc/ordne_mcp/index.html` (MCP server crate)

**GitHub Pages (Recommended)**
This repo includes a GitHub Actions workflow that builds rustdoc and publishes it to GitHub Pages.

Setup steps:
1. Ensure GitHub Pages is enabled for the repository.
2. Set the Pages source to `GitHub Actions`.
3. Push to `main` or run the workflow manually.

The published site serves the `target/doc` output.
