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

/// A node in the DAG visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagNode {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default)]
    pub domain: String,
    pub x: f64,
    pub y: f64,
}

/// An edge in the DAG visualization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagEdge {
    pub from: String,
    pub to: String,
}

/// DAG response from /api/v1/dag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DagResponse {
    pub nodes: Vec<DagNode>,
    pub edges: Vec<DagEdge>,
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
/// Token usage record from the /api/v1/tokens endpoint (per-record).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsageItem {
    pub id: i64,
    pub timestamp: String,
    pub epic_id: String,
    pub task_id: Option<String>,
    pub phase: Option<String>,
    pub model: Option<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: Option<f64>,
}

/// Aggregated token usage per task from the /api/v1/tokens?epic_id=X endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskTokenSummary {
    pub task_id: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub estimated_cost: f64,
}

/// Global stats from the /api/v1/stats endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatsResponse {
    pub total_epics: i64,
    pub open_epics: i64,
    pub total_tasks: i64,
    pub done_tasks: i64,
    pub in_progress_tasks: i64,
    pub blocked_tasks: i64,
    pub total_tokens: i64,
}

/// Event row from the /api/v1/events-history endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventItem {
    pub id: i64,
    pub timestamp: String,
    pub epic_id: String,
    pub task_id: Option<String>,
    pub event_type: String,
    pub actor: Option<String>,
    pub payload: Option<String>,
}


/// A memory entry from /api/v1/memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    #[serde(rename = "type", default)]
    pub entry_type: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub module: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub problem_type: Option<String>,
    #[serde(default)]
    pub track: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Fetch memory entries with optional filters.
#[cfg(feature = "hydrate")]
pub async fn fetch_memory(track: Option<&str>, module: Option<&str>) -> Result<Vec<MemoryEntry>, String> {
    let mut url = format!("{}/api/v1/memory", api_base());
    let mut params = Vec::new();
    if let Some(t) = track { params.push(format!("track={t}")); }
    if let Some(m) = module { params.push(format!("module={m}")); }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;

    if let Some(entries) = data.get("entries") {
        serde_json::from_value(entries.clone()).map_err(|e| format!("parse error: {e}"))
    } else if let Some(arr) = data.as_array() {
        serde_json::from_value(serde_json::Value::Array(arr.clone())).map_err(|e| format!("parse error: {e}"))
    } else {
        Ok(vec![])
    }
}

/// Fetch global stats from the daemon API.
#[cfg(feature = "hydrate")]
pub async fn fetch_stats() -> Result<StatsResponse, String> {
    let url = format!("{}/api/v1/stats", api_base());
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json().await.map_err(|e| format!("json error: {e}"))
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

/// Fetch DAG layout for an epic.
#[cfg(feature = "hydrate")]
pub async fn fetch_dag(epic_id: &str) -> Result<DagResponse, String> {
    let url = format!("{}/api/v1/dag?epic_id={}", api_base(), epic_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json().await.map_err(|e| format!("json error: {e}"))
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

/// Mutate the DAG (add/remove dep, retry/skip task) via POST.
#[cfg(feature = "hydrate")]
pub async fn mutate_dag(action: &str, params: serde_json::Value, version: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/v1/dag/mutate", api_base());
    let body = serde_json::json!({
        "action": action,
        "params": params,
        "version": version,
    });
    let resp = gloo_net::http::Request::post(&url)
        .json(&body)
        .map_err(|e| format!("json error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    let status = resp.status();
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;

    if status == 409 {
        return Err(format!("conflict: {}", json.get("error").and_then(|v| v.as_str()).unwrap_or("version mismatch")));
    }
    if status >= 400 {
        return Err(format!("HTTP {}: {}", status, json.get("error").and_then(|v| v.as_str()).unwrap_or("unknown")));
    }

    Ok(json)
}


/// Fetch token usage for a task.
#[cfg(feature = "hydrate")]
pub async fn fetch_tokens_by_task(task_id: &str) -> Result<Vec<TokenUsageItem>, String> {
    let url = format!("{}/api/v1/tokens?task_id={}", api_base(), task_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json().await.map_err(|e| format!("json error: {e}"))
}

/// Fetch aggregated token usage per task for an epic.
#[cfg(feature = "hydrate")]
pub async fn fetch_tokens_by_epic(epic_id: &str) -> Result<Vec<TaskTokenSummary>, String> {
    let url = format!("{}/api/v1/tokens?epic_id={}", api_base(), epic_id);
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    resp.json().await.map_err(|e| format!("json error: {e}"))
}

/// Fetch config as a list of key-value pairs from /api/v1/config.
#[cfg(feature = "hydrate")]
pub async fn fetch_config() -> Result<Vec<(String, serde_json::Value)>, String> {
    let url = format!("{}/api/v1/config", api_base());
    let resp = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await.map_err(|e| format!("json error: {e}"))?;

    match data {
        serde_json::Value::Object(map) => {
            Ok(map.into_iter().collect())
        }
        _ => Ok(vec![]),
    }
}

/// Generic POST helper that sends JSON and returns Result<(), String>.
#[cfg(feature = "hydrate")]
pub async fn post_json(path: &str, body: serde_json::Value) -> Result<(), String> {
    let url = format!("{}{}", api_base(), path);
    let resp = gloo_net::http::Request::post(&url)
        .json(&body)
        .map_err(|e| format!("json error: {e}"))?
        .send()
        .await
        .map_err(|e| format!("fetch error: {e}"))?;

    if resp.ok() { Ok(()) } else { Err(format!("HTTP {}", resp.status())) }
}

#[cfg(not(feature = "hydrate"))]
pub async fn post_json(_path: &str, _body: serde_json::Value) -> Result<(), String> { Ok(()) }

/// Block a task via POST.
#[cfg(feature = "hydrate")]
pub async fn block_task(task_id: &str, reason: &str) -> Result<(), String> {
    post_json("/api/v1/tasks/block", serde_json::json!({"task_id": task_id, "reason": reason})).await
}

#[cfg(not(feature = "hydrate"))]
pub async fn block_task(_task_id: &str, _reason: &str) -> Result<(), String> { Ok(()) }

/// Skip a task via POST.
#[cfg(feature = "hydrate")]
pub async fn skip_task(task_id: &str, reason: &str) -> Result<(), String> {
    post_json("/api/v1/tasks/skip", serde_json::json!({"task_id": task_id, "reason": reason})).await
}

#[cfg(not(feature = "hydrate"))]
pub async fn skip_task(_task_id: &str, _reason: &str) -> Result<(), String> { Ok(()) }

/// Restart a task via POST.
#[cfg(feature = "hydrate")]
pub async fn restart_task(task_id: &str) -> Result<(), String> {
    post_json("/api/v1/tasks/restart", serde_json::json!({"task_id": task_id})).await
}

#[cfg(not(feature = "hydrate"))]
pub async fn restart_task(_task_id: &str) -> Result<(), String> { Ok(()) }

/// Create a new task via POST.
#[cfg(feature = "hydrate")]
pub async fn create_task(id: &str, epic_id: &str, title: &str, depends_on: Vec<String>, domain: &str) -> Result<(), String> {
    let mut body = serde_json::json!({
        "id": id,
        "epic_id": epic_id,
        "title": title,
    });
    if !depends_on.is_empty() {
        body["depends_on"] = serde_json::json!(depends_on);
    }
    if !domain.is_empty() {
        body["domain"] = serde_json::json!(domain);
    }
    post_json("/api/v1/tasks/create", body).await
}

#[cfg(not(feature = "hydrate"))]
pub async fn create_task(_id: &str, _epic_id: &str, _title: &str, _depends_on: Vec<String>, _domain: &str) -> Result<(), String> { Ok(()) }

#[cfg(not(feature = "hydrate"))]
pub async fn fetch_memory(_track: Option<&str>, _module: Option<&str>) -> Result<Vec<MemoryEntry>, String> { Ok(vec![]) }

// SSR stubs — these won't be called on the server but need to exist for compilation.
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_config() -> Result<Vec<(String, serde_json::Value)>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_stats() -> Result<StatsResponse, String> { Ok(StatsResponse::default()) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_epics() -> Result<Vec<EpicSummary>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_tasks(_epic_id: &str) -> Result<Vec<TaskItem>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_dag(_epic_id: &str) -> Result<DagResponse, String> {
    Ok(DagResponse { nodes: vec![], edges: vec![] })
}
#[cfg(not(feature = "hydrate"))]
pub async fn start_task(_task_id: &str) -> Result<(), String> { Ok(()) }
#[cfg(not(feature = "hydrate"))]
pub async fn done_task(_task_id: &str) -> Result<(), String> { Ok(()) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_tokens_by_task(_task_id: &str) -> Result<Vec<TokenUsageItem>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn fetch_tokens_by_epic(_epic_id: &str) -> Result<Vec<TaskTokenSummary>, String> { Ok(vec![]) }
#[cfg(not(feature = "hydrate"))]
pub async fn mutate_dag(_action: &str, _params: serde_json::Value, _version: &str) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({"success": true}))
}
