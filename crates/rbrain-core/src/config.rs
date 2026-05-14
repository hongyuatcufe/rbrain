use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QwenConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepSeekConfig {
    pub api_key: String,
    pub base_url: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub repo_dir: PathBuf,        // ~/brain
    pub data_dir: PathBuf,        // ~/.rbrain
    pub db_path: PathBuf,         // ~/.rbrain/brain.db
    pub vectors_path: PathBuf,    // ~/.rbrain/vectors.usearch
    pub tantivy_dir: PathBuf,     // ~/.rbrain/tantivy
    pub dictionaries_dir: PathBuf,// ~/.rbrain/dictionaries
    pub qwen: QwenConfig,
    pub deepseek: DeepSeekConfig,
    pub embedding_dim: usize,     // 1024, locked
    pub log_level: String,
}

impl Default for Config {
    fn default() -> Self {
        let home = home::home_dir().expect("HOME not set");
        let data_dir = home.join(".rbrain");
        Config {
            repo_dir: home.join("brain"),
            data_dir: data_dir.clone(),
            db_path: data_dir.join("brain.db"),
            vectors_path: data_dir.join("vectors.usearch"),
            tantivy_dir: data_dir.join("tantivy"),
            dictionaries_dir: data_dir.join("dictionaries"),
            qwen: QwenConfig::default(),
            deepseek: DeepSeekConfig::default(),
            embedding_dim: 1024,
            log_level: "info".to_string(),
        }
    }
}

impl Default for QwenConfig {
    fn default() -> Self {
        QwenConfig {
            api_key: String::new(),
            base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
            model: "text-embedding-v4".to_string(),
        }
    }
}

impl Default for DeepSeekConfig {
    fn default() -> Self {
        DeepSeekConfig {
            api_key: String::new(),
            base_url: "https://api.deepseek.com/v1".to_string(),
            model: "deepseek-chat".to_string(),
        }
    }
}

impl Config {
    /// Walk up from CWD looking for a `.rbrain/` directory.
    /// Returns the `.rbrain/` path if found, otherwise None.
    fn find_local_rbrain() -> Option<PathBuf> {
        let mut dir = std::env::current_dir().ok()?;
        loop {
            let candidate = dir.join(".rbrain");
            if candidate.is_dir() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Load config with the following priority (last wins):
    ///   1. Global defaults
    ///   2. Global ~/.rbrain/config.toml  (API keys, model settings)
    ///   3. Local .rbrain/ path overrides (if found by walking up from CWD)
    ///   4. Local .rbrain/config.toml     (project-level overrides)
    ///   5. RBRAIN_* environment variables
    pub fn load() -> Result<Self, figment::Error> {
        use figment::{Figment, providers::{Format, Toml, Env, Serialized}};

        let home = home::home_dir().expect("HOME not set");
        let global_config = home.join(".rbrain").join("config.toml");

        let mut fig = Figment::new()
            .merge(Serialized::defaults(Config::default()))
            .merge(Toml::file(&global_config));

        if let Some(local_dir) = Self::find_local_rbrain() {
            let project_dir = local_dir
                .parent()
                .expect("`.rbrain` has no parent")
                .to_path_buf();

            // Override every data path to point inside the local .rbrain/
            let path_overrides = serde_json::json!({
                "repo_dir":        project_dir,
                "data_dir":        local_dir,
                "db_path":         local_dir.join("brain.db"),
                "vectors_path":    local_dir.join("vectors.usearch"),
                "tantivy_dir":     local_dir.join("tantivy"),
                "dictionaries_dir": local_dir.join("dictionaries"),
            });

            fig = fig
                .merge(Serialized::globals(path_overrides))
                .merge(Toml::file(local_dir.join("config.toml")));
        }

        fig.merge(Env::prefixed("RBRAIN_").global()).extract()
    }

    /// Whether this config was resolved from a project-local `.rbrain/`.
    pub fn is_local(&self) -> bool {
        self.data_dir
            .file_name()
            .is_some_and(|n| n == ".rbrain")
    }
}
