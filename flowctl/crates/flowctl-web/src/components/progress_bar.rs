//! Progress bar component.

use leptos::prelude::*;

/// A horizontal progress bar.
#[component]
pub fn ProgressBar(
    #[prop(into)] done: usize,
    #[prop(into)] total: usize,
) -> impl IntoView {
    let pct = if total > 0 { (done * 100) / total } else { 0 };

    view! {
        <div class="w-full bg-gray-700 rounded-full h-2">
            <div
                class="bg-cyan-500 h-2 rounded-full transition-all"
                style={format!("width: {}%", pct)}
            />
        </div>
        <span class="text-xs text-gray-400 mt-1">
            {format!("{done}/{total}")}
        </span>
    }
}
