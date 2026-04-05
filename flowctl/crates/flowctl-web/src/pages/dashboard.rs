//! Dashboard page: global stats overview with progress ring, timeline, and epic grid.

use leptos::prelude::*;

use crate::api;

/// Format token count with K/M suffix.
fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

/// Dashboard page component — global stats overview.
#[component]
pub fn DashboardPage() -> impl IntoView {
    let stats = LocalResource::new(move || async move {
        api::fetch_stats().await.unwrap_or_default()
    });
    let epics = LocalResource::new(move || async move {
        api::fetch_epics().await.unwrap_or_default()
    });

    view! {
        <div class="fade-in">
            <h1>"Dashboard"</h1>

            // --- Stats Row ---
            <Suspense fallback=move || view! { <div class="stats-row"><p style="color: var(--color-text-muted)">"Loading stats..."</p></div> }>
                {move || {
                    stats.get().map(|s| {
                        let pct = if s.total_tasks > 0 {
                            (s.done_tasks as f64 / s.total_tasks as f64 * 100.0) as u32
                        } else { 0 };
                        let epics_label = format!("{}/{}", s.open_epics, s.total_epics);
                        let tasks_label = format!("{}/{} ({}%)", s.done_tasks, s.total_tasks, pct);
                        let in_progress_label = s.in_progress_tasks.to_string();
                        let tokens_label = format_tokens(s.total_tokens);

                        // Progress ring values (SVG)
                        let radius: f64 = 54.0;
                        let circumference = 2.0 * std::f64::consts::PI * radius;
                        let offset = circumference - (pct as f64 / 100.0) * circumference;
                        let pct_text = format!("{}%", pct);
                        let dash_array = format!("{}", circumference);
                        let dash_offset = format!("{}", offset);

                        view! {
                            // Stats cards
                            <div class="stats-row">
                                <div class="stat-card">
                                    <div class="stat-value">{epics_label}</div>
                                    <div class="stat-label">"Total Epics (open/total)"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-value">{tasks_label}</div>
                                    <div class="stat-label">"Tasks Done"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-value">{in_progress_label}</div>
                                    <div class="stat-label">"In Progress"</div>
                                </div>
                                <div class="stat-card">
                                    <div class="stat-value">{tokens_label}</div>
                                    <div class="stat-label">"Token Usage"</div>
                                </div>
                            </div>

                            // Progress ring
                            <div style="display: flex; justify-content: center; margin-bottom: var(--space-6)">
                                <svg width="140" height="140" viewBox="0 0 120 120">
                                    <circle
                                        cx="60" cy="60" r={radius.to_string()}
                                        fill="none"
                                        stroke="var(--color-bg-hover)"
                                        stroke-width="8"
                                    />
                                    <circle
                                        cx="60" cy="60" r={radius.to_string()}
                                        fill="none"
                                        stroke="var(--color-accent)"
                                        stroke-width="8"
                                        stroke-linecap="round"
                                        stroke-dasharray={dash_array}
                                        stroke-dashoffset={dash_offset}
                                        transform="rotate(-90 60 60)"
                                        style="transition: stroke-dashoffset 0.5s ease"
                                    />
                                    <text
                                        x="60" y="60"
                                        text-anchor="middle"
                                        dominant-baseline="central"
                                        fill="var(--color-text)"
                                        font-size="20"
                                        font-weight="700"
                                    >{pct_text}</text>
                                </svg>
                            </div>
                        }
                    })
                }}
            </Suspense>

            // --- Recent Activity Timeline ---
            <Suspense fallback=move || view! { <p style="color: var(--color-text-muted)">"Loading activity..."</p> }>
                {move || {
                    epics.get().map(|epics_data| {
                        let all_epics: Vec<_> = epics_data.into_iter().collect();
                        let recent: Vec<_> = all_epics.iter().take(10).cloned().collect();

                        let timeline_view = if recent.is_empty() {
                            view! {
                                <p style="color: var(--color-text-muted)">"No recent activity."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div style="display: flex; flex-direction: column; gap: var(--space-3); margin-bottom: var(--space-6)">
                                    {recent.into_iter().map(|epic| {
                                        let badge_class = match epic.status.as_str() {
                                            "done" => "badge badge-done",
                                            "closed" => "badge badge-closed",
                                            "in_progress" => "badge badge-in_progress",
                                            "blocked" => "badge badge-blocked",
                                            _ => "badge badge-todo",
                                        };
                                        let task_count = format!("{}/{} tasks", epic.done, epic.tasks);
                                        view! {
                                            <div class="card" style="padding: var(--space-3) var(--space-4)">
                                                <div class="flex-between">
                                                    <span style="font-weight: 500">{epic.title.clone()}</span>
                                                    <div style="display: flex; align-items: center; gap: var(--space-2)">
                                                        <span style="font-size: var(--text-xs); color: var(--color-text-dim)">{task_count}</span>
                                                        <span class={badge_class}>{epic.status.clone()}</span>
                                                    </div>
                                                </div>
                                            </div>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        };

                        // --- Epic Grid ---
                        let grid_view = if all_epics.is_empty() {
                            view! {
                                <p style="color: var(--color-text-muted)">"No epics found. Create one with flowctl epic create."</p>
                            }.into_any()
                        } else {
                            view! {
                                <div class="card-grid">
                                    {all_epics.into_iter().map(|epic| {
                                        let progress = if epic.tasks > 0 {
                                            (epic.done as f64 / epic.tasks as f64 * 100.0) as u32
                                        } else { 0 };
                                        let badge_class = match epic.status.as_str() {
                                            "done" => "badge badge-done",
                                            "closed" => "badge badge-closed",
                                            "in_progress" => "badge badge-in_progress",
                                            "blocked" => "badge badge-blocked",
                                            _ if progress > 0 => "badge badge-in_progress",
                                            _ => "badge badge-todo",
                                        };
                                        let link = format!("/epic/{}", epic.id);
                                        let width = format!("width: {}%", progress);
                                        let count = format!("{}/{}", epic.done, epic.tasks);
                                        view! {
                                            <a href={link} class="card" style="display: block; text-decoration: none; color: inherit;">
                                                <div class="flex-between" style="margin-bottom: var(--space-2)">
                                                    <h2 style="margin-bottom: 0">{epic.title.clone()}</h2>
                                                    <span class={badge_class}>{epic.status.clone()}</span>
                                                </div>
                                                <p style="font-size: var(--text-sm); color: var(--color-text-muted); margin-bottom: var(--space-2)">{epic.id.clone()}</p>
                                                <div style="display: flex; align-items: center; gap: var(--space-3)">
                                                    <div class="progress-track" style="flex: 1">
                                                        <div class="progress-fill" style={width}></div>
                                                    </div>
                                                    <span style="font-size: var(--text-sm); color: var(--color-text-muted)">{count}</span>
                                                </div>
                                            </a>
                                        }
                                    }).collect::<Vec<_>>()}
                                </div>
                            }.into_any()
                        };

                        view! {
                            <h2>"Recent Activity"</h2>
                            {timeline_view}
                            <h2>"Epics"</h2>
                            {grid_view}
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
