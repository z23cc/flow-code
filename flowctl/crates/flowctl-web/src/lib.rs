//! flowctl-web: Leptos web frontend for the flowctl platform.
//!
//! Provides a real-time dashboard for managing epics, tasks, and DAG visualization.
//! Compiles to WASM for client-side hydration and supports SSR via axum.

pub mod api;
pub mod app;
pub mod pages;
pub mod components;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(app::App);
}
