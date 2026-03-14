use std::path::{Path, PathBuf};
use std::collections::HashMap;
use anyhow::{Result, Context};
use ignore::WalkBuilder;
use indicatif::{ProgressBar, ProgressStyle};
use arrow_array::{StringArray, Int32Array, RecordBatch, RecordBatchIterator, Array};
use futures::{stream, StreamExt};
use std::sync::Arc;
use lancedb::query::{ExecutableQuery, QueryBase};
use colored::*;

use crate::utils::{get_file_hash, chunk_text, is_ignored_path};
use crate::embeddings::Embedder;
use crate::db::{get_db, get_or_create_table, get_schema};

pub async fn list_files(root: &Path) -> Result<Vec<PathBuf>> {
    let root = root.canonicalize()?;
    let mut files = Vec::new();
    let walker = WalkBuilder::new(&root)
        .hidden(false)
        .git_ignore(true)
        .build();

    for result in walker {
        let entry = result?;
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            let rel_path = entry.path().strip_prefix(&root)?.to_path_buf();
            if !is_ignored_path(&rel_path) {
                files.push(rel_path);
            }
        }
    }
    Ok(files)
}

pub async fn index_codebase(root: &Path, db_path: &str) -> Result<()> {
    let db = get_db(db_path).await?;
    let table = get_or_create_table(&db).await?;
    let embedder = Arc::new(Embedder::new());

    let files_on_disk = list_files(root).await?;
    
    let mut batches = table.query().execute().await?;
    let mut db_files: HashMap<String, String> = HashMap::new();
    
    while let Some(batch) = batches.next().await {
        let batch = batch?;
        let path_col = batch.column_by_name("file_path")
            .context("Missing file_path column")?
            .as_any().downcast_ref::<StringArray>()
            .context("file_path not a StringArray")?;
        let hash_col = batch.column_by_name("file_hash")
            .context("Missing file_hash column")?
            .as_any().downcast_ref::<StringArray>()
            .context("file_hash not a StringArray")?;
        
        for i in 0..batch.num_rows() {
            db_files.insert(path_col.value(i).to_string(), hash_col.value(i).to_string());
        }
    }

    let mut to_delete = Vec::new();
    let mut to_index = Vec::new();

    for (path, _) in &db_files {
        if !root.join(path).exists() {
            to_delete.push(path.clone());
        }
    }

    for rel_path in files_on_disk {
        let path_str = rel_path.to_string_lossy().to_string();
        let abs_path = root.join(&rel_path);
        let current_hash = get_file_hash(&abs_path)?;
        
        if !db_files.contains_key(&path_str) || db_files.get(&path_str).unwrap() != &current_hash {
            if db_files.contains_key(&path_str) {
                to_delete.push(path_str.clone());
            }
            to_index.push((path_str, current_hash));
        }
    }

    sync_files(root, &table, embedder, to_index, to_delete).await?;
    Ok(())
}

pub async fn sync_files(
    root: &Path,
    table: &lancedb::Table,
    embedder: Arc<Embedder>,
    to_index: Vec<(String, String)>,
    to_delete: Vec<String>,
) -> Result<()> {
    if !to_delete.is_empty() {
        let paths_str = to_delete.iter()
            .map(|p| format!("'{}'", p.replace("'", "''")))
            .collect::<Vec<_>>()
            .join(", ");
        table.delete(&format!("file_path IN ({})", paths_str)).await?;
    }

    if to_index.is_empty() {
        return Ok(());
    }

    let pb = ProgressBar::new(to_index.len() as u64);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")?
        .progress_chars("#>-"));

    let root = root.to_path_buf();
    let schema = get_schema();
    let stream = stream::iter(to_index.into_iter().map(|(rel_path, file_hash)| {
        let embedder = Arc::clone(&embedder);
        let root = root.clone();
        let pb = pb.clone();
        let schema = schema.clone();
        async move {
            pb.set_message(format!("Indexing {}", rel_path));
            let abs_path = root.join(&rel_path);
            let content = std::fs::read_to_string(&abs_path).unwrap_or_default();
            let chunks = chunk_text(&content, 7000, 700);
            
            let mut row_paths = Vec::new();
            let mut row_hashes = Vec::new();
            let mut row_parts = Vec::new();
            let mut row_contents = Vec::new();
            let mut row_vectors = Vec::new();

            for (i, chunk) in chunks.into_iter().enumerate() {
                let embed_text = format!("File: {}\n\n{}", rel_path, chunk);
                let vector = embedder.get_embedding(&embed_text).await?;
                
                row_paths.push(rel_path.clone());
                row_hashes.push(file_hash.clone());
                row_parts.push(i as i32);
                row_contents.push(content.clone());
                
                let mut v_vec = Vec::new();
                for val in vector {
                    v_vec.push(Some(val));
                }
                row_vectors.push(Some(v_vec));
            }
            pb.inc(1);
            Result::<RecordBatch>::Ok(RecordBatch::try_new(
                schema,
                vec![
                    Arc::new(StringArray::from(row_paths)),
                    Arc::new(StringArray::from(row_hashes)),
                    Arc::new(Int32Array::from(row_parts)),
                    Arc::new(StringArray::from(row_contents)),
                    Arc::new(arrow_array::FixedSizeListArray::from_iter_primitive::<arrow_array::types::Float32Type, _, _>(
                        row_vectors, 3072
                    )),
                ],
            )?)
        }
    })).buffer_unordered(10);

    let mut batches = Vec::new();
    let mut res_stream = stream;
    while let Some(res) = res_stream.next().await {
        batches.push(res?);
    }

    if !batches.is_empty() {
        let reader = RecordBatchIterator::new(batches.into_iter().map(Ok), get_schema());
        table.add(reader).execute().await?;
    }
    
    pb.finish_with_message("Sync complete");

    Ok(())
}

pub async fn search_codebase(query: &str, db_path: &str, limit: usize) -> Result<()> {
    let db = get_db(db_path).await?;
    let table = get_or_create_table(&db).await?;
    let embedder = Embedder::new();
    
    let query_vector = embedder.get_embedding(query).await?;
    // Increase internal limit to ensure we get enough unique files after deduplication
    let mut results = table.vector_search(query_vector)?
        .limit(limit * 10)
        .execute()
        .await?;

    println!("\nTop results for: '{}'\n", query.bold());
    
    let mut seen_paths = std::collections::HashSet::new();
    let mut count = 0;

    while let Some(batch) = results.next().await {
        let batch = batch?;
        let path_col = batch.column_by_name("file_path")
            .context("file_path column not found")?
            .as_any().downcast_ref::<StringArray>()
            .context("file_path is not a StringArray")?;

        for i in 0..batch.num_rows() {
            let path = path_col.value(i);
            if !seen_paths.contains(path) {
                println!("📄 {}", path.cyan().bold());
                seen_paths.insert(path.to_string());
                count += 1;
                if count >= limit {
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}
