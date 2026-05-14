use rbrain_core::error::BrainError;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
    SqlitePool,
};
use std::path::PathBuf;
use std::time::Duration;

pub async fn open_database(db_path: &PathBuf) -> Result<SqlitePool, BrainError> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let opts = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .pragma("temp_store", "memory");

    SqlitePool::connect_with(opts)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))
}
