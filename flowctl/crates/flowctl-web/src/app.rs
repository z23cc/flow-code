//! Root application component with router.

use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::pages::{dashboard::DashboardPage, epic_detail::EpicDetailPage};

/// Shell component that wraps the entire app (provides <head> metadata).
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="dark">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone()/>
                <HydrationScripts options/>
                <MetaTags/>
                <link rel="stylesheet" href="/pkg/flowctl-web.css"/>
            </head>
            <body class="bg-gray-900 text-gray-100 min-h-screen">
                <App/>
            </body>
        </html>
    }
}

/// Main application component with routing.
#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="flowctl — AI Development Platform"/>
        <Router>
            <nav class="bg-gray-800 border-b border-gray-700 px-6 py-3">
                <div class="flex items-center justify-between max-w-7xl mx-auto">
                    <a href="/" class="text-xl font-bold text-cyan-400">"flowctl"</a>
                    <div class="flex gap-4 text-sm text-gray-400">
                        <a href="/" class="hover:text-white">"Dashboard"</a>
                    </div>
                </div>
            </nav>
            <main class="max-w-7xl mx-auto px-6 py-8">
                <Routes fallback=|| view! { <p class="text-red-400">"Page not found."</p> }>
                    <Route path=path!("/") view=DashboardPage/>
                    <Route path=path!("/epic/:id") view=EpicDetailPage/>
                </Routes>
            </main>
        </Router>
    }
}
