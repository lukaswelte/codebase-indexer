use std::path::PathBuf;
use std::time::Duration;
use notify::{Watcher, RecursiveMode, Config, Event};
use std::sync::mpsc::channel;
use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::logic::{index_codebase, search_codebase, sync_files};
use crate::db::{get_db, get_or_create_table};
use crate::embeddings::Embedder;
use crate::utils::{get_file_hash, is_ignored_path};
use std::sync::Arc;

pub mod utils;
pub mod embeddings;
pub mod db;
pub mod logic;

#[derive(Parser)]

#[command(name = "codebase-indexer")]
#[command(about = "Codebase Indexer CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan the codebase and update the vector index
    Index {
        /// Root directory to index
        #[arg(short, long, default_value = ".")]
        root: PathBuf,
    },
    /// Monitor the codebase and update the index in real-time
    Watch {
        /// Root directory to watch
        #[arg(short, long, default_value = ".")]
        root: PathBuf,
    },
    /// Search for relevant code snippets
    Search {
        /// Search query
        query: String,
        /// Number of results to return
        #[arg(short, long, default_value_t = 5)]
        limit: usize,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

    let db_dir = std::env::current_dir()?.join(".codebase_indexer");
    if !db_dir.exists() {
        std::fs::create_dir_all(&db_dir)?;
    }
    let db_path = db_dir.join("lancedb").to_string_lossy().to_string();

    match cli.command {
        Commands::Index { root } => {
            let root = root.canonicalize()?;
            index_codebase(&root, &db_path).await?;
            println!("Indexing complete.");
        }
        Commands::Watch { root } => {
            let root = root.canonicalize()?;
            println!("Starting initial sync for {:?}...", root);
            index_codebase(&root, &db_path).await?;
            println!("Initial sync complete. Watching for changes...");

            let (tx, rx) = channel();
            let mut watcher = notify::RecommendedWatcher::new(tx, Config::default())?;
            watcher.watch(&root, RecursiveMode::Recursive)?;

            let db = get_db(&db_path).await?;
            let table = get_or_create_table(&db).await?;
            let embedder = Arc::new(Embedder::new());

            loop {
                if let Ok(res) = rx.recv() {
                    let event: Event = res?;
                    // Debounce
                    std::thread::sleep(Duration::from_secs(2));
                    let mut events = vec![event];
                    while let Ok(Ok(e)) = rx.try_recv() {
                        events.push(e);
                    }
                    
                    let mut to_index = Vec::new();
                    let mut to_delete = Vec::new();

                    for e in events {
                        for path in e.paths {
                            if path.is_dir() {
                                continue;
                            }
                            
                            let rel_path = match path.strip_prefix(&root) {
                                Ok(p) => p.to_path_buf(),
                                Err(_) => continue,
                            };

                            if is_ignored_path(&rel_path) {
                                continue;
                            }
                            
                            let rel_path_str = rel_path.to_string_lossy().to_string();

                            match e.kind {
                                notify::EventKind::Modify(_) | notify::EventKind::Create(_) => {
                                    if path.exists() {
                                        let hash = get_file_hash(&path)?;
                                        to_delete.push(rel_path_str.clone());
                                        to_index.push((rel_path_str, hash));
                                    }
                                }
                                notify::EventKind::Remove(_) => {
                                    to_delete.push(rel_path_str);
                                }
                                _ => {}
                            }
                        }
                    }

                    if !to_index.is_empty() || !to_delete.is_empty() {
                        println!("Syncing {} additions and {} deletions...", to_index.len(), to_delete.len());
                        sync_files(&root, &table, Arc::clone(&embedder), to_index, to_delete).await?;
                        println!("Sync complete.");
                    }
                }
            }
        }
        Commands::Search { query, limit } => {
            search_codebase(&query, &db_path, limit).await?;
        }
    }

    Ok(())
}
