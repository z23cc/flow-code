//! Epic detail page: task list with status badges.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api;

/// Epic detail page component — shows tasks for a specific epic.
#[component]
pub fn EpicDetailPage() -> impl IntoView {
    let params = use_params_map();
    let epic_id = move || params.read().get("id").unwrap_or_default();

    let tasks = LocalResource::new(move || {
        let id = epic_id();
        async move {
            api::fetch_tasks(&id).await.unwrap_or_default()
        }
    });

    view! {
        <div>
            <div class="flex items-center gap-3 mb-6">
                <a href="/" class="text-gray-400 hover:text-white">"← Back"</a>
                <h1 class="text-2xl font-bold">{move || epic_id()}</h1>
                <a href={move || format!("/dag/{}", epic_id())}
                   class="ml-auto px-3 py-1 bg-gray-700 hover:bg-gray-600 text-gray-300 text-sm rounded">
                    "DAG View"
                </a>
            </div>
            <Suspense fallback=move || view! { <p class="text-gray-400">"Loading tasks..."</p> }>
                {move || {
                    tasks.get().map(|tasks_data| {
                        let task_list: Vec<_> = tasks_data.into_iter().collect();
                        if task_list.is_empty() {
                            view! {
                                <p class="text-gray-400">"No tasks found for this epic."</p>
                            }.into_any()
                        } else {
                            let total = task_list.len();
                            let done_count = task_list.iter().filter(|t| t.status == "done").count();
                            let summary = format!("{done_count}/{total} tasks complete");
                            view! {
                                <div class="mb-4 text-sm text-gray-400">{summary}</div>
                                <div class="space-y-2">
                                    {task_list.into_iter().map(|task| {
                                        let (badge_class, badge_text) = match task.status.as_str() {
                                            "done" => ("bg-green-600", "done"),
                                            "in_progress" => ("bg-yellow-600", "in progress"),
                                            "blocked" => ("bg-red-600", "blocked"),
                                            "skipped" => ("bg-gray-600", "skipped"),
                                            _ => ("bg-gray-700", "todo"),
                                        };
                                        let badge_cls = format!("px-2 py-0.5 rounded text-xs font-medium text-white {badge_class}");
                                        let deps = if task.depends_on.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" → {}", task.depends_on.join(", "))
                                        };
                                        view! {
                                            <div class="flex items-center gap-3 bg-gray-800 rounded-lg p-3 border border-gray-700">
                                                <span class={badge_cls}>{badge_text}</span>
                                                <div class="flex-1">
                                                    <span class="text-white">{task.title.clone()}</span>
                                                    <span class="text-xs text-gray-500 ml-2">{task.id.clone()}</span>
                                                    <span class="text-xs text-gray-600 ml-2">{deps}</span>
                                                </div>
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
