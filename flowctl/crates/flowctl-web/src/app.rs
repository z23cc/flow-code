//! Root application component with router.

use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::hooks::use_location;
use leptos_router::path;

use crate::components::toast::{EventToastBridge, ToastProvider};
use crate::pages::{
    agents::AgentsPage, dag_view::DagViewPage, dashboard::DashboardPage,
    epic_detail::EpicDetailPage, memory::MemoryPage, replay::ReplayPage, settings::SettingsPage,
};

/// Breadcrumb component that derives crumbs from the current URL path.
#[component]
fn Breadcrumb() -> impl IntoView {
    let location = use_location();

    let crumbs = move || {
        let path = location.pathname.get();
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if segments.is_empty() {
            return vec![("Dashboard".to_string(), String::new())];
        }

        let mut result: Vec<(String, String)> = vec![("Dashboard".to_string(), "/".to_string())];
        let mut accumulated = String::new();

        for (i, seg) in segments.iter().enumerate() {
            accumulated.push('/');
            accumulated.push_str(seg);
            let label = match *seg {
                "epic" | "dag" | "replay" => continue,
                "settings" => "Settings".to_string(),
                "agents" => "Agents".to_string(),
                "memory" => "Memory".to_string(),
                id => {
                    if i > 0 {
                        match segments[i - 1] {
                            "epic" => format!("Epic: {}", id),
                            "dag" => format!("DAG: {}", id),
                            "replay" => format!("Replay: {}", id),
                            _ => id.to_string(),
                        }
                    } else {
                        id.to_string()
                    }
                }
            };
            let href = if i == segments.len() - 1 {
                String::new()
            } else {
                accumulated.clone()
            };
            result.push((label, href));
        }

        result
    };

    view! {
        <div class="breadcrumb">
            {move || {
                let items = crumbs();
                let total = items.len();
                items
                    .into_iter()
                    .enumerate()
                    .map(|(i, (label, href))| {
                        let show_sep = i > 0;
                        let is_link = !href.is_empty() && (i < total - 1 || total == 1);
                        let sep = if show_sep { " > " } else { "" };
                        view! {
                            <span>
                                {sep}
                                {if is_link {
                                    leptos::either::Either::Left(view! { <a href={href}>{label}</a> })
                                } else {
                                    leptos::either::Either::Right(view! { <span>{label}</span> })
                                }}
                            </span>
                        }
                    })
                    .collect::<Vec<_>>()
            }}
        </div>
    }
}

/// Install keyboard shortcuts (hydrate/client-side only).
#[component]
fn KeyboardShortcuts() -> impl IntoView {
    #[cfg(feature = "hydrate")]
    {
        use leptos_router::hooks::use_navigate;

        let navigate = use_navigate();
        let handler = wasm_bindgen::closure::Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(
            move |ev: web_sys::KeyboardEvent| {
                // Don't intercept when typing in inputs/textareas
                if let Some(target) = ev.target() {
                    if let Ok(el) = target.dyn_into::<web_sys::HtmlInputElement>() {
                        let tag = el.tag_name().to_ascii_lowercase();
                        if tag == "input" || tag == "textarea" || tag == "select" {
                            return;
                        }
                    }
                }
                if ev.ctrl_key() || ev.alt_key() || ev.meta_key() {
                    return;
                }
                let key = ev.key();
                let path = match key.as_str() {
                    "d" | "D" => Some("/"),
                    "g" | "G" => Some("/dag/current"),
                    "s" | "S" => Some("/settings"),
                    "a" | "A" => Some("/agents"),
                    "m" | "M" => Some("/memory"),
                    _ => None,
                };
                if let Some(p) = path {
                    ev.prevent_default();
                    navigate(p, Default::default());
                }
            },
        );

        if let Some(window) = web_sys::window() {
            let _ = window.add_event_listener_with_callback(
                "keydown",
                handler.as_ref().unchecked_ref(),
            );
        }
        handler.forget();
    }

    view! {}
}

/// Main application component with routing.
#[component]
pub fn App() -> impl IntoView {
    view! {
        <ToastProvider>
        <Router>
            <EventToastBridge/>
            <nav class="nav">
                <div class="nav-inner">
                    <a href="/" class="nav-brand">"flowctl"</a>
                    <input type="checkbox" id="nav-toggle" class="nav-toggle"/>
                    <label for="nav-toggle" class="nav-hamburger">
                        <span></span>
                        <span></span>
                        <span></span>
                    </label>
                    <div class="nav-links">
                        <a href="/">
                            <span class="nav-link-text">"Dashboard"</span>
                            <span class="nav-shortcut">"D"</span>
                        </a>
                        <a href="/agents">
                            <span class="nav-link-text">"Agents"</span>
                            <span class="nav-shortcut">"A"</span>
                        </a>
                        <a href="/memory">
                            <span class="nav-link-text">"Memory"</span>
                            <span class="nav-shortcut">"M"</span>
                        </a>
                        <a href="/settings">
                            <span class="nav-link-text">"Settings"</span>
                            <span class="nav-shortcut">"S"</span>
                        </a>
                    </div>
                </div>
            </nav>
            <Breadcrumb/>
            <KeyboardShortcuts/>
            <main class="container">
                <Routes fallback=|| view! { <p class="text-red-400">"Page not found."</p> }>
                    <Route path=path!("/") view=DashboardPage/>
                    <Route path=path!("/epic/:id") view=EpicDetailPage/>
                    <Route path=path!("/dag/:id") view=DagViewPage/>
                    <Route path=path!("/replay/:id") view=ReplayPage/>
                    <Route path=path!("/agents") view=AgentsPage/>
                    <Route path=path!("/agents/:id") view=AgentsPage/>
                    <Route path=path!("/memory") view=MemoryPage/>
                    <Route path=path!("/settings") view=SettingsPage/>
                </Routes>
            </main>
        </Router>
        </ToastProvider>
    }
}
