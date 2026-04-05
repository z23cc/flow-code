//! Memory browser page: displays memory entries with filtering.

use leptos::prelude::*;

use crate::api;

/// Memory browser page component.
#[component]
pub fn MemoryPage() -> impl IntoView {
    let (track_filter, set_track_filter) = signal("all".to_string());
    let (search_text, set_search_text) = signal(String::new());

    let memory = LocalResource::new(move || {
        let track = track_filter.get();
        async move {
            let track_param = if track == "all" { None } else { Some(track.as_str()) };
            api::fetch_memory(track_param, None).await.unwrap_or_default()
        }
    });

    view! {
        <div class="fade-in">
            <h1>"Memory"</h1>

            // Filter bar
            <div class="card" style="margin-bottom: var(--space-6); display: flex; gap: var(--space-4); align-items: center; flex-wrap: wrap">
                <label style="color: var(--color-text-muted); font-size: var(--text-sm)">
                    "Track: "
                    <select
                        style="background: var(--color-bg); color: var(--color-text); border: 1px solid var(--color-border); border-radius: 4px; padding: var(--space-1) var(--space-2); font-size: var(--text-sm)"
                        on:change=move |ev| {
                            use leptos::prelude::*;
                            let val = event_target_value(&ev);
                            set_track_filter.set(val);
                        }
                    >
                        <option value="all">"All"</option>
                        <option value="bug">"Bug"</option>
                        <option value="knowledge">"Knowledge"</option>
                    </select>
                </label>
                <input
                    type="text"
                    placeholder="Search content..."
                    style="background: var(--color-bg); color: var(--color-text); border: 1px solid var(--color-border); border-radius: 4px; padding: var(--space-1) var(--space-2); font-size: var(--text-sm); flex: 1; min-width: 200px"
                    on:input=move |ev| {
                        use leptos::prelude::*;
                        set_search_text.set(event_target_value(&ev));
                    }
                />
            </div>

            // Memory entries
            <Suspense fallback=move || view! { <p style="color: var(--color-text-muted)">"Loading memory entries..."</p> }>
                {move || {
                    memory.get().map(|entries| {
                        let search = search_text.get().to_lowercase();
                        let filtered: Vec<_> = entries.into_iter()
                            .filter(|e| {
                                if search.is_empty() {
                                    true
                                } else {
                                    e.content.to_lowercase().contains(&search)
                                        || e.module.as_deref().unwrap_or("").to_lowercase().contains(&search)
                                }
                            })
                            .collect();

                        if filtered.is_empty() {
                            view! {
                                <p style="color: var(--color-text-muted)">"No memory entries found."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div style="display: flex; flex-direction: column; gap: var(--space-4)">
                                    {filtered.into_iter().map(|entry| {
                                        let type_badge = type_badge_style(&entry.entry_type);
                                        let track_badge = track_badge_style(entry.track.as_deref().unwrap_or(""));
                                        let module_text = entry.module.clone().unwrap_or_default();
                                        let severity_text = entry.severity.as_ref().map(|s| format!("Severity: {s}")).unwrap_or_default();

                                        view! {
                                            <div class="card" style="padding: var(--space-4)">
                                                <div style="display: flex; gap: var(--space-2); align-items: center; margin-bottom: var(--space-2); flex-wrap: wrap">
                                                    <span class="badge" style=type_badge.0>{type_badge.1}</span>
                                                    {(!track_badge.1.is_empty()).then(|| view! {
                                                        <span class="badge" style=track_badge.0>{track_badge.1}</span>
                                                    })}
                                                    {(!module_text.is_empty()).then(|| view! {
                                                        <span class="badge" style="background: var(--color-bg); border: 1px solid var(--color-border); color: var(--color-text-muted); padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)">{module_text}</span>
                                                    })}
                                                    {(!severity_text.is_empty()).then(|| view! {
                                                        <span style="color: var(--color-text-muted); font-size: var(--text-xs); margin-left: auto">{severity_text}</span>
                                                    })}
                                                </div>
                                                <p style="color: var(--color-text); font-size: var(--text-sm); white-space: pre-wrap; margin: 0">{entry.content}</p>
                                            </div>
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

/// Returns (style, label) for a type badge.
fn type_badge_style(entry_type: &str) -> (String, String) {
    match entry_type {
        "pitfall" => (
            "background: #7f1d1d; color: #fca5a5; padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            "pitfall".to_string(),
        ),
        "convention" => (
            "background: #1e3a5f; color: #93c5fd; padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            "convention".to_string(),
        ),
        "decision" => (
            "background: #3b0764; color: #d8b4fe; padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            "decision".to_string(),
        ),
        other => (
            "background: var(--color-bg); border: 1px solid var(--color-border); color: var(--color-text-muted); padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            other.to_string(),
        ),
    }
}

/// Returns (style, label) for a track badge.
fn track_badge_style(track: &str) -> (String, String) {
    match track {
        "bug" => (
            "background: #7c2d12; color: #fdba74; padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            "bug".to_string(),
        ),
        "knowledge" => (
            "background: #14532d; color: #86efac; padding: 2px 8px; border-radius: 4px; font-size: var(--text-xs)".to_string(),
            "knowledge".to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}
