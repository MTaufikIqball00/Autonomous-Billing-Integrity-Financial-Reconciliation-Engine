use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

pub async fn connect_postgres(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(10))
        .connect(database_url)
        .await
        .context("Failed to connect to PostgreSQL")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("Failed to apply PostgreSQL migrations")?;

    Ok(pool)
}

#[derive(Clone)]
pub struct OptionalRedis {
    client: Option<redis::Client>,
}

impl OptionalRedis {
    pub async fn connect(redis_url: Option<&str>) -> Self {
        match redis_url {
            Some(url) => match redis::Client::open(url) {
                Ok(client) => {
                    match client.get_multiplexed_async_connection().await {
                        Ok(mut conn) => {
                            let ping: redis::RedisResult<String> =
                                redis::cmd("PING").query_async(&mut conn).await;
                            if ping.is_ok() {
                                tracing::info!("Redis queue backend connected");
                                return Self {
                                    client: Some(client),
                                };
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Redis unavailable; using PostgreSQL durable queue");
                        }
                    }
                    Self { client: None }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Invalid REDIS_URL; using PostgreSQL durable queue");
                    Self { client: None }
                }
            },
            None => Self { client: None },
        }
    }

    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }

    pub async fn publish_job_signal(&self, queue_name: &str, job_id: &str) {
        if let Some(client) = &self.client {
            if let Ok(mut conn) = client.get_multiplexed_async_connection().await {
                let _: redis::RedisResult<i64> = redis::cmd("LPUSH")
                    .arg(queue_name)
                    .arg(job_id)
                    .query_async(&mut conn)
                    .await;
            }
        }
    }
}
