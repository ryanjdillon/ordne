mod server;

use anyhow::Result;
use clap::Parser;
use ordne_lib::Database;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "ordne-mcp")]
#[command(about = "MCP server for ordne - exposes file management operations to AI agents")]
struct Args {
    #[arg(long, help = "Path to the SQLite database file")]
    db: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    let db_path = args.db.unwrap_or_else(|| {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("ordne").unwrap();
        xdg_dirs
            .place_data_file("ordne.db")
            .expect("Failed to create data directory")
    });

    log::info!("Starting ordne MCP server");
    log::info!("Database: {:?}", db_path);

    let mut db = ordne_lib::SqliteDatabase::open(&db_path)?;
    db.initialize()?;

    let server = server::OrdneServer::new(db);

    // Serve MCP server over stdio
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    rmcp::serve_server(server, (stdin, stdout)).await?;

    Ok(())
}
