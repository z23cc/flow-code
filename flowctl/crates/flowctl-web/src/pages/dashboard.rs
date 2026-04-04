//! Dashboard page: lists all epics with status and progress.

use leptos::prelude::*;

use crate::api;

/// Dashboard page component — shows all epics.
#[component]
pub fn DashboardPage() -> impl IntoView {
    let epics = LocalResource::new(move || async move {
        api::fetch_epics().await.unwrap_or_default()
    });

    view! {
        <div>
            <h1 class="text-2xl font-bold mb-6">"Dashboard"</h1>
            <Suspense fallback=move || view! { <p class="text-gray-400">"Loading epics..."</p> }>
                {move || {
                    epics.get().map(|epics_data| {
                        let epics_list: Vec<_> = epics_data.into_iter().collect();
                        if epics_list.is_empty() {
                            view! {
                                <p class="text-gray-400">"No epics found. Create one with flowctl epic create."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div class="grid gap-4">
                                    {epics_list.into_iter().map(|epic| {
                                        let progress = if epic.tasks > 0 {
                                            (epic.done as f64 / epic.tasks as f64 * 100.0) as u32
                                        } else { 0 };
                                        let status_class = match epic.status.as_str() {
                                            "done" | "closed" => "bg-green-600",
                                            _ if progress == 100 => "bg-blue-600",
                                            _ if progress > 0 => "bg-yellow-600",
                                            _ => "bg-gray-600",
                                        };
                                        let link = format!("/epic/{}", epic.id);
                                        let badge = format!("px-2 py-1 rounded text-xs font-medium text-white {status_class}");
                                        let width = format!("width: {}%", progress);
                                        let count = format!("{}/{}", epic.done, epic.tasks);
                                        view! {
                                            <a href={link}
                                               class="block bg-gray-800 rounded-lg p-4 hover:bg-gray-750 border border-gray-700 hover:border-gray-600 transition-colors">
                                                <div class="flex items-center justify-between mb-2">
                                                    <h2 class="text-lg font-semibold text-white">{epic.title.clone()}</h2>
                                                    <span class={badge}>{epic.status.clone()}</span>
                                                </div>
                                                <p class="text-sm text-gray-400 mb-2">{epic.id.clone()}</p>
                                                <div class="flex items-center gap-3">
                                                    <div class="flex-1 bg-gray-700 rounded-full h-2">
                                                        <div class="bg-cyan-500 h-2 rounded-full" style={width}></div>
                                                    </div>
                                                    <span class="text-sm text-gray-400">{count}</span>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
