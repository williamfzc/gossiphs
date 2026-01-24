use crate::symbol::Symbol;
use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

#[derive(Serialize, Deserialize, Debug)]
pub struct CacheEntry {
    pub blob_id: String, // Git Blob OID or content hash
    pub symbols: Vec<Symbol>,
}

pub struct CacheManager {
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(project_path: &str) -> Result<Self> {
        let cache_dir = Path::new(project_path).join(".gossiphs").join("cache");
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir).context("Failed to create cache directory")?;
        }
        Ok(CacheManager { cache_dir })
    }

    pub fn get(&self, file_path: &str, blob_id: &str) -> Option<Vec<Symbol>> {
        let entry_path = self.get_entry_path(file_path);
        if !entry_path.exists() {
            return None;
        }

        let data = fs::read(&entry_path).ok()?;
        let entry: CacheEntry = bincode::deserialize(&data).ok()?;
        
        if entry.blob_id == blob_id {
            Some(entry.symbols)
        } else {
            None
        }
    }

    pub fn set(&self, file_path: &str, blob_id: &str, symbols: Vec<Symbol>) -> Result<()> {
        let entry = CacheEntry {
            blob_id: blob_id.to_string(),
            symbols,
        };
        let entry_path = self.get_entry_path(file_path);
        let data = bincode::serialize(&entry).context("Failed to serialize cache entry")?;
        fs::write(entry_path, data).context("Failed to write cache file")?;
        Ok(())
    }

    fn get_entry_path(&self, file_path: &str) -> PathBuf {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        file_path.hash(&mut hasher);
        let hash = hasher.finish();
        self.cache_dir.join(format!("{:x}.bin", hash))
    }

    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        if self.cache_dir.exists() {
            fs::remove_dir_all(&self.cache_dir).context("Failed to clear cache directory")?;
            fs::create_dir_all(&self.cache_dir).context("Failed to recreate cache directory")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::Symbol;
    use std::sync::Arc;
    use tree_sitter::Range;

    #[test]
    fn test_cache_basic() {
        let test_dir = "./.test_cache_basic";
        let cm = CacheManager::new(test_dir).unwrap();
        cm.clear().unwrap();

        let file_path = "test.rs";
        let blob_id = "abc123456";
        let range = Range {
            start_byte: 0,
            end_byte: 0,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 0 },
        };
        let symbols = vec![
            Symbol::new_def(Arc::new(file_path.to_string()), Arc::new("foo".to_string()), range)
        ];

        // set
        cm.set(file_path, blob_id, symbols.clone()).unwrap();

        // get same
        let cached = cm.get(file_path, blob_id).unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].name.as_ref(), "foo");

        // get diff blob_id
        assert!(cm.get(file_path, "wrong").is_none());

        // clear
        cm.clear().unwrap();
        assert!(cm.get(file_path, blob_id).is_none());
        
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn test_cache_corrupted_data() {
        let test_dir = "./.test_cache_corrupted";
        let cm = CacheManager::new(test_dir).unwrap();
        let file_path = "corrupted.rs";
        let entry_path = cm.get_entry_path(file_path);
        
        // Write invalid data
        fs::write(&entry_path, b"invalid data").unwrap();
        
        // Should return None instead of panicking
        assert!(cm.get(file_path, "any").is_none());
        
        fs::remove_dir_all(test_dir).unwrap();
    }

    #[test]
    fn test_cache_large_symbols() {
        let test_dir = "./.test_cache_large";
        let cm = CacheManager::new(test_dir).unwrap();
        let file_path = "large.rs";
        let blob_id = "large_blob";
        let range = Range {
            start_byte: 0,
            end_byte: 0,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 0 },
        };
        
        let mut symbols = Vec::new();
        for i in 0..1000 {
            symbols.push(Symbol::new_def(
                Arc::new(file_path.to_string()),
                Arc::new(format!("func_{}", i)),
                range
            ));
        }
        
        cm.set(file_path, blob_id, symbols.clone()).unwrap();
        let cached = cm.get(file_path, blob_id).unwrap();
        assert_eq!(cached.len(), 1000);
        assert_eq!(cached[999].name.as_ref(), "func_999");
        
        fs::remove_dir_all(test_dir).unwrap();
    }
}
