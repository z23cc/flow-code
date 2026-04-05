//! Agent/Events log viewer page: real-time WebSocket event stream.
//!
//! Client-only component: SSR renders a "Connecting..." placeholder,
//! hydration establishes a WebSocket to /api/v1/events for live streaming.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::components::forms::Button;

/// Agent log viewer page — live event stream via WebSocket.
#[component]
pub fn AgentsPage() -> impl IntoView {
    let params = use_params_map();
    let task_id_filter = move || {
        let p = params.read();
        let id = p.get("id").unwrap_or_default();
        if id.is_empty() { None } else { Some(id) }
    };

    // Reactive signals for events and filter text.
    let (events, set_events) = signal(Vec::<EventLogEntry>::new());
    let (filter_text, set_filter_text) = signal(String::new());

    // Start WebSocket on hydrate.
    #[cfg(feature = "hydrate")]
    {
        use leptos::prelude::Effect;
        Effect::new(move |_| {
            spawn_event_listener(set_events);
        });
    }
    // Suppress unused warning in SSR mode.
    #[cfg(not(feature = "hydrate"))]
    let _ = set_events;

    // Filtered events based on route param + text filter.
    let filtered_events = move || {
        let all = events.get();
        let route_filter = task_id_filter();
        let text = filter_text.get().to_lowercase();

        all.into_iter()
            .filter(|e| {
                if let Some(ref tid) = route_filter {
                    if e.task_id.as_deref() != Some(tid.as_str()) {
                        return false;
                    }
                }
                if !text.is_empty() {
                    let matches_task = e.task_id.as_deref().unwrap_or("").to_lowercase().contains(&text);
                    let matches_type = e.event_type.to_lowercase().contains(&text);
                    if !matches_task && !matches_type {
                        return false;
                    }
                }
                true
            })
            .collect::<Vec<_>>()
    };

    let title = move || {
        match task_id_filter() {
            Some(tid) => format!("Agent Events: {}", tid),
            None => "Agent Events".to_string(),
        }
    };

    view! {
        <div class="fade-in">
            <div style="display: flex; align-items: center; gap: var(--space-3); margin-bottom: var(--space-4)">
                <a href="/" style="color: var(--color-text-muted)">"← Dashboard"</a>
                <h1>{move || title()}</h1>
            </div>

            <div style="margin-bottom: var(--space-4)">
                <input
                    type="text"
                    placeholder="Filter by task ID or event type..."
                    style="width: 100%; padding: var(--space-2) var(--space-3); background: var(--color-bg-card); border: 1px solid var(--color-border); border-radius: var(--radius); color: var(--color-text); font-family: var(--font-mono); font-size: var(--text-sm)"
                    on:input=move |ev| {
                        set_filter_text.set(event_target_value(&ev));
                    }
                />
            </div>

            {move || {
                let tid = task_id_filter();
                if let Some(tid) = tid {
                    let tid_restart = tid.clone();
                    let tid_skip = tid.clone();
                    let tid_block = tid.clone();
                    let set_events_restart = set_events;
                    let set_events_skip = set_events;
                    let set_events_block = set_events;
                    view! {
                        <div style="display: flex; gap: var(--space-2); margin-bottom: var(--space-3)">
                            <Button
                                label="Retry"
                                variant="warning"
                                on_click=move || {
                                    let id = tid_restart.clone();
                                    let set_evts = set_events_restart;
                                    leptos::task::spawn_local(async move {
                                        let result = crate::api::restart_task(&id).await;
                                        let msg = match result {
                                            Ok(()) => format!("Action performed: restart ({})", id),
                                            Err(e) => format!("Restart failed: {e}"),
                                        };
                                        set_evts.update(|evts| evts.push(EventLogEntry {
                                            timestamp: local_time_now(),
                                            event_type: "UserAction".to_string(),
                                            task_id: Some(id),
                                            message: msg,
                                        }));
                                    });
                                }
                            />
                            <Button
                                label="Skip"
                                on_click=move || {
                                    let id = tid_skip.clone();
                                    let set_evts = set_events_skip;
                                    leptos::task::spawn_local(async move {
                                        let result = crate::api::skip_task(&id, "Skipped from web UI").await;
                                        let msg = match result {
                                            Ok(()) => format!("Action performed: skip ({})", id),
                                            Err(e) => format!("Skip failed: {e}"),
                                        };
                                        set_evts.update(|evts| evts.push(EventLogEntry {
                                            timestamp: local_time_now(),
                                            event_type: "UserAction".to_string(),
                                            task_id: Some(id),
                                            message: msg,
                                        }));
                                    });
                                }
                            />
                            <Button
                                label="Block"
                                variant="danger"
                                on_click=move || {
                                    let id = tid_block.clone();
                                    let set_evts = set_events_block;
                                    leptos::task::spawn_local(async move {
                                        let result = crate::api::block_task(&id, "Blocked from web UI").await;
                                        let msg = match result {
                                            Ok(()) => format!("Action performed: block ({})", id),
                                            Err(e) => format!("Block failed: {e}"),
                                        };
                                        set_evts.update(|evts| evts.push(EventLogEntry {
                                            timestamp: local_time_now(),
                                            event_type: "UserAction".to_string(),
                                            task_id: Some(id),
                                            message: msg,
                                        }));
                                    });
                                }
                            />
                        </div>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }
            }}

            <div
                class="card"
                id="event-log"
                style="max-height: 70vh; overflow-y: auto; padding: var(--space-3); font-family: var(--font-mono); font-size: var(--text-sm); line-height: 1.6"
            >
                {move || {
                    let items = filtered_events();
                    if items.is_empty() {
                        view! {
                            <p style="color: var(--color-text-muted)">"Connecting to event stream..."</p>
                        }.into_any()
                    } else {
                        view! {
                            <div>
                                {items.into_iter().map(|e| {
                                    let color = event_color(&e.event_type);
                                    let task_label = e.task_id.clone().unwrap_or_default();
                                    let style = format!("color: {color}");
                                    view! {
                                        <div style="white-space: nowrap">
                                            <span style="color: var(--color-text-dim)">"["</span>
                                            <span style="color: var(--color-text-dim)">{e.timestamp.clone()}</span>
                                            <span style="color: var(--color-text-dim)">"] ["</span>
                                            <span style={style}>{e.event_type.clone()}</span>
                                            <span style="color: var(--color-text-dim)">"] ["</span>
                                            <span style="color: var(--color-text-muted)">{task_label}</span>
                                            <span style="color: var(--color-text-dim)">"] "</span>
                                            <span style="color: var(--color-text)">{e.message.clone()}</span>
                                        </div>
                                    }
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            <div style="margin-top: var(--space-3); display: flex; gap: var(--space-4); font-size: var(--text-xs); color: var(--color-text-dim)">
                <span style="display: flex; align-items: center; gap: var(--space-1)">
                    <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; background: #10b981"></span>
                    "Completed"
                </span>
                <span style="display: flex; align-items: center; gap: var(--space-1)">
                    <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; background: #ef4444"></span>
                    "Failed"
                </span>
                <span style="display: flex; align-items: center; gap: var(--space-1)">
                    <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; background: #f59e0b"></span>
                    "Started"
                </span>
                <span style="display: flex; align-items: center; gap: var(--space-1)">
                    <span style="display: inline-block; width: 10px; height: 10px; border-radius: 2px; background: #6b7280"></span>
                    "Other"
                </span>
            </div>
        </div>
    }
}

/// A parsed event log entry for display.
#[derive(Debug, Clone)]
struct EventLogEntry {
    timestamp: String,
    event_type: String,
    task_id: Option<String>,
    message: String,
}

/// Return the current local time as HH:MM:SS for log entries.
fn local_time_now() -> String {
    #[cfg(feature = "hydrate")]
    {
        let date = js_sys::Date::new_0();
        format!(
            "{:02}:{:02}:{:02}",
            date.get_hours(),
            date.get_minutes(),
            date.get_seconds()
        )
    }
    #[cfg(not(feature = "hydrate"))]
    {
        "??:??:??".to_string()
    }
}

/// Return a CSS color string for an event type.
fn event_color(event_type: &str) -> &'static str {
    match event_type {
        "TaskCompleted" | "EpicCompleted" | "GuardPassed" | "WaveCompleted" => "#10b981",
        "TaskFailed" | "GuardFailed" | "LockConflict" | "CircuitOpen" => "#ef4444",
        "TaskStarted" | "TaskReady" | "WaveStarted" => "#f59e0b",
        "TaskZombie" => "#f97316",
        "UserAction" => "#3b82f6",
        _ => "#6b7280",
    }
}

/// Build a human-readable message from event JSON.
fn format_event_message(value: &serde_json::Value) -> String {
    let event = value.get("event").unwrap_or(value);
    let etype = event.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");
    let data = event.get("data");

    match etype {
        "TaskReady" => "Dependencies satisfied, ready for dispatch".to_string(),
        "TaskStarted" => "Dispatched to worker".to_string(),
        "TaskCompleted" => "Completed successfully".to_string(),
        "TaskFailed" => {
            let err = data
                .and_then(|d| d.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error");
            format!("Failed: {err}")
        }
        "TaskZombie" => "Zombie detected (no heartbeat)".to_string(),
        "WaveStarted" => {
            let count = data
                .and_then(|d| d.get("task_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let wave = data
                .and_then(|d| d.get("wave"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Wave {wave} started ({count} tasks)")
        }
        "WaveCompleted" => {
            let wave = data
                .and_then(|d| d.get("wave"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Wave {wave} completed")
        }
        "EpicCompleted" => "All tasks done".to_string(),
        "GuardPassed" => "Guard check passed".to_string(),
        "GuardFailed" => {
            let err = data
                .and_then(|d| d.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Guard failed: {err}")
        }
        "LockConflict" => {
            let file = data
                .and_then(|d| d.get("file"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let held_by = data
                .and_then(|d| d.get("held_by"))
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("Lock conflict on {file} (held by {held_by})")
        }
        "CircuitOpen" => {
            let n = data
                .and_then(|d| d.get("consecutive_failures"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Circuit breaker opened ({n} consecutive failures)")
        }
        "DaemonStarted" => {
            let pid = data
                .and_then(|d| d.get("pid"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            format!("Daemon started (pid {pid})")
        }
        "DaemonShutdown" => "Daemon shutting down".to_string(),
        _ => format!("{etype}"),
    }
}

/// Extract event type string from the JSON.
fn extract_event_type(value: &serde_json::Value) -> String {
    let event = value.get("event").unwrap_or(value);
    event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Extract task_id from the event JSON (if present).
fn extract_task_id(value: &serde_json::Value) -> Option<String> {
    let event = value.get("event").unwrap_or(value);
    event
        .get("data")
        .and_then(|d| d.get("task_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Extract timestamp from the event JSON.
fn extract_timestamp(value: &serde_json::Value) -> String {
    value
        .get("timestamp")
        .and_then(|v| v.as_str())
        .map(|s| {
            // Trim to HH:MM:SS for display.
            if let Some(t_pos) = s.find('T') {
                let time_part = &s[t_pos + 1..];
                time_part.split('.').next().unwrap_or(time_part).to_string()
            } else {
                s.to_string()
            }
        })
        .unwrap_or_else(|| "??:??:??".to_string())
}

/// Spawn a WebSocket listener that appends events and auto-scrolls.
#[cfg(feature = "hydrate")]
fn spawn_event_listener(set_events: WriteSignal<Vec<EventLogEntry>>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let window = web_sys::window().expect("no window");
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let host = location
        .host()
        .unwrap_or_else(|_| "localhost:17319".to_string());
    let ws_url = format!("{ws_protocol}//{host}/api/v1/events");

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let on_message =
        Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            if let Some(text) = event.data().as_string() {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    let entry = EventLogEntry {
                        timestamp: extract_timestamp(&value),
                        event_type: extract_event_type(&value),
                        task_id: extract_task_id(&value),
                        message: format_event_message(&value),
                    };
                    set_events.update(|evts| evts.push(entry));

                    // Auto-scroll the log container.
                    if let Some(window) = web_sys::window() {
                        if let Some(doc) = window.document() {
                            if let Some(el) = doc.get_element_by_id("event-log") {
                                el.set_scroll_top(el.scroll_height());
                            }
                        }
                    }
                }
            }
        });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
}
