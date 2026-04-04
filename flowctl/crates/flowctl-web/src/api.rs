//! API client for communicating with the flowctl daemon.
//!
//! Uses `gloo-net` on WASM (client-side) for fetch requests.
//! On the server side (SSR), these functions won't be called directly.

use serde::{Deserialize, Serialize};

/// API base URL — defaults to same origin.
#[allow(dead_code)]
fn api_base() -> String {
    // In the browser, use relative URLs (same origin).
    // Can be overridden via window.__FLOWCTL_API for dev.
    String::new()
}

/// Epic summary from the /api/v1/epics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpicSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub tasks: usize,
    pub done: usize,
}

/// Epics list response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpicsResponse {
    pub epics: Vec<EpicSummary>,
    pub count: usize,
    pub success: bool,
}

/// Task from the /api/v1/tasks endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskItem {
    pub id: String,
    pub title: String,
    pub status: String,
    pub epic: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub domain: String,
}

/// Fetch all epics from the daemon API.
#[cfg(feature = "hydrate")]
pub async fn fetch_epics() -> Result<Vec<EpicSummary>, String> {
    let url = format!("{}/api/v1/epics", api_base());
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;

    // The epics endpoint returns a JSON array or {epics: [...]}
    if let Some(arr) = data.as_array() {
        serde_json::from_value(serde_json::Value::Array(arr.clone()))
            .map_err(|e| format!("parse error: {e}"))
    } else if let Some(epics) = data.get("epics") {
        serde_json::from_value(epics.clone())
            .map_err(|e| format!("parse error: {e}"))
    } else {
        Err("unexpected response format".to_string())
    }
}

/// Fetch tasks for an epic.
#[cfg(feature = "hydrate")]
pub async fn fetch_tasks(epic_id: &str) -> Result<Vec<TaskItem>, String> {
    let url = format!("{}/api/v1/tasks?epic_id={}", api_base(), epic_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;

    if let Some(arr) = data.as_array() {
        serde_json::from_value(serde_json::Value::Array(arr.clone()))
            .map_err(|e| format!("parse error: {e}"))
    } else {
        serde_json::from_value(data).map_err(|e| format!("parse error: {e}"))
    }
}

/// Start a task via POST.
#[cfg(feature = "hydrate")]
pub async fn start_task(task_id: &str) -> Result<(), String> {
    let url = format!("{}/api/v1/tasks/start", api_base());
    let body = serde_json::json!({"task_id": task_id});
    let resp = gloo_net::http::Request::post(&url)
        .json(&body)
        .map_err(|e| format!("json error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if resp.ok() { Ok(()) } else { Err(format!("HTTP {}", resp.status())) }
}

/// Complete a task via POST.
#[cfg(feature = "hydrate")]
pub async fn done_task(task_id: &str) -> Result<(), String> {
    let url = format!("{}/api/v1/tasks/done", api_base());
    let body = serde_json::json!({"task_id": task_id});
    let resp = gloo_net::http::Request::post(&url)
        .json(&body)
        .map_err(|e| format!("json error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if resp.ok() { Ok(()) } else { Err(format!("HTTP {}", resp.status())) }
}

// SSR stubs — these won't be called on the server but need to exist for compilation.
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_epics() -> Result<Vec<EpicSummary>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_tasks(_epic_id: &str) -> Result<Vec<TaskItem>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn start_task(_task_id: &str) -> Result<(), String> { Ok(()) }
#[cfg(not(feature = "hydrate"))]
pub async fn done_task(_task_id: &str) -> Result<(), String> { Ok(()) }
