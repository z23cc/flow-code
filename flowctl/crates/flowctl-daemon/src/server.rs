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
    // Derive the project root from .flow/.state/ → parent of .flow/
    let working_dir = runtime.paths.state_dir
        .parent()  // .flow/
        .and_then(|p| p.parent())  // project root
        .context("cannot resolve project root from state_dir")?;
    let conn = flowctl_db::open(working_dir)
        .with_context(|| format!("failed to open db in {}", working_dir.display()))?;
    let cancel = runtime.cancel.clone();
    let state = Arc::new(DaemonState {
        runtime,
        event_bus,
        db: Arc::new(std::sync::Mutex::new(conn)),
    });
    Ok((state, cancel))
}

/// Build the Axum router with all daemon API routes.
/// Public so the CLI can merge this with other routes (e.g. static file serving).
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
        .route("/api/v1/dag", get(handlers::dag_handler))
        .route("/api/v1/dag/mutate", post(handlers::dag_mutate_handler))
        .route("/api/v1/tasks/create", post(handlers::create_task_handler))
        .route("/api/v1/tasks/start", post(handlers::start_task_handler))
        .route("/api/v1/tasks/done", post(handlers::done_task_handler))
        .route("/api/v1/epics/create", post(handlers::create_epic_handler))
        .route("/api/v1/tasks/skip", post(handlers::skip_task_handler))
        .route("/api/v1/tasks/block", post(handlers::block_task_handler))
        .route("/api/v1/tasks/restart", post(handlers::restart_task_handler))
        .route("/api/v1/config", get(handlers::config_handler))
        .route("/api/v1/memory", get(handlers::memory_handler))
        .route("/api/v1/stats", get(handlers::stats_handler))
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
        let _tmp = TempDir::new().unwrap();
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

    /// Create a test router backed by an in-memory-like DB (via test_setup).
    fn test_router() -> (TempDir, axum::Router) {
        let (tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).unwrap();
        let router = build_router(state);
        (tmp, router)
    }

    #[tokio::test]
    async fn epics_endpoint_empty_db() {
        let (_tmp, app) = test_router();
        let req = axum::http::Request::builder()
            .uri("/api/v1/epics")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tasks_endpoint_empty_db() {
        let (_tmp, app) = test_router();
        let req = axum::http::Request::builder()
            .uri("/api/v1/tasks")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_task_with_valid_data() {
        let (_tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).unwrap();
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('fn-99-test', 'Test', 'open', 'epics/fn-99-test.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let app = build_router(state);

        let create_body = serde_json::json!({
            "id": "fn-99-test.1",
            "epic_id": "fn-99-test",
            "title": "Test Task"
        });
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/create")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(create_body.to_string()))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_task_rejects_invalid_id() {
        let (_tmp, app) = test_router();
        let create_body = serde_json::json!({
            "id": "../../bad-id",
            "epic_id": "test-epic",
            "title": "Bad Task"
        });
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/create")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(create_body.to_string()))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn start_task_validates_transition() {
        // Setup: create epic + task in todo state, then start it (should succeed),
        // then try to start again from in_progress (should fail with CONFLICT).
        let (tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).unwrap();
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('e1', 'E', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO tasks (id, epic_id, title, status, domain, file_path, created_at, updated_at) VALUES ('e1.1', 'e1', 'T', 'todo', 'general', 't.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let app = build_router(state.clone());

        // Start: todo → in_progress (should succeed)
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"e1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Try done: in_progress → done (should succeed)
        let app2 = build_router(state.clone());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/done")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"e1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app2, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Try start again: done → in_progress (should fail with CONFLICT)
        let app3 = build_router(state);
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"e1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app3, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn done_task_rejects_from_todo() {
        let (tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).unwrap();
        {
            let conn = state.db.lock().unwrap();
            conn.execute(
                "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('e2', 'E', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
            conn.execute(
                "INSERT INTO tasks (id, epic_id, title, status, domain, file_path, created_at, updated_at) VALUES ('e2.1', 'e2', 'T', 'todo', 'general', 't.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            ).unwrap();
        }
        let app = build_router(state);

        // done from todo → should be rejected
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/done")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"e2.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn start_nonexistent_task_returns_error() {
        let (_tmp, app) = test_router();
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"nonexistent.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

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
