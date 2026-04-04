//! DAG visualization page: renders task dependency graph as SVG.
//!
//! Client-only component: SSR renders a loading placeholder, hydration
//! activates the DAG fetch and WebSocket subscription for live updates.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api;

/// Color for a task status.
fn status_color(status: &str) -> &'static str {
    match status {
        "done" => "#16a34a",       // green-600
        "in_progress" => "#ca8a04", // yellow-600
        "blocked" => "#dc2626",    // red-600
        "skipped" => "#4b5563",    // gray-600
        _ => "#374151",            // gray-700 (todo)
    }
}

/// Text color for contrast on status backgrounds.
fn status_text_color(status: &str) -> &'static str {
    match status {
        "done" | "in_progress" | "blocked" => "#ffffff",
        _ => "#d1d5db", // gray-300
    }
}

/// Node dimensions for SVG rendering.
const NODE_WIDTH: f64 = 160.0;
const NODE_HEIGHT: f64 = 60.0;
const NODE_RX: f64 = 8.0;

/// DAG view page component — renders SVG dependency graph.
#[component]
pub fn DagViewPage() -> impl IntoView {
    let params = use_params_map();
    let epic_id = move || params.read().get("id").unwrap_or_default();

    let dag_data = LocalResource::new(move || {
        let id = epic_id();
        async move { api::fetch_dag(&id).await.ok() }
    });

    // Reactive signal for node statuses (updated via WebSocket).
    let (status_updates, _set_status_updates) =
        signal(std::collections::HashMap::<String, String>::new());

    // Set up WebSocket subscription for live updates (client-side only).
    #[cfg(feature = "hydrate")]
    {
        let set_status_updates = _set_status_updates;
        use leptos::prelude::Effect;
        Effect::new(move |_| {
            spawn_ws_listener(set_status_updates);
        });
    }

    view! {
        <div>
            <div class="flex items-center gap-3 mb-6">
                <a href={move || format!("/epic/{}", epic_id())} class="text-gray-400 hover:text-white">"← Back to Epic"</a>
                <h1 class="text-2xl font-bold">"DAG View: " {move || epic_id()}</h1>
            </div>
            <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 overflow-auto">
                <Suspense fallback=move || view! { <p class="text-gray-400">"Loading DAG..."</p> }>
                    {move || {
                        dag_data.get().map(|maybe_dag| {
                            match maybe_dag {
                                None => view! {
                                    <p class="text-red-400">"Failed to load DAG data."</p>
                                }.into_any(),
                                Some(dag) if dag.nodes.is_empty() => view! {
                                    <p class="text-gray-400">"No tasks in this epic."</p>
                                }.into_any(),
                                Some(dag) => {
                                    let updates = status_updates.get();
                                    // Compute SVG viewport bounds.
                                    let min_x = dag.nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
                                    let max_x = dag.nodes.iter().map(|n| n.x).fold(f64::NEG_INFINITY, f64::max);
                                    let min_y = dag.nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
                                    let max_y = dag.nodes.iter().map(|n| n.y).fold(f64::NEG_INFINITY, f64::max);
                                    let padding = 40.0;
                                    let vb_x = min_x - padding;
                                    let vb_y = min_y - padding - NODE_HEIGHT / 2.0;
                                    let vb_w = (max_x - min_x) + NODE_WIDTH + padding * 2.0;
                                    let vb_h = (max_y - min_y) + NODE_HEIGHT + padding * 2.0;
                                    let viewbox = format!("{vb_x} {vb_y} {vb_w} {vb_h}");

                                    // Build node position lookup for edge drawing.
                                    let node_positions: std::collections::HashMap<String, (f64, f64)> =
                                        dag.nodes.iter().map(|n| (n.id.clone(), (n.x, n.y))).collect();

                                    let edges_view: Vec<_> = dag.edges.iter().map(|edge| {
                                        let from_pos = node_positions.get(&edge.from).copied().unwrap_or((0.0, 0.0));
                                        let to_pos = node_positions.get(&edge.to).copied().unwrap_or((0.0, 0.0));
                                        // Edge: from right side of source to left side of target.
                                        let x1 = from_pos.0 + NODE_WIDTH / 2.0;
                                        let y1 = from_pos.1;
                                        let x2 = to_pos.0 - NODE_WIDTH / 2.0;
                                        let y2 = to_pos.1;
                                        // Cubic bezier for smooth curves.
                                        let mid_x = (x1 + x2) / 2.0;
                                        let d = format!("M {x1} {y1} C {mid_x} {y1}, {mid_x} {y2}, {x2} {y2}");
                                        view! {
                                            <path d={d} fill="none" stroke="#6b7280" stroke-width="2"
                                                  marker-end="url(#arrowhead)"/>
                                        }
                                    }).collect();

                                    let nodes_view: Vec<_> = dag.nodes.iter().map(|node| {
                                        // Use WebSocket-updated status if available, else original.
                                        let status = updates.get(&node.id)
                                            .cloned()
                                            .unwrap_or_else(|| node.status.clone());
                                        let fill = status_color(&status);
                                        let text_fill = status_text_color(&status);
                                        let rx = node.x - NODE_WIDTH / 2.0;
                                        let ry = node.y - NODE_HEIGHT / 2.0;
                                        // Truncate long titles.
                                        let title = if node.title.len() > 20 {
                                            format!("{}...", &node.title[..17])
                                        } else {
                                            node.title.clone()
                                        };
                                        let short_id = node.id.clone();
                                        view! {
                                            <g>
                                                <rect x={rx.to_string()} y={ry.to_string()}
                                                      width={NODE_WIDTH.to_string()} height={NODE_HEIGHT.to_string()}
                                                      rx={NODE_RX.to_string()} fill={fill.to_string()}
                                                      stroke="#4b5563" stroke-width="1"/>
                                                <text x={node.x.to_string()} y={(node.y - 6.0).to_string()}
                                                      text-anchor="middle" fill={text_fill.to_string()}
                                                      font-size="12" font-weight="bold">{title}</text>
                                                <text x={node.x.to_string()} y={(node.y + 14.0).to_string()}
                                                      text-anchor="middle" fill={text_fill.to_string()}
                                                      font-size="10" opacity="0.7">{short_id}</text>
                                            </g>
                                        }
                                    }).collect();

                                    view! {
                                        <svg viewBox={viewbox} class="w-full" style="min-height: 400px;">
                                            <defs>
                                                <marker id="arrowhead" markerWidth="10" markerHeight="7"
                                                        refX="10" refY="3.5" orient="auto">
                                                    <polygon points="0 0, 10 3.5, 0 7" fill="#6b7280"/>
                                                </marker>
                                            </defs>
                                            {edges_view}
                                            {nodes_view}
                                        </svg>
                                    }.into_any()
                                }
                            }
                        })
                    }}
                </Suspense>
            </div>
            <div class="mt-4 flex gap-4 text-xs text-gray-500">
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #374151"></span>
                    "Todo"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #ca8a04"></span>
                    "In Progress"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #16a34a"></span>
                    "Done"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #dc2626"></span>
                    "Blocked"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #4b5563"></span>
                    "Skipped"
                </span>
            </div>
        </div>
    }
}

/// Spawn a WebSocket listener that updates node statuses in real-time.
///
/// Connects to /api/v1/events and parses task status change events.
/// Only compiled for the hydrate (WASM) target.
#[cfg(feature = "hydrate")]
fn spawn_ws_listener(
    set_status: WriteSignal<std::collections::HashMap<String, String>>,
) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    // Build WebSocket URL from current page location.
    let window = web_sys::window().expect("no window");
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let host = location.host().unwrap_or_else(|_| "localhost:17319".to_string());
    let ws_url = format!("{ws_protocol}//{host}/api/v1/events");

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    // On message: parse event and update status map if it's a task status change.
    let on_message = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if let Some(text) = event.data().as_string() {
            // Events are JSON with {event: {type, task_id, new_status, ...}, timestamp}
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let evt = value.get("event").unwrap_or(&value);
                if let (Some(task_id), Some(new_status)) = (
                    evt.get("task_id").and_then(|v| v.as_str()),
                    evt.get("new_status").and_then(|v| v.as_str()),
                ) {
                    set_status.update(|map| {
                        map.insert(task_id.to_string(), new_status.to_lowercase());
                    });
                }
            }
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget(); // leak closure to keep WebSocket alive
}
