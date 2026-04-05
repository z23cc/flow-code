//! Toast notification component with WebSocket event bridge.
//!
//! Provides a global toast system that displays notifications from WebSocket events.
//! Toasts appear bottom-right, slide in, and auto-dismiss after 4 seconds.

use leptos::prelude::*;

/// Toast variant determines the color scheme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastVariant {
    Success,
    Warning,
    Error,
    Info,
}

impl ToastVariant {
    fn css_class(self) -> &'static str {
        match self {
            ToastVariant::Success => "toast-success",
            ToastVariant::Warning => "toast-warning",
            ToastVariant::Error => "toast-error",
            ToastVariant::Info => "toast-info",
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
struct Toast {
    id: u32,
    message: String,
    variant: ToastVariant,
    exiting: bool,
}

/// Maximum number of visible toasts.
const MAX_TOASTS: usize = 3;

/// Auto-dismiss delay in milliseconds.
const DISMISS_MS: i32 = 4000;

/// Exit animation duration in milliseconds.
const EXIT_ANIM_MS: i32 = 300;

/// Global toast provider that wraps the app and provides toast functionality.
#[component]
pub fn ToastProvider(children: Children) -> impl IntoView {
    let (toasts, set_toasts) = signal(Vec::<Toast>::new());
    let (next_id, set_next_id) = signal(0u32);

    // Provide the toast adder as context so any component can show toasts.
    let show_toast = Callback::new(move |(message, variant): (String, ToastVariant)| {
        let id = next_id.get_untracked();
        set_next_id.set(id + 1);

        set_toasts.update(|list| {
            // If at max capacity, remove oldest.
            while list.len() >= MAX_TOASTS {
                list.remove(0);
            }
            list.push(Toast {
                id,
                message,
                variant,
                exiting: false,
            });
        });

        // Schedule exit animation then removal.
        #[cfg(feature = "hydrate")]
        {
            let set_toasts = set_toasts;
            // Start exit animation after DISMISS_MS.
            let exit_cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                set_toasts.update(|list| {
                    if let Some(t) = list.iter_mut().find(|t| t.id == id) {
                        t.exiting = true;
                    }
                });

                // Remove after exit animation completes.
                let set_toasts = set_toasts;
                let remove_cb = wasm_bindgen::closure::Closure::<dyn Fn()>::new(move || {
                    set_toasts.update(|list| {
                        list.retain(|t| t.id != id);
                    });
                });
                if let Some(window) = web_sys::window() {
                    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                        remove_cb.as_ref().unchecked_ref(),
                        EXIT_ANIM_MS,
                    );
                }
                remove_cb.forget();
            });
            if let Some(window) = web_sys::window() {
                let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                    exit_cb.as_ref().unchecked_ref(),
                    DISMISS_MS,
                );
            }
            exit_cb.forget();
        }
    });

    provide_context(show_toast);

    view! {
        {children()}
        <div class="toast-container">
            {move || {
                toasts.get().into_iter().map(|toast| {
                    let class = format!(
                        "toast {} {}",
                        toast.variant.css_class(),
                        if toast.exiting { "toast-exit" } else { "toast-enter" }
                    );
                    view! {
                        <div class={class}>
                            <span class="toast-message">{toast.message}</span>
                        </div>
                    }
                }).collect::<Vec<_>>()
            }}
        </div>
    }
}

/// WebSocket event bridge that converts daemon events into toast notifications.
/// Place this component inside the `ToastProvider`.
#[component]
pub fn EventToastBridge() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        let show_toast = use_context::<Callback<(String, ToastVariant)>>()
            .expect("EventToastBridge must be inside ToastProvider");

        // Connect to the WebSocket events endpoint.
        let connect = move || {
            let Some(window) = web_sys::window() else { return };
            let location = window.location();
            let protocol = location.protocol().unwrap_or_default();
            let host = location.host().unwrap_or_default();
            let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
            let ws_url = format!("{ws_protocol}//{host}/api/v1/events");

            let Ok(ws) = web_sys::WebSocket::new(&ws_url) else { return };

            let show = show_toast;
            let onmessage = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::MessageEvent)>::new(
                move |ev: web_sys::MessageEvent| {
                    let Some(data) = ev.data().as_string() else { return };
                    let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else { return };

                    let event_type = json
                        .get("event_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let task_id = json
                        .get("task_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let epic_id = json
                        .get("epic_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");

                    match event_type {
                        "task_completed" | "task_done" => {
                            show.call((
                                format!("Task {} completed", task_id),
                                ToastVariant::Success,
                            ));
                        }
                        "task_failed" | "task_blocked" => {
                            show.call((
                                format!("Task {} failed", task_id),
                                ToastVariant::Error,
                            ));
                        }
                        "epic_completed" | "epic_done" | "epic_closed" => {
                            show.call((
                                format!("Epic {} completed!", epic_id),
                                ToastVariant::Success,
                            ));
                        }
                        "task_started" => {
                            show.call((
                                format!("Task {} started", task_id),
                                ToastVariant::Info,
                            ));
                        }
                        _ => {}
                    }
                },
            );

            ws.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
            onmessage.forget();

            // Keep the WebSocket alive by forgetting it (it lives for the page lifetime).
            std::mem::forget(ws);
        };

        connect();
    }

    view! {}
}
