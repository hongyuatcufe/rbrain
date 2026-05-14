use async_trait::async_trait;
use rbrain_core::error::Result;
use rbrain_core::vector_store::VectorStore;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct UsearchStore {
    index: Mutex<usearch::Index>,
    path: PathBuf,
    dim: usize,
}

impl UsearchStore {
    pub fn new(path: PathBuf, dim: usize) -> Result<Self> {
        let index = usearch::Index::new(&usearch::IndexOptions {
            dimensions: dim,
            metric: usearch::MetricKind::Cos,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 128,
            multi: false,
            quantization: usearch::ScalarKind::F32,
        }).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch init: {}", e))
        ))?;

        // Pre-reserve capacity so the HNSW graph is properly initialised.
        // usearch can SIGSEGV on remove() when the index has no reserved capacity.
        index.reserve(64).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch reserve: {}", e))
        ))?;

        let store = Self {
            index: Mutex::new(index),
            path: path.clone(),
            dim,
        };

        if path.exists() {
            store.load()?;
        }

        Ok(store)
    }
}

#[async_trait]
impl VectorStore for UsearchStore {
    async fn upsert(&self, chunk_id: i64, embedding: &[f32]) -> Result<()> {
        let idx = self.index.lock().unwrap();
        if idx.size() >= idx.capacity() {
            let reserve_to = (idx.capacity() + 256).next_power_of_two();
            idx.reserve(reserve_to).map_err(|e| rbrain_core::error::BrainError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("usearch reserve: {}", e))
            ))?;
        }
        idx.add(chunk_id as u64, embedding).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch add: {}", e))
        ))?;
        Ok(())
    }

    async fn upsert_batch(&self, items: &[(i64, Vec<f32>)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let idx = self.index.lock().unwrap();
        // Ensure there is enough capacity for the whole batch before inserting.
        let needed = idx.size() + items.len();
        if needed > idx.capacity() {
            let reserve_to = (needed + 256).next_power_of_two();
            idx.reserve(reserve_to).map_err(|e| rbrain_core::error::BrainError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("usearch reserve: {}", e))
            ))?;
        }
        for (id, vec) in items {
            idx.add(*id as u64, vec).map_err(|e| rbrain_core::error::BrainError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("usearch add: {}", e))
            ))?;
        }
        Ok(())
    }

    async fn delete(&self, chunk_id: i64) -> Result<()> {
        let idx = self.index.lock().unwrap();
        if idx.size() > 0 {
            idx.remove(chunk_id as u64).map_err(|e| rbrain_core::error::BrainError::Io(
                std::io::Error::new(std::io::ErrorKind::Other, format!("usearch remove: {}", e))
            ))?;
        }
        Ok(())
    }

    async fn search(&self, query: &[f32], k: usize) -> Result<Vec<(i64, f32)>> {
        let idx = self.index.lock().unwrap();
        let results = idx.search(query, k).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch search: {}", e))
        ))?;
        let keys: Vec<u64> = results.keys.to_vec();
        let distances: Vec<f32> = results.distances.to_vec();
        Ok(keys.iter().zip(distances.iter()).map(|(k, d)| (*k as i64, *d)).collect())
    }

    async fn save(&self) -> Result<()> {
        let idx = self.index.lock().unwrap();
        let path_str = self.path.to_str().unwrap_or("");
        idx.save(path_str).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch save: {}", e))
        ))?;
        Ok(())
    }

    fn load(&self) -> Result<()> {
        let idx = self.index.lock().unwrap();
        let path_str = self.path.to_str().unwrap_or("");
        idx.load(path_str).map_err(|e| rbrain_core::error::BrainError::Io(
            std::io::Error::new(std::io::ErrorKind::Other, format!("usearch load: {}", e))
        ))?;
        Ok(())
    }
}
