use serde::{Deserialize, Serialize};
use anyhow::{Result, Context};
use std::env;

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

pub struct Embedder {
    client: reqwest::Client,
    api_base: String,
    api_key: String,
    model: String,
}

impl Embedder {
    pub fn new() -> Self {
        let api_base = env::var("EMBEDDING_API_BASE")
            .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());
        let api_key = env::var("EMBEDDING_API_KEY")
            .expect("EMBEDDING_API_KEY must be set");
        let model = env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "text-embedding-3-large".to_string());

        Self {
            client: reqwest::Client::new(),
            api_base,
            api_key,
            model,
        }
    }

    pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.api_base.trim_end_matches('/'));
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&EmbeddingRequest {
                model: self.model.clone(),
                input: vec![text.to_string()],
            })
            .send()
            .await
            .context("Failed to send embedding request")?
            .json::<EmbeddingResponse>()
            .await
            .context("Failed to parse embedding response")?;

        response.data.into_iter().next()
            .map(|d| d.embedding)
            .context("No embedding found in response")
    }

    pub async fn get_embeddings_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        
        let url = format!("{}/embeddings", self.api_base.trim_end_matches('/'));
        let response = self.client
            .post(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&EmbeddingRequest {
                model: self.model.clone(),
                input: texts,
            })
            .send()
            .await
            .context("Failed to send batch embedding request")?
            .json::<EmbeddingResponse>()
            .await
            .context("Failed to parse batch embedding response")?;

        Ok(response.data.into_iter().map(|d| d.embedding).collect())
    }
}
