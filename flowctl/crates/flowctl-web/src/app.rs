//! Root application component with router.

use leptos::prelude::*;
use leptos_router::components::*;
use leptos_router::path;

use crate::pages::{dag_view::DagViewPage, dashboard::DashboardPage, epic_detail::EpicDetailPage, replay::ReplayPage};

/// Main application component with routing.
#[component]
pub fn App() -> impl IntoView {
    view! {
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
                    <Route path=path!("/dag/:id") view=DagViewPage/>
                    <Route path=path!("/replay/:id") view=ReplayPage/>
                </Routes>
            </main>
        </Router>
    }
}
