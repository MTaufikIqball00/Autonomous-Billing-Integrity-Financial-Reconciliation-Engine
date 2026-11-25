use abifr_backend::{
    agents::run_document_pipeline,
    config::AppConfig,
    db::{connect_postgres, OptionalRedis},
    doc_intel_client::DocIntelClient,
    routes::build_router,
    security::RateLimiter,
    state::AppState,
    storage::DocumentStorage,
};
use anyhow::Result;
use axum::http::{header, HeaderValue, Method};
use sqlx::Row;
use std::{net::SocketAddr, time::Duration};
use tokio::signal;
use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    trace::TraceLayer,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "abifr_backend=info,tower_http=info,sqlx=warn".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env()?;
    let db = connect_postgres(&config.database_url).await?;
    let redis = OptionalRedis::connect(config.redis_url.as_deref()).await;

    let storage = DocumentStorage::new(config.storage_dir.clone(), config.max_upload_bytes);
    storage.ensure_initialized().await?;

    let doc_intel = DocIntelClient::new(
        config.doc_intel_url.clone(),
        config.doc_intel_internal_token.clone(),
        config.doc_intel_timeout_secs,
    )?;

    let state = AppState {
        config: config.clone(),
        db,
        redis,
        storage,
        doc_intel,
        api_rate_limiter: RateLimiter::new(120, 60),
        login_rate_limiter: RateLimiter::new(10, 300),
    };

    state.ensure_bootstrap_users().await?;

    // Background durable job queue worker (PostgreSQL FOR UPDATE SKIP LOCKED + stale lock recovery)
    let worker_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        loop {
            interval.tick().await;

            // Recover stale worker locks (> 5 minutes)
            let _ = sqlx::query(
                r#"
                UPDATE processing_jobs
                SET status = CASE WHEN attempts >= max_attempts THEN 'dead_letter' ELSE 'failed' END,
                    last_error = COALESCE(last_error, 'Stale worker lock recovered after timeout'),
                    locked_at = NULL,
                    updated_at = NOW()
                WHERE status = 'running' AND locked_at < NOW() - INTERVAL '5 minutes'
                "#,
            )
            .execute(&worker_state.db)
            .await;

            let claimed = sqlx::query(
                r#"
                UPDATE processing_jobs
                SET status = 'running', attempts = attempts + 1, locked_at = NOW(), updated_at = NOW()
                WHERE id = (
                    SELECT id FROM processing_jobs
                    WHERE job_type = 'document_pipeline'
                      AND (status = 'queued' OR (status = 'failed' AND attempts < max_attempts))
                    ORDER BY created_at ASC
                    FOR UPDATE SKIP LOCKED
                    LIMIT 1
                )
                RETURNING id, target_id, attempts, max_attempts
                "#,
            )
            .fetch_optional(&worker_state.db)
            .await;

            if let Ok(Some(row)) = claimed {
                let job_id: Uuid = row.get("id");
                let target_id: Uuid = row.get("target_id");
                let attempts: i32 = row.get("attempts");
                let max_attempts: i32 = row.get("max_attempts");

                match run_document_pipeline(&worker_state, target_id, None).await {
                    Ok(_) => {
                        let _ = sqlx::query(
                            "UPDATE processing_jobs SET status = 'completed', locked_at = NULL, completed_at = NOW(), updated_at = NOW() WHERE id = $1",
                        )
                        .bind(job_id)
                        .execute(&worker_state.db)
                        .await;
                    }
                    Err(e) => {
                        let next_status = if attempts >= max_attempts {
                            "dead_letter"
                        } else {
                            "failed"
                        };
                        let _ = sqlx::query(
                            "UPDATE processing_jobs SET status = $2, locked_at = NULL, last_error = $3, updated_at = NOW() WHERE id = $1",
                        )
                        .bind(job_id)
                        .bind(next_status)
                        .bind(e.to_string())
                        .execute(&worker_state.db)
                        .await;
                    }
                }
            }
        }
    });

    let origin = HeaderValue::from_str(&config.frontend_origin)?;
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::HeaderName::from_static("x-csrf-token"),
        ]);

    let app = build_router(state)
        .layer(RequestBodyLimitLayer::new(config.max_upload_bytes + 1_048_576))
        .layer(cors)
        .layer(TraceLayer::new_for_http());

    let bind_addr = format!("{}:{}", config.backend_host, config.backend_port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!(address = %bind_addr, "ABIFR Rust backend listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        let _ = signal::ctrl_c().await;
        tracing::info!("Shutdown signal received; closing Rust backend");
    })
    .await?;

    Ok(())
}
