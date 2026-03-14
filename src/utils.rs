use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use tiktoken_rs::cl100k_base;

pub fn is_ignored_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy().to_lowercase();

    // Ignore hidden files and directories
    if path_str.starts_with('.') || path_str.contains("/.") {
        return true;
    }

    // Ignore common development directories
    if path_str.contains("target/")
        || path_str.contains("node_modules/")
        || path_str.contains("vendor/")
    {
        return true;
    }

    // Ignore asset extensions
    let asset_extensions = [
        ".png", ".jpg", ".jpeg", ".gif", ".svg", ".ico", ".webp", ".mp4", ".mov", ".webm", ".avi",
        ".mkv", ".pdf", ".zip", ".gz", ".tar", ".7z", ".woff", ".woff2", ".ttf", ".eot", ".otf",
        ".exe", ".dll", ".so", ".dylib", ".pyc", ".pyo", ".class",
    ];

    asset_extensions.iter().any(|ext| path_str.ends_with(ext))
}

pub fn get_file_hash(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 4096];

    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

pub fn chunk_text(text: &str, max_tokens: usize, overlap: usize) -> Vec<String> {
    let bpe = cl100k_base().unwrap();
    let tokens = bpe.encode_with_special_tokens(text);

    if tokens.len() <= max_tokens {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < tokens.len() {
        let end = std::cmp::min(start + max_tokens, tokens.len());
        let chunk_tokens = &tokens[start..end];
        chunks.push(bpe.decode(chunk_tokens.to_vec()).unwrap());

        if end >= tokens.len() {
            break;
        }
        start += max_tokens - overlap;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_get_file_hash() {
        let mut tmp_file = NamedTempFile::new().unwrap();
        write!(tmp_file, "hello world").unwrap();
        let hash = get_file_hash(tmp_file.path()).unwrap();
        // sha256 of "hello world"
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_chunk_text_no_split() {
        let text = "This is a short text.";
        let chunks = chunk_text(text, 100, 10);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], text);
    }

    #[test]
    fn test_chunk_text_with_split() {
        let text = "one two three four five six seven eight nine ten";
        // Each word is roughly one token in cl100k_base
        let chunks = chunk_text(text, 5, 2);
        assert!(chunks.len() > 1);
    }
}
