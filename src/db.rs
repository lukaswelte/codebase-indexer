use arrow::datatypes::{DataType, Field, Schema, SchemaRef};
use arrow_array::RecordBatch;
use arrow_array::RecordBatchIterator;
use std::sync::Arc;
use anyhow::Result;

pub const TABLE_NAME: &str = "code_files";

pub fn get_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("file_path", DataType::Utf8, false),
        Field::new("file_hash", DataType::Utf8, false),
        Field::new("part_index", DataType::Int32, false),
        Field::new("content", DataType::Utf8, false),
        Field::new("vector", DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 3072), false),
    ]))
}

pub async fn get_db(db_path: &str) -> Result<lancedb::Connection> {
    Ok(lancedb::connect(db_path).execute().await?)
}

pub async fn get_or_create_table(db: &lancedb::Connection) -> Result<lancedb::Table> {
    let table_names: Vec<String> = db.table_names().execute().await?;
    if table_names.contains(&TABLE_NAME.to_string()) {
        Ok(db.open_table(TABLE_NAME).execute().await?)
    } else {
        let schema = get_schema();
        let empty_batch = RecordBatch::new_empty(schema.clone());
        let reader = RecordBatchIterator::new(vec![Ok(empty_batch)], schema);
        Ok(db.create_table(TABLE_NAME, reader).execute().await?)
    }
}
