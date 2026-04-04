//! Status badge component for epic/task status display.

use leptos::prelude::*;

/// A colored badge showing epic or task status.
#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let class = match status.as_str() {
        "open" | "todo" => "bg-blue-900 text-blue-300",
        "in_progress" => "bg-yellow-900 text-yellow-300",
        "done" => "bg-green-900 text-green-300",
        "blocked" => "bg-red-900 text-red-300",
        "closed" => "bg-gray-700 text-gray-400",
        _ => "bg-gray-700 text-gray-400",
    };

    view! {
        <span class={format!("px-2 py-0.5 rounded text-xs font-medium {class}")}>
            {status}
        </span>
    }
}
