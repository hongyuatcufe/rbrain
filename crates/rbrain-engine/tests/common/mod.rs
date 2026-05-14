use tempfile::TempDir;
use rbrain_core::config::Config;

pub struct TestBrain {
    pub config: Config,
    pub _tmpdir: TempDir,
}

impl TestBrain {
    pub async fn new() -> Self {
        let tmpdir = TempDir::new().expect("Failed to create temp dir");
        let data_dir = tmpdir.path().join("data");
        std::fs::create_dir_all(&data_dir).expect("Failed to create data dir");

        let config = Config {
            repo_dir: tmpdir.path().join("brain"),
            data_dir: data_dir.clone(),
            db_path: data_dir.join("brain.db"),
            vectors_path: data_dir.join("vectors.usearch"),
            tantivy_dir: data_dir.join("tantivy"),
            dictionaries_dir: data_dir.join("dictionaries"),
            ..Default::default()
        };

        std::fs::create_dir_all(&config.repo_dir).expect("Failed to create brain dir");

        Self { config, _tmpdir: tmpdir }
    }
}
