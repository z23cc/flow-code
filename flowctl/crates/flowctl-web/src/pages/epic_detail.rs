//! Epic detail page: task list with status badges, action buttons, and create form.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

use crate::api;
use crate::components::forms::{TextInput, Select, Button};

/// Epic detail page component — shows tasks for a specific epic.
#[component]
pub fn EpicDetailPage() -> impl IntoView {
    let params = use_params_map();
    let epic_id = move || params.read().get("id").unwrap_or_default();

    // Refresh trigger — increment to re-fetch tasks.
    let (refresh, set_refresh) = signal(0u32);

    let tasks = LocalResource::new(move || {
        let _v = refresh.get(); // subscribe to refresh trigger
        let id = epic_id();
        async move {
            api::fetch_tasks(&id).await.unwrap_or_default()
        }
    });

    let do_refresh = move || set_refresh.update(|n| *n += 1);

    view! {
        <div class="fade-in">
            <div class="flex-between" style="margin-bottom: var(--space-6)">
                <div style="display: flex; align-items: center; gap: var(--space-3)">
                    <a href="/" style="color: var(--color-text-muted); text-decoration: none">"← Back"</a>
                    <h1 style="margin-bottom: 0">{move || epic_id()}</h1>
                </div>
                <a href={move || format!("/dag/{}", epic_id())} class="btn btn-primary">
                    "DAG View"
                </a>
            </div>
            <CreateTaskForm epic_id=Signal::derive(epic_id) on_created=do_refresh />
            <Suspense fallback=move || view! { <p style="color: var(--color-text-muted)">"Loading tasks..."</p> }>
                {move || {
                    let do_refresh = do_refresh.clone();
                    tasks.get().map(move |tasks_data| {
                        let task_list: Vec<_> = tasks_data.into_iter().collect();
                        if task_list.is_empty() {
                            view! {
                                <p style="color: var(--color-text-muted)">"No tasks found for this epic."</p>
                            }.into_any()
                        } else {
                            let total = task_list.len();
                            let done_count = task_list.iter().filter(|t| t.status == "done").count();
                            let in_progress_count = task_list.iter().filter(|t| t.status == "in_progress").count();
                            let blocked_count = task_list.iter().filter(|t| t.status == "blocked").count();
                            let summary = format!("{done_count}/{total} tasks complete");
                            view! {
                                <div class="stats-row">
                                    <div class="stat-card">
                                        <div class="stat-value">{total.to_string()}</div>
                                        <div class="stat-label">"Total"</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-value" style="color: var(--color-success)">{done_count.to_string()}</div>
                                        <div class="stat-label">"Done"</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-value" style="color: var(--color-warning)">{in_progress_count.to_string()}</div>
                                        <div class="stat-label">"In Progress"</div>
                                    </div>
                                    <div class="stat-card">
                                        <div class="stat-value" style="color: var(--color-error)">{blocked_count.to_string()}</div>
                                        <div class="stat-label">"Blocked"</div>
                                    </div>
                                </div>
                                <div style="margin-bottom: var(--space-4); font-size: var(--text-sm); color: var(--color-text-muted)">{summary}</div>
                                <div style="display: flex; flex-direction: column; gap: var(--space-2)">
                                    {task_list.into_iter().map(|task| {
                                        let do_refresh = do_refresh.clone();
                                        view! { <TaskRow task=task on_action=do_refresh /> }
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

/// Collapsible form for creating a new task in the epic.
#[component]
fn CreateTaskForm(
    epic_id: Signal<String>,
    on_created: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let (open, set_open) = signal(false);
    let title = RwSignal::new(String::new());
    let deps = RwSignal::new(String::new());
    let domain = RwSignal::new("general".to_string());
    let (submitting, set_submitting) = signal(false);
    let (error, set_error) = signal(Option::<String>::None);

    let on_submit = move || {
        let t = title.get();
        if t.is_empty() { return; }
        let eid = epic_id.get();
        let dep_list: Vec<String> = deps.get()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let d = domain.get();
        // Generate a task ID: epic_id.N where N is a timestamp suffix for uniqueness
        let task_num = {
            #[cfg(feature = "hydrate")]
            { js_sys::Date::now() as u64 % 10000 }
            #[cfg(not(feature = "hydrate"))]
            { 0u64 }
        };
        let task_id = format!("{}.{}", eid, task_num);
        set_submitting.set(true);
        set_error.set(None);
        leptos::task::spawn_local(async move {
            match api::create_task(&task_id, &eid, &t, dep_list, &d).await {
                Ok(()) => {
                    title.set(String::new());
                    deps.set(String::new());
                    domain.set("general".to_string());
                    set_open.set(false);
                    on_created();
                }
                Err(e) => set_error.set(Some(e)),
            }
            set_submitting.set(false);
        });
    };

    let domain_options = vec![
        ("general".to_string(), "General".to_string()),
        ("frontend".to_string(), "Frontend".to_string()),
        ("backend".to_string(), "Backend".to_string()),
        ("testing".to_string(), "Testing".to_string()),
        ("docs".to_string(), "Docs".to_string()),
        ("ops".to_string(), "Ops".to_string()),
    ];

    view! {
        <div style="margin-bottom: var(--space-4)">
            <button
                class="btn"
                style="font-size: var(--text-sm)"
                on:click=move |_| set_open.update(|o| *o = !*o)
            >
                {move || if open.get() { "- Hide Create Task" } else { "+ Create Task" }}
            </button>
            <Show when=move || open.get()>
                <div class="card" style="margin-top: var(--space-2); padding: var(--space-4); display: flex; flex-direction: column; gap: var(--space-3)">
                    <TextInput label="Title" placeholder="Task title" value=title />
                    <TextInput label="Dependencies" placeholder="Comma-separated task IDs" value=deps />
                    <Select label="Domain" options=domain_options.clone() value=domain />
                    {move || error.get().map(|e| view! {
                        <div style="color: var(--color-error); font-size: var(--text-xs)">{e}</div>
                    })}
                    <div>
                        <Button
                            label="Create"
                            variant="primary"
                            disabled=Signal::derive(move || submitting.get())
                            on_click=on_submit.clone()
                        />
                    </div>
                </div>
            </Show>
        </div>
    }
}

/// A single task row with status badge and contextual action buttons.
#[component]
fn TaskRow(
    task: api::TaskItem,
    on_action: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let badge_class = match task.status.as_str() {
        "done" => "badge badge-done",
        "in_progress" => "badge badge-in_progress",
        "blocked" => "badge badge-blocked",
        "skipped" => "badge badge-skipped",
        _ => "badge badge-todo",
    };
    let badge_text = match task.status.as_str() {
        "in_progress" => "in progress".to_string(),
        other => other.to_string(),
    };
    let deps = if task.depends_on.is_empty() {
        String::new()
    } else {
        format!(" → {}", task.depends_on.join(", "))
    };

    let task_id = task.id.clone();
    let status = task.status.clone();

    view! {
        <div class="task-row card" style="padding: var(--space-3); display: flex; align-items: center; gap: var(--space-2)">
            <span class={badge_class}>{badge_text}</span>
            <div class="task-title" style="flex: 1">
                <span>{task.title.clone()}</span>
                <span style="font-size: var(--text-xs); color: var(--color-text-dim); margin-left: var(--space-2)">{task.id.clone()}</span>
                <span style="font-size: var(--text-xs); color: var(--color-text-dim); margin-left: var(--space-2)">{deps}</span>
            </div>
            <TaskActions task_id=task_id status=status on_action=on_action />
        </div>
    }
}

/// Contextual action buttons based on task status.
#[component]
fn TaskActions(
    task_id: String,
    status: String,
    on_action: impl Fn() + Copy + Send + Sync + 'static,
) -> impl IntoView {
    let (busy, set_busy) = signal(false);

    let run_action = move |action: &'static str, tid: String| {
        set_busy.set(true);
        leptos::task::spawn_local(async move {
            let result = match action {
                "start" => api::start_task(&tid).await,
                "done" => api::done_task(&tid).await,
                "block" => api::block_task(&tid, "Blocked via dashboard").await,
                "restart" => api::restart_task(&tid).await,
                _ => Ok(()),
            };
            if let Err(e) = result {
                leptos::logging::error!("Action {} failed: {}", action, e);
            }
            set_busy.set(false);
            on_action();
        });
    };

    let disabled = Signal::derive(move || busy.get());

    match status.as_str() {
        "todo" => {
            let tid = task_id.clone();
            view! {
                <div style="display: flex; gap: var(--space-1)">
                    <Button label="Start" variant="success" disabled=disabled on_click=move || run_action("start", tid.clone()) />
                </div>
            }.into_any()
        }
        "in_progress" => {
            let tid_done = task_id.clone();
            let tid_block = task_id.clone();
            view! {
                <div style="display: flex; gap: var(--space-1)">
                    <Button label="Done" variant="success" disabled=disabled on_click=move || run_action("done", tid_done.clone()) />
                    <Button label="Block" variant="danger" disabled=disabled on_click=move || run_action("block", tid_block.clone()) />
                </div>
            }.into_any()
        }
        "blocked" | "failed" => {
            let tid = task_id.clone();
            view! {
                <div style="display: flex; gap: var(--space-1)">
                    <Button label="Restart" variant="warning" disabled=disabled on_click=move || run_action("restart", tid.clone()) />
                </div>
            }.into_any()
        }
        _ => view! { <div></div> }.into_any(),
    }
}
