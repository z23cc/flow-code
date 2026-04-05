//! DAG visualization page: renders task dependency graph using Canvas 2D.
//!
//! Client-only component: SSR renders a loading placeholder, hydration
//! activates the DAG fetch and WebSocket subscription for live updates.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api;
use crate::components::dag_graph::DagGraph;

/// DAG view page component — renders Canvas dependency graph.
#[component]
pub fn DagViewPage() -> impl IntoView {
    let params = use_params_map();
    let epic_id = move || params.read().get("id").unwrap_or_default();

    // Reactive DAG data — refetched on mutation.
    let (refresh_tick, set_refresh_tick) = signal(0u32);

    let dag_data = LocalResource::new(move || {
        let _ = refresh_tick.get(); // subscribe to refresh
        let id = epic_id();
        async move { api::fetch_dag(&id).await.ok() }
    });

    let on_mutated = Callback::new(move |()| {
        set_refresh_tick.update(|v| *v += 1);
    });

    // Set up WebSocket subscription for live updates (client-side only).
    #[cfg(feature = "hydrate")]
    {
        use leptos::prelude::Effect;
        Effect::new(move |_| {
            // WebSocket updates trigger a full refetch for simplicity with Canvas.
            spawn_ws_listener(set_refresh_tick);
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
                                    let eid = epic_id();
                                    view! {
                                        <DagGraph
                                            dag=dag
                                            version="".to_string()
                                            _epic_id=eid
                                            on_mutated=on_mutated
                                        />
                                    }.into_any()
                                }
                            }
                        })
                    }}
                </Suspense>
            </div>
            <div class="mt-4 flex gap-4 text-xs text-gray-500">
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #6b7280"></span>
                    "Todo"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #f59e0b"></span>
                    "In Progress"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #10b981"></span>
                    "Done"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #ef4444"></span>
                    "Blocked"
                </span>
                <span class="flex items-center gap-1">
                    <span class="inline-block w-3 h-3 rounded" style="background: #6b7280"></span>
                    "Skipped"
                </span>
            </div>
        </div>
    }
}

/// Spawn a WebSocket listener that triggers DAG refresh on status changes.
///
/// Connects to /api/v1/events and bumps the refresh tick on task status updates.
/// Only compiled for the hydrate (WASM) target.
#[cfg(feature = "hydrate")]
fn spawn_ws_listener(set_refresh: WriteSignal<u32>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

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

    let on_message = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if let Some(text) = event.data().as_string() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let evt = value.get("event").unwrap_or(&value);
                if evt.get("task_id").is_some() && evt.get("new_status").is_some() {
                    set_refresh.update(|v| *v += 1);
                }
            }
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
}
