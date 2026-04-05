//! Reusable form components: TextInput, Select, Button.

use leptos::prelude::*;

/// A labeled text input.
#[component]
pub fn TextInput(
    #[prop(into)] label: String,
    #[prop(into)] placeholder: String,
    value: RwSignal<String>,
) -> impl IntoView {
    view! {
        <div style="display: flex; flex-direction: column; gap: var(--space-1)">
            <label style="font-size: var(--text-xs); color: var(--color-text-muted)">{label}</label>
            <input
                type="text"
                placeholder=placeholder
                class="form-input"
                style="padding: var(--space-2); border: 1px solid var(--color-border); border-radius: var(--radius); background: var(--color-bg-card); color: var(--color-text); font-size: var(--text-sm)"
                prop:value=move || value.get()
                on:input=move |ev| {
                    value.set(event_target_value(&ev));
                }
            />
        </div>
    }
}

/// A labeled select dropdown.
#[component]
pub fn Select(
    #[prop(into)] label: String,
    #[prop(into)] options: Vec<(String, String)>,
    value: RwSignal<String>,
) -> impl IntoView {
    view! {
        <div style="display: flex; flex-direction: column; gap: var(--space-1)">
            <label style="font-size: var(--text-xs); color: var(--color-text-muted)">{label}</label>
            <select
                class="form-select"
                style="padding: var(--space-2); border: 1px solid var(--color-border); border-radius: var(--radius); background: var(--color-bg-card); color: var(--color-text); font-size: var(--text-sm)"
                prop:value=move || value.get()
                on:change=move |ev| {
                    value.set(event_target_value(&ev));
                }
            >
                {options.into_iter().map(|(val, display)| {
                    view! { <option value={val}>{display}</option> }
                }).collect::<Vec<_>>()}
            </select>
        </div>
    }
}

/// A styled button with variant support.
#[component]
pub fn Button(
    #[prop(into)] label: String,
    #[prop(into, optional)] variant: Option<String>,
    #[prop(into, optional)] disabled: Option<Signal<bool>>,
    on_click: impl Fn() + Send + Sync + 'static,
) -> impl IntoView {
    let variant = variant.unwrap_or_default();
    let class = match variant.as_str() {
        "success" => "btn btn-success",
        "danger" => "btn btn-danger",
        "warning" => "btn btn-warning",
        "primary" => "btn btn-primary",
        _ => "btn",
    };
    let is_disabled = disabled.unwrap_or(Signal::derive(|| false));

    view! {
        <button
            class=class
            style="padding: var(--space-1) var(--space-3); font-size: var(--text-xs); cursor: pointer"
            disabled=move || is_disabled.get()
            on:click=move |_| on_click()
        >
            {label}
        </button>
    }
}
