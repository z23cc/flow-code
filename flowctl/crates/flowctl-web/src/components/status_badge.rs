//! Status badge component for epic/task status display.

use leptos::prelude::*;

/// A colored badge showing epic or task status.
/// Uses the design system `.badge` + `.badge-{status}` classes from main.css.
#[component]
pub fn StatusBadge(#[prop(into)] status: String) -> impl IntoView {
    let badge_modifier = match status.as_str() {
        "open" | "todo" => "badge-todo",
        "in_progress" => "badge-in_progress",
        "done" => "badge-done",
        "blocked" => "badge-blocked",
        "failed" => "badge-failed",
        "closed" => "badge-closed",
        "skipped" => "badge-skipped",
        _ => "badge-todo",
    };

    view! {
        <span class={format!("badge {badge_modifier}")}>
            {status}
        </span>
    }
}
