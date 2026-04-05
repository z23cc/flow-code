//! Settings page: displays config key-value pairs and invariants.

use leptos::prelude::*;

use crate::api;

/// Settings page component — shows config and invariants.
#[component]
pub fn SettingsPage() -> impl IntoView {
    let config = LocalResource::new(move || async move {
        api::fetch_config().await.unwrap_or_default()
    });

    view! {
        <div class="fade-in">
            <h1>"Settings"</h1>

            // Section 1: Config
            <div class="card" style="margin-bottom: var(--space-6)">
                <h2 style="margin-bottom: var(--space-4)">"Configuration"</h2>
                <Suspense fallback=move || view! { <p style="color: var(--color-text-muted)">"Loading config..."</p> }>
                    {move || {
                        config.get().map(|entries| {
                            let items: Vec<_> = entries.into_iter().collect();
                            if items.is_empty() {
                                view! {
                                    <p style="color: var(--color-text-muted)">"No configuration found. Create .flow/config.json to get started."</p>
                                }.into_any()
                            } else {
                                view! {
                                    <table style="width: 100%; border-collapse: collapse;">
                                        <thead>
                                            <tr style="border-bottom: 1px solid var(--color-border)">
                                                <th style="text-align: left; padding: var(--space-2) var(--space-3); color: var(--color-text-muted); font-size: var(--text-sm)">"Key"</th>
                                                <th style="text-align: left; padding: var(--space-2) var(--space-3); color: var(--color-text-muted); font-size: var(--text-sm)">"Value"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {items.into_iter().map(|(key, value)| {
                                                let display = format_config_value(&value);
                                                view! {
                                                    <tr style="border-bottom: 1px solid var(--color-border)">
                                                        <td style="padding: var(--space-2) var(--space-3); font-family: monospace; font-size: var(--text-sm)">{key}</td>
                                                        <td style="padding: var(--space-2) var(--space-3); font-size: var(--text-sm)">{display}</td>
                                                    </tr>
                                                }
                                            }).collect::<Vec<_>>()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }
                        })
                    }}
                </Suspense>
            </div>

            // Section 2: Invariants
            <div class="card">
                <h2 style="margin-bottom: var(--space-4)">"Invariants"</h2>
                <Suspense fallback=move || view! { <p style="color: var(--color-text-muted)">"Loading..."</p> }>
                    {move || {
                        config.get().map(|entries| {
                            let invariants: Vec<_> = entries.iter()
                                .filter(|(k, _)| k.starts_with("invariant"))
                                .collect();
                            if invariants.is_empty() {
                                view! {
                                    <p style="color: var(--color-text-muted)">"No invariants registered. Use flowctl invariant add to create one."</p>
                                }.into_any()
                            } else {
                                view! {
                                    <div style="font-family: monospace; font-size: var(--text-sm); white-space: pre-wrap; color: var(--color-text)">
                                        {invariants.into_iter().map(|(key, value)| {
                                            let display = format_config_value(value);
                                            view! {
                                                <div style="margin-bottom: var(--space-2)">
                                                    <span style="color: var(--color-cyan)">{key.clone()}</span>
                                                    ": "
                                                    <span>{display}</span>
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
        </div>
    }
}

/// Format a serde_json::Value for display.
fn format_config_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => if *b { "true".to_string() } else { "false".to_string() },
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => "null".to_string(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    }
}
