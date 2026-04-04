//! Epic detail page: task list, progress, and DAG visualization.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

/// Epic detail page component.
#[component]
pub fn EpicDetailPage() -> impl IntoView {
    let params = use_params_map();
    let epic_id = move || params.read().get("id");

    view! {
        <div>
            <h1 class="text-2xl font-bold mb-6">
                {move || format!("Epic: {}", epic_id().unwrap_or_default())}
            </h1>
            <p class="text-gray-400">"Task list and DAG will appear here."</p>
        </div>
    }
}
