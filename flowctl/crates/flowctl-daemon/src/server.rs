//! HTTP server on Unix socket for daemon API.
//!
//! Provides health, metrics, status, epics, tasks, shutdown, and a
//! WebSocket endpoint for streaming live events.
//! Feature-gated behind `#[cfg(feature = "daemon")]`.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::{get, post};
use tokio::net::{TcpListener, UnixListener};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::handlers::{
    self, AppState, DaemonState,
};
use crate::lifecycle::{set_socket_permissions, DaemonRuntime};

/// Create shared app state with a DB connection.
pub fn create_state(runtime: DaemonRuntime, event_bus: flowctl_scheduler::EventBus) -> Result<(AppState, tokio_util::sync::CancellationToken)> {
    let db_path = runtime.paths.state_dir.parent()
        .map(|flow_dir| flow_dir.join("flowctl.db"))
        .context("cannot resolve db path")?;
    let conn = flowctl_db::open(&db_path)
        .with_context(|| format!("failed to open db: {}", db_path.display()))?;
    let cancel = runtime.cancel.clone();
    let state = Arc::new(DaemonState {
        runtime,
        event_bus,
        db: std::sync::Mutex::new(conn),
    });
    Ok((state, cancel))
}

/// Build the Axum router with all daemon API routes.
/// Public so the CLI can merge this with other routes (e.g. Leptos SSR).
pub fn build_router(state: AppState) -> axum::Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    axum::Router::new()
        .route("/api/v1/health", get(handlers::health_handler))
        .route("/api/v1/metrics", get(handlers::metrics_handler))
        .route("/api/v1/status", get(handlers::status_handler))
        .route("/api/v1/epics", get(handlers::epics_handler))
        .route("/api/v1/tasks", get(handlers::tasks_handler))
        .route("/api/v1/tasks/create", post(handlers::create_task_handler))
        .route("/api/v1/tasks/start", post(handlers::start_task_handler))
        .route("/api/v1/tasks/done", post(handlers::done_task_handler))
        .route("/api/v1/shutdown", post(handlers::shutdown_handler))
        .route("/api/v1/events", get(handlers::events_ws_handler))
        .layer(cors)
        .with_state(state)
}

/// Start the HTTP server on a Unix socket.
///
/// Binds to the socket path from `runtime.paths.socket_file`, sets 0600
/// permissions, and serves until the cancellation token is triggered.
pub async fn serve(runtime: DaemonRuntime, event_bus: flowctl_scheduler::EventBus) -> Result<()> {
    let socket_path = runtime.paths.socket_file.clone();

    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind Unix socket: {}", socket_path.display()))?;

    // Set socket permissions to 0600 (owner only)
    set_socket_permissions(&socket_path)?;

    info!("daemon API listening on {}", socket_path.display());

    let (state, cancel) = create_state(runtime, event_bus)?;

    let router = build_router(state);

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
            info!("HTTP server shutting down");
        })
        .await
        .context("HTTP server error")?;

    Ok(())
}

/// Start the HTTP server on a TCP port (for web browser access).
pub async fn serve_tcp(
    runtime: DaemonRuntime,
    event_bus: flowctl_scheduler::EventBus,
    port: u16,
) -> Result<()> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind TCP: {addr}"))?;

    info!("daemon API listening on http://{addr}");

    let (state, cancel) = create_state(runtime, event_bus)?;

    let router = build_router(state);

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            cancel.cancelled().await;
            info!("HTTP server shutting down");
        })
        .await
        .context("HTTP server error")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::{DaemonPaths, DaemonRuntime};
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_setup() -> (TempDir, DaemonRuntime, flowctl_scheduler::EventBus) {
        let tmp = TempDir::new().unwrap();
        let flow_dir = tmp.path().join(".flow");
        let paths = DaemonPaths::new(&flow_dir);
        paths.ensure_state_dir().unwrap();
        // Create DB so create_state() works.
        let _ = flowctl_db::open(&flow_dir);
        let runtime = DaemonRuntime::new(paths);
        let (event_bus, _critical_rx) = flowctl_scheduler::EventBus::with_default_capacity();
        (tmp, runtime, event_bus)
    }

    #[tokio::test]
    async fn server_starts_and_responds_to_health() {
        let (_tmp, runtime, event_bus) = test_setup();
        let cancel = runtime.cancel.clone();
        let socket_path = runtime.paths.socket_file.clone();

        let server_handle = tokio::spawn(async move {
            serve(runtime, event_bus).await.unwrap();
        });

        // Give the server a moment to bind
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect to the socket and check health
        let stream = tokio::net::UnixStream::connect(&socket_path).await.unwrap();

        // Use hyper to send a request
        let (mut sender, conn) = hyper::client::conn::http1::handshake(
            hyper_util::rt::TokioIo::new(stream),
        )
        .await
        .unwrap();

        tokio::spawn(conn);

        let req = hyper::Request::builder()
            .uri("/api/v1/health")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Shutdown
        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
    }

    #[tokio::test]
    async fn shutdown_endpoint_triggers_cancellation() {
        let (_tmp, runtime, event_bus) = test_setup();
        let cancel = runtime.cancel.clone();
        let socket_path = runtime.paths.socket_file.clone();

        tokio::spawn(async move {
            serve(runtime, event_bus).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let stream = tokio::net::UnixStream::connect(&socket_path).await.unwrap();

        let (mut sender, conn) = hyper::client::conn::http1::handshake(
            hyper_util::rt::TokioIo::new(stream),
        )
        .await
        .unwrap();

        tokio::spawn(conn);

        let req = hyper::Request::builder()
            .method("POST")
            .uri("/api/v1/shutdown")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // The cancel token should now be triggered
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(cancel.is_cancelled());
    }

    use axum::http::StatusCode;

    #[tokio::test]
    async fn status_endpoint_returns_overview() {
        let (_tmp, runtime, event_bus) = test_setup();
        let cancel = runtime.cancel.clone();
        let socket_path = runtime.paths.socket_file.clone();

        tokio::spawn(async move {
            serve(runtime, event_bus).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let stream = tokio::net::UnixStream::connect(&socket_path).await.unwrap();
        let (mut sender, conn) = hyper::client::conn::http1::handshake(
            hyper_util::rt::TokioIo::new(stream),
        )
        .await
        .unwrap();
        tokio::spawn(conn);

        let req = hyper::Request::builder()
            .uri("/api/v1/status")
            .body(http_body_util::Empty::<bytes::Bytes>::new())
            .unwrap();

        let resp = sender.send_request(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        cancel.cancel();
    }
}
