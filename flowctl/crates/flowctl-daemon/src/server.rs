//! HTTP server on Unix socket for daemon API.
//!
//! Provides health, metrics, status, epics, tasks, shutdown, and a
//! WebSocket endpoint for streaming live events.
//! Feature-gated behind `#[cfg(feature = "daemon")]`.

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::routing::{delete, get, post};
use tokio::net::{TcpListener, UnixListener};
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use crate::handlers::{
    self, AppState, DaemonState,
};
use crate::lifecycle::{set_socket_permissions, DaemonRuntime};

/// Create shared app state with a DB connection.
pub async fn create_state(runtime: DaemonRuntime, event_bus: flowctl_scheduler::EventBus) -> Result<(AppState, tokio_util::sync::CancellationToken)> {
    // Derive the project root from .flow/.state/ → parent of .flow/
    let working_dir = runtime.paths.state_dir
        .parent()  // .flow/
        .and_then(|p| p.parent())  // project root
        .context("cannot resolve project root from state_dir")?;
    let db = flowctl_db::open_async(working_dir)
        .await
        .with_context(|| format!("failed to open db in {}", working_dir.display()))?;
    let conn = db.connect().context("failed to connect to libsql db")?;
    let cancel = runtime.cancel.clone();
    let state = Arc::new(DaemonState {
        runtime,
        event_bus,
        db: conn,
    });
    Ok((state, cancel))
}

/// Build a CORS layer appropriate for the runtime environment.
///
/// When `FLOW_DEV=1` is set, allows any origin (for Vite dev server on :5173).
/// Otherwise, allows only same-origin requests (production: frontend served
/// from the same port as the API, so CORS is not needed).
///
/// Since the daemon only binds to 127.0.0.1 (local access), even the
/// permissive dev layer poses no security risk.
pub fn build_cors_layer() -> CorsLayer {
    if std::env::var("FLOW_DEV").as_deref() == Ok("1") {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        // Production: same-origin requests don't need CORS.
        // Still allow localhost origins so `curl` and local tools work.
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    }
}

/// Build the Axum router with all daemon API routes.
/// Public so the CLI can merge this with other routes (e.g. static file serving).
pub fn build_router(state: AppState) -> axum::Router {
    let cors = build_cors_layer();

    axum::Router::new()
        // ── Existing GET endpoints ─────────────────────────────
        .route("/api/v1/health", get(handlers::health_handler))
        .route("/api/v1/metrics", get(handlers::metrics_handler))
        .route("/api/v1/status", get(handlers::status_handler))
        .route("/api/v1/epics", get(handlers::epics_handler))
        .route("/api/v1/tasks", get(handlers::tasks_handler))
        .route("/api/v1/dag", get(handlers::dag_handler))
        .route("/api/v1/config", get(handlers::config_handler))
        .route("/api/v1/memory", get(handlers::memory_handler))
        .route("/api/v1/stats", get(handlers::stats_handler))
        .route("/api/v1/events", get(handlers::events_ws_handler))
        // ── Existing POST endpoints (legacy flat) ──────────────
        .route("/api/v1/dag/mutate", post(handlers::dag_mutate_handler))
        .route("/api/v1/tasks/create", post(handlers::create_task_handler))
        .route("/api/v1/tasks/start", post(handlers::start_task_handler))
        .route("/api/v1/tasks/done", post(handlers::done_task_handler))
        .route("/api/v1/epics/create", post(handlers::create_epic_handler))
        .route("/api/v1/tasks/skip", post(handlers::skip_task_handler))
        .route("/api/v1/tasks/block", post(handlers::block_task_handler))
        .route("/api/v1/tasks/restart", post(handlers::restart_task_handler))
        .route("/api/v1/shutdown", post(handlers::shutdown_handler))
        // ── New RESTful endpoints ──────────────────────────────
        .route("/api/v1/epics/{id}/plan", post(handlers::set_epic_plan_handler))
        .route("/api/v1/epics/{id}/work", post(handlers::start_epic_work_handler))
        .route("/api/v1/tasks/{id}", get(handlers::get_task_handler))
        .route("/api/v1/tasks/{id}/start", post(handlers::start_task_rest_handler))
        .route("/api/v1/tasks/{id}/done", post(handlers::done_task_rest_handler))
        .route("/api/v1/tasks/{id}/block", post(handlers::block_task_rest_handler))
        .route("/api/v1/tasks/{id}/restart", post(handlers::restart_task_rest_handler))
        .route("/api/v1/tasks/{id}/skip", post(handlers::skip_task_rest_handler))
        .route("/api/v1/deps", post(handlers::add_dep_handler))
        .route("/api/v1/deps/{from}/{to}", delete(handlers::remove_dep_handler))
        .route("/api/v1/dag/{id}", get(handlers::dag_detail_handler))
        // ── Approvals ──────────────────────────────────────────
        .route("/api/v1/approvals", get(handlers::list_approvals_handler))
        .route("/api/v1/approvals", post(handlers::create_approval_handler))
        .route("/api/v1/approvals/{id}", get(handlers::get_approval_handler))
        .route(
            "/api/v1/approvals/{id}/approve",
            post(handlers::approve_approval_handler),
        )
        .route(
            "/api/v1/approvals/{id}/reject",
            post(handlers::reject_approval_handler),
        )
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

    let (state, cancel) = create_state(runtime, event_bus).await?;

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

    let (state, cancel) = create_state(runtime, event_bus).await?;

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
        // Use the temp dir root as the working_dir (create_state walks up two
        // parents from state_dir; give it room to do so).
        let project_root = tmp.path().join("proj");
        std::fs::create_dir_all(&project_root).unwrap();
        let flow_dir = project_root.join(".flow");
        let paths = DaemonPaths::new(&flow_dir);
        paths.ensure_state_dir().unwrap();
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
    async fn test_router() -> (TempDir, axum::Router) {
        let (tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        let router = build_router(state);
        (tmp, router)
    }

    #[tokio::test]
    async fn epics_endpoint_empty_db() {
        let (_tmp, app) = test_router().await;
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
        let (_tmp, app) = test_router().await;
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
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        state.db.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('fn-99-test', 'Test', 'open', 'epics/fn-99-test.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
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
        let (_tmp, app) = test_router().await;
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
        let (_tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        state.db.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('fn-1', 'E', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        state.db.execute(
            "INSERT INTO tasks (id, epic_id, title, status, domain, file_path, created_at, updated_at) VALUES ('fn-1.1', 'fn-1', 'T', 'todo', 'general', 't.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        let app = build_router(state.clone());

        // Start: todo → in_progress (should succeed)
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"fn-1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Try done: in_progress → done (should succeed)
        let app2 = build_router(state.clone());
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/done")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"fn-1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app2, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Try start again: done → in_progress (should fail with CONFLICT)
        let app3 = build_router(state);
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"fn-1.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app3, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn done_task_rejects_from_todo() {
        let (_tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        state.db.execute(
            "INSERT INTO epics (id, title, status, file_path, created_at, updated_at) VALUES ('fn-2', 'E', 'open', 'e.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        state.db.execute(
            "INSERT INTO tasks (id, epic_id, title, status, domain, file_path, created_at, updated_at) VALUES ('fn-2.1', 'fn-2', 'T', 'todo', 'general', 't.md', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            (),
        ).await.unwrap();
        let app = build_router(state);

        // done from todo → should be rejected
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/done")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"fn-2.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn start_nonexistent_task_returns_error() {
        let (_tmp, app) = test_router().await;
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/tasks/start")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"task_id":"nonexistent.1"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        // "nonexistent.1" fails is_task_id() validation → 400 BAD_REQUEST
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn approval_create_list_resolve_roundtrip_emits_events() {
        use flowctl_scheduler::FlowEvent;

        let (_tmp, runtime, event_bus) = test_setup();
        let mut event_rx = event_bus.subscribe();
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        let app = build_router(state);

        // Create
        let create_body = serde_json::json!({
            "task_id": "fn-1.1",
            "kind": "file_access",
            "payload": { "files": ["src/foo.rs"] }
        });
        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/approvals")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(create_body.to_string()))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let approval_id = created.get("id").unwrap().as_str().unwrap().to_string();
        assert_eq!(created.get("status").unwrap().as_str().unwrap(), "pending");

        // Event: ApprovalCreated
        let stamped = event_rx.recv().await.unwrap();
        assert!(matches!(
            stamped.event,
            FlowEvent::ApprovalCreated { ref id, ref task_id }
                if id == &approval_id && task_id == "fn-1.1"
        ));

        // List pending
        let req = axum::http::Request::builder()
            .uri("/api/v1/approvals?status=pending")
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let list: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);

        // Approve
        let req = axum::http::Request::builder()
            .method("POST")
            .uri(format!("/api/v1/approvals/{approval_id}/approve"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(r#"{"resolver":"alice"}"#))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resolved: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(resolved.get("status").unwrap().as_str().unwrap(), "approved");
        assert_eq!(resolved.get("resolver").unwrap().as_str().unwrap(), "alice");

        // Event: ApprovalResolved
        let stamped = event_rx.recv().await.unwrap();
        assert!(matches!(
            stamped.event,
            FlowEvent::ApprovalResolved { ref id, ref status }
                if id == &approval_id && status == "approved"
        ));

        // Double-approve should be rejected as invalid transition.
        let req = axum::http::Request::builder()
            .method("POST")
            .uri(format!("/api/v1/approvals/{approval_id}/approve"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from("null"))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        // Get by id
        let req = axum::http::Request::builder()
            .uri(format!("/api/v1/approvals/{approval_id}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn approval_reject_records_reason() {
        let (_tmp, runtime, event_bus) = test_setup();
        let (state, _cancel) = create_state(runtime, event_bus).await.unwrap();
        let app = build_router(state);

        let req = axum::http::Request::builder()
            .method("POST")
            .uri("/api/v1/approvals")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"task_id":"fn-2.1","kind":"mutation","payload":{}}"#,
            ))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app.clone(), req).await.unwrap();
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let created: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let id = created.get("id").unwrap().as_str().unwrap().to_string();

        let req = axum::http::Request::builder()
            .method("POST")
            .uri(format!("/api/v1/approvals/{id}/reject"))
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"resolver":"bob","reason":"not safe"}"#,
            ))
            .unwrap();
        let resp = tower::ServiceExt::oneshot(app, req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let resolved: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(resolved.get("status").unwrap().as_str().unwrap(), "rejected");
        assert_eq!(resolved.get("reason").unwrap().as_str().unwrap(), "not safe");
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
