use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rbrain_core::error::{BrainError, Result};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;
use tracing::{error, info, warn};

pub mod handlers;
pub use handlers::{EmbedPageHandler, ExtractLinksHandler, SyncRepoHandler};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Job {
    pub id: i64,
    pub queue: String,
    pub name: String,
    pub params: String,
    pub status: JobStatus,
    pub priority: i32,
    pub attempts: i32,
    pub max_attempts: i32,
    pub last_error: Option<String>,
    pub parent_job_id: Option<i64>,
    pub depth: i32,
    pub result: Option<String>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub heartbeat_at: Option<DateTime<Utc>>,
    pub idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[repr(i32)]
pub enum JobStatus {
    Pending = 1,
    Running = 2,
    Done = 3,
    Failed = 4,
    Cancelled = 5,
}

impl std::fmt::Display for JobStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JobStatus::Pending => write!(f, "pending"),
            JobStatus::Running => write!(f, "running"),
            JobStatus::Done => write!(f, "done"),
            JobStatus::Failed => write!(f, "failed"),
            JobStatus::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for JobStatus {
    type Err = BrainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "pending" => Ok(JobStatus::Pending),
            "running" => Ok(JobStatus::Running),
            "done" => Ok(JobStatus::Done),
            "failed" => Ok(JobStatus::Failed),
            "cancelled" => Ok(JobStatus::Cancelled),
            _ => Err(BrainError::Conflict(format!("invalid job status: {}", s))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JobStats {
    pub pending: i64,
    pub running: i64,
    pub done: i64,
    pub failed: i64,
    pub cancelled: i64,
}

#[derive(Debug)]
pub struct JobQueue {
    db: SqlitePool,
}

impl JobQueue {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }

    pub async fn submit_job(
        &self,
        name: &str,
        params: &serde_json::Value,
        queue: Option<&str>,
        priority: Option<i32>,
        parent_job_id: Option<i64>,
        idempotency_key: Option<&str>,
    ) -> Result<i64> {
        let params_str = serde_json::to_string(params)?;
        let queue = queue.unwrap_or("default");
        let priority = priority.unwrap_or(0);
        let depth = if let Some(parent_id) = parent_job_id {
            let parent_depth: i32 = sqlx::query_scalar("SELECT depth FROM jobs WHERE id = ?1")
                .bind(parent_id)
                .fetch_one(&self.db)
                .await
                .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            parent_depth + 1
        } else {
            0
        };

        let id: i64 = if let Some(key) = idempotency_key {
            sqlx::query_scalar(
                "INSERT INTO jobs (queue, name, params, status, priority, parent_job_id, depth, created_at, idempotency_key) \
                 VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, datetime('now'), ?7) \
                 ON CONFLICT (idempotency_key) DO UPDATE SET id = id \
                 RETURNING id"
            )
            .bind(queue)
            .bind(name)
            .bind(&params_str)
            .bind(priority)
            .bind(parent_job_id)
            .bind(depth)
            .bind(key)
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        } else {
            sqlx::query_scalar(
                "INSERT INTO jobs (queue, name, params, status, priority, parent_job_id, depth, created_at) \
                 VALUES (?1, ?2, ?3, 'pending', ?4, ?5, ?6, datetime('now')) \
                 RETURNING id"
            )
            .bind(queue)
            .bind(name)
            .bind(&params_str)
            .bind(priority)
            .bind(parent_job_id)
            .bind(depth)
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?
        };

        Ok(id)
    }

    pub async fn claim_job(&self, queue: &str) -> Result<Option<Job>> {
        let job: Option<Job> = sqlx::query_as(
            "UPDATE jobs SET status = 'running', started_at = datetime('now'), heartbeat_at = datetime('now'), attempts = attempts + 1 \
             WHERE id = (SELECT id FROM jobs WHERE queue = ?1 AND status = 'pending' ORDER BY priority DESC, id LIMIT 1) \
             RETURNING *"
        )
        .bind(queue)
        .fetch_optional(&self.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(job)
    }

    pub async fn complete_job(&self, job_id: i64, result: Option<&serde_json::Value>) -> Result<()> {
        let result_str = result.map(|r| r.to_string());
        
        sqlx::query(
            "UPDATE jobs SET status = 'done', finished_at = datetime('now'), result = ?1 WHERE id = ?2"
        )
        .bind(result_str)
        .bind(job_id)
        .execute(&self.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    pub async fn fail_job(&self, job_id: i64, error: &str) -> Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = 'failed', finished_at = datetime('now'), last_error = ?1 WHERE id = ?2"
        )
        .bind(error)
        .bind(job_id)
        .execute(&self.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    pub async fn retry_or_fail_job(&self, job_id: i64, error: &str) -> Result<JobStatus> {
        let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id = ?1")
            .bind(job_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        if job.attempts >= job.max_attempts {
            self.fail_job(job_id, error).await?;
            Ok(JobStatus::Failed)
        } else {
            sqlx::query(
                "UPDATE jobs SET status = 'pending', started_at = NULL, heartbeat_at = NULL, last_error = ?1 WHERE id = ?2"
            )
            .bind(error)
            .bind(job_id)
            .execute(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            Ok(JobStatus::Pending)
        }
    }

    pub async fn cancel_job(&self, job_id: i64) -> Result<()> {
        let mut tx = self.db.begin().await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        self.cancel_job_recursive(&mut tx, job_id).await?;

        tx.commit().await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    async fn cancel_job_recursive(&self, tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, job_id: i64) -> Result<()> {
        let child_ids: Vec<i64> = sqlx::query_scalar("SELECT id FROM jobs WHERE parent_job_id = ?1")
            .bind(job_id)
            .fetch_all(&mut **tx)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        for child_id in child_ids {
            Box::pin(self.cancel_job_recursive(tx, child_id)).await?;
        }

        sqlx::query(
            "UPDATE jobs SET status = 'cancelled', finished_at = datetime('now') WHERE id = ?1 AND status IN ('pending', 'running')"
        )
        .bind(job_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    pub async fn get_job(&self, job_id: i64) -> Result<Job> {
        let job: Job = sqlx::query_as("SELECT * FROM jobs WHERE id = ?1")
            .bind(job_id)
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(job)
    }

    pub async fn list_jobs(
        &self,
        status: Option<JobStatus>,
        limit: Option<usize>,
    ) -> Result<Vec<Job>> {
        let mut query = "SELECT * FROM jobs".to_string();
        let mut conditions = Vec::new();

        if let Some(s) = status {
            conditions.push(format!("status = '{}'", s));
        }

        if !conditions.is_empty() {
            query.push_str(" WHERE ");
            query.push_str(&conditions.join(" AND "));
        }

        query.push_str(" ORDER BY created_at DESC");

        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {}", lim));
        }

        let jobs: Vec<Job> = sqlx::query_as(&query)
            .fetch_all(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(jobs)
    }

    pub async fn update_heartbeat(&self, job_id: i64) -> Result<()> {
        sqlx::query("UPDATE jobs SET heartbeat_at = datetime('now') WHERE id = ?1")
            .bind(job_id)
            .execute(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(())
    }

    pub async fn recover_stalled(&self, stall_threshold_secs: i64) -> Result<Vec<i64>> {
        let stalled_jobs: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM jobs WHERE status = 'running' AND \
             (heartbeat_at IS NULL OR datetime(heartbeat_at) < datetime('now', '-' || ?1 || ' seconds'))"
        )
        .bind(stall_threshold_secs)
        .fetch_all(&self.db)
        .await
        .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        for job_id in &stalled_jobs {
            let _ = self.retry_or_fail_job(*job_id, "Worker heartbeat timeout - job may have stalled").await;
        }

        Ok(stalled_jobs)
    }

    pub async fn get_stats(&self) -> Result<JobStats> {
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'pending'")
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let running: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'running'")
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let done: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'done'")
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'failed'")
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        let cancelled: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'cancelled'")
            .fetch_one(&self.db)
            .await
            .map_err(|e| BrainError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(JobStats {
            pending,
            running,
            done,
            failed,
            cancelled,
        })
    }
}

pub struct Worker {
    queue: Arc<JobQueue>,
    handlers: std::collections::HashMap<String, Arc<dyn JobHandler>>,
    concurrency: usize,
    poll_interval_ms: u64,
    heartbeat_interval_secs: u64,
    stall_threshold_secs: i64,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker")
            .field("concurrency", &self.concurrency)
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("heartbeat_interval_secs", &self.heartbeat_interval_secs)
            .field("stall_threshold_secs", &self.stall_threshold_secs)
            .field("handlers_count", &self.handlers.len())
            .finish_non_exhaustive()
    }
}

#[async_trait]
pub trait JobHandler: Send + Sync {
    fn name(&self) -> &str;
    async fn handle(&self, job: &Job, params: serde_json::Value) -> Result<Option<serde_json::Value>>;
}

impl Worker {
    pub fn new(queue: Arc<JobQueue>, concurrency: usize) -> Self {
        Self {
            queue,
            handlers: std::collections::HashMap::new(),
            concurrency,
            poll_interval_ms: 500,
            heartbeat_interval_secs: 5,
            stall_threshold_secs: 60,
        }
    }

    pub fn register_handler(&mut self, handler: Arc<dyn JobHandler>) {
        self.handlers.insert(handler.name().to_string(), handler);
    }

    pub async fn run(&self, shutdown: tokio::sync::watch::Receiver<bool>) -> Result<()> {
        let shutdown_rx = shutdown;
        let mut last_recovery = std::time::Instant::now();
        let recovery_interval = std::time::Duration::from_secs(30);

        info!("Worker started with concurrency {}", self.concurrency);

        loop {
            if *shutdown_rx.borrow() {
                info!("Worker shutting down");
                break;
            }

            if last_recovery.elapsed() > recovery_interval {
                match self.queue.recover_stalled(self.stall_threshold_secs).await {
                    Ok(stalled) if !stalled.is_empty() => {
                        warn!("Recovered {} stalled jobs", stalled.len());
                    }
                    Ok(_) => {}
                    Err(e) => {
                        error!("Failed to recover stalled jobs: {}", e);
                    }
                }
                last_recovery = std::time::Instant::now();
            }

            match self.queue.claim_job("default").await {
                Ok(Some(job)) => {
                    info!("Claimed job {} ({})", job.id, job.name);
                    self.process_job(&job).await;
                }
                Ok(None) => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(self.poll_interval_ms)).await;
                }
                Err(e) => {
                    error!("Failed to claim job: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_millis(self.poll_interval_ms)).await;
                }
            }
        }

        Ok(())
    }

    async fn process_job(&self, job: &Job) {
        let params: serde_json::Value = match serde_json::from_str(&job.params) {
            Ok(p) => p,
            Err(e) => {
                let _ = self.queue.fail_job(job.id, &format!("Failed to parse params: {}", e)).await;
                return;
            }
        };

        let handler = self.handlers.get(&job.name);
        if handler.is_none() {
            let _ = self.queue.fail_job(job.id, &format!("No handler registered for job type: {}", job.name)).await;
            return;
        }

        let handler = handler.unwrap();
        let job_id = job.id;
        let queue = self.queue.clone();
        let handler = handler.clone();
        let heartbeat_interval = self.heartbeat_interval_secs;

        let heartbeat_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(heartbeat_interval));
            loop {
                interval.tick().await;
                if let Err(e) = queue.update_heartbeat(job_id).await {
                    error!("Failed to update heartbeat for job {}: {}", job_id, e);
                }
            }
        });

        match handler.handle(job, params).await {
            Ok(result) => {
                heartbeat_handle.abort();
                if let Err(e) = self.queue.complete_job(job.id, result.as_ref()).await {
                    error!("Failed to complete job {}: {}", job.id, e);
                } else {
                    info!("Completed job {}", job.id);
                }
            }
            Err(e) => {
                heartbeat_handle.abort();
                let error_msg = e.to_string();
                if let Err(e) = self.queue.retry_or_fail_job(job.id, &error_msg).await {
                    error!("Failed to update job {} status: {}", job.id, e);
                } else {
                    warn!("Job {} failed: {}", job.id, error_msg);
                }
            }
        }
    }
}
