//! Replay page: timeline of task execution events with token usage overlay.
//!
//! Client-only component: SSR renders a loading placeholder, hydration
//! activates the data fetch from the daemon API.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api;
use crate::components::token_chart::TokenChart;

/// Replay page component -- timeline view with token usage overlay.
#[component]
pub fn ReplayPage() -> impl IntoView {
    let params = use_params_map();
    let task_id = move || params.read().get("id").unwrap_or_default();

    // Derive the epic ID from the task ID (everything before the last dot).
    let epic_id = move || {
        let tid = task_id();
        tid.rsplit_once('.').map(|(e, _)| e.to_string()).unwrap_or(tid)
    };

    let events_data = LocalResource::new(move || {
        let eid = epic_id();
        let tid = task_id();
        async move {
            let events = api::fetch_tasks(&eid).await.ok();
            let tokens = api::fetch_tokens_by_task(&tid).await.ok();
            let epic_tokens = api::fetch_tokens_by_epic(&eid).await.ok();
            (events, tokens, epic_tokens)
        }
    });

    view! {
        <div>
            <div class="flex items-center gap-3 mb-6">
                <a href={move || format!("/epic/{}", epic_id())} class="text-gray-400 hover:text-white">"← Back to Epic"</a>
                <h1 class="text-2xl font-bold">"Replay: " {move || task_id()}</h1>
            </div>

            <Suspense fallback=move || view! { <p class="text-gray-400">"Loading replay data..."</p> }>
                {move || {
                    events_data.get().map(|data| {
                        let (_tasks, tokens, epic_tokens) = data;

                        // Token usage section for this task.
                        let token_section = if let Some(ref records) = tokens {
                            if records.is_empty() {
                                view! {
                                    <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 mb-6">
                                        <h2 class="text-lg font-semibold mb-3">"Token Usage"</h2>
                                        <p class="text-gray-400 text-sm">"No token records for this task."</p>
                                    </div>
                                }.into_any()
                            } else {
                                let total_input: i64 = records.iter().map(|r| r.input_tokens).sum();
                                let total_output: i64 = records.iter().map(|r| r.output_tokens).sum();
                                let total_cost: f64 = records.iter().filter_map(|r| r.estimated_cost).sum();

                                let rows: Vec<_> = records.iter().map(|rec| {
                                    let phase = rec.phase.clone().unwrap_or_else(|| "-".to_string());
                                    let model = rec.model.clone().unwrap_or_else(|| "-".to_string());
                                    let cost = rec.estimated_cost.map(|c| format!("${c:.4}")).unwrap_or_else(|| "-".to_string());
                                    view! {
                                        <tr class="border-b border-gray-700">
                                            <td class="py-2 px-3 text-xs text-gray-400">{rec.timestamp.clone()}</td>
                                            <td class="py-2 px-3 text-xs">{phase}</td>
                                            <td class="py-2 px-3 text-xs text-gray-300">{model}</td>
                                            <td class="py-2 px-3 text-xs text-right text-cyan-400">{rec.input_tokens.to_string()}</td>
                                            <td class="py-2 px-3 text-xs text-right text-violet-400">{rec.output_tokens.to_string()}</td>
                                            <td class="py-2 px-3 text-xs text-right text-gray-300">{cost}</td>
                                        </tr>
                                    }
                                }).collect();

                                view! {
                                    <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 mb-6">
                                        <h2 class="text-lg font-semibold mb-3">"Token Usage"</h2>
                                        <div class="flex gap-6 mb-4 text-sm">
                                            <div>
                                                <span class="text-gray-400">"Input: "</span>
                                                <span class="text-cyan-400 font-mono">{total_input.to_string()}</span>
                                            </div>
                                            <div>
                                                <span class="text-gray-400">"Output: "</span>
                                                <span class="text-violet-400 font-mono">{total_output.to_string()}</span>
                                            </div>
                                            <div>
                                                <span class="text-gray-400">"Cost: "</span>
                                                <span class="text-white font-mono">{format!("${total_cost:.4}")}</span>
                                            </div>
                                        </div>
                                        <div class="overflow-x-auto">
                                            <table class="w-full text-left">
                                                <thead>
                                                    <tr class="border-b border-gray-600 text-gray-400 text-xs">
                                                        <th class="py-2 px-3">"Timestamp"</th>
                                                        <th class="py-2 px-3">"Phase"</th>
                                                        <th class="py-2 px-3">"Model"</th>
                                                        <th class="py-2 px-3 text-right">"Input"</th>
                                                        <th class="py-2 px-3 text-right">"Output"</th>
                                                        <th class="py-2 px-3 text-right">"Cost"</th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {rows}
                                                </tbody>
                                            </table>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        } else {
                            view! {
                                <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 mb-6">
                                    <h2 class="text-lg font-semibold mb-3">"Token Usage"</h2>
                                    <p class="text-red-400 text-sm">"Failed to load token data."</p>
                                </div>
                            }.into_any()
                        };

                        // Epic-wide token chart.
                        let chart_section = if let Some(ref summaries) = epic_tokens {
                            view! {
                                <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 mb-6">
                                    <h2 class="text-lg font-semibold mb-3">"Epic Token Distribution"</h2>
                                    <TokenChart data={summaries.clone()}/>
                                </div>
                            }.into_any()
                        } else {
                            view! {
                                <div class="bg-gray-800 rounded-lg border border-gray-700 p-4 mb-6">
                                    <h2 class="text-lg font-semibold mb-3">"Epic Token Distribution"</h2>
                                    <p class="text-red-400 text-sm">"Failed to load epic token data."</p>
                                </div>
                            }.into_any()
                        };

                        view! {
                            <div>
                                {token_section}
                                {chart_section}
                            </div>
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
