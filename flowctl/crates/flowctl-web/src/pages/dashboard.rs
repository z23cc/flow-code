//! Dashboard page: lists all epics with status statistics.

use leptos::prelude::*;

/// Dashboard page component.
#[component]
pub fn DashboardPage() -> impl IntoView {
    view! {
        <div>
            <h1 class="text-2xl font-bold mb-6">"Dashboard"</h1>
            <p class="text-gray-400">"Epic list will appear here."</p>
        </div>
    }
}
