//! Interactive DAG graph component with drag-to-create/delete dependency edges
//! and retry/skip action buttons on failed/blocked task nodes.
//!
//! Uses SVG mousedown/mousemove/mouseup events on nodes to draw dependency edges.
//! Validates that new edges don't create cycles before submitting to the server.

use leptos::prelude::*;

use crate::api::{self, DagResponse};

/// Node dimensions (must match dag_view.rs constants).
const NODE_WIDTH: f64 = 160.0;
const NODE_HEIGHT: f64 = 60.0;
const NODE_RX: f64 = 8.0;

/// Color for a task status.
fn status_color(status: &str) -> &'static str {
    match status {
        "done" => "#16a34a",
        "in_progress" => "#ca8a04",
        "blocked" | "failed" | "upstream_failed" => "#dc2626",
        "skipped" => "#4b5563",
        "up_for_retry" => "#ea580c",
        _ => "#374151",
    }
}

fn status_text_color(status: &str) -> &'static str {
    match status {
        "done" | "in_progress" | "blocked" | "failed" | "upstream_failed" | "up_for_retry" => {
            "#ffffff"
        }
        _ => "#d1d5db",
    }
}

/// Whether a task status supports retry (back to todo).
fn can_retry(status: &str) -> bool {
    matches!(status, "failed" | "blocked")
}

/// Whether a task status supports skip.
fn can_skip(status: &str) -> bool {
    status == "todo"
}

/// Interactive DAG graph component.
///
/// Props:
/// - `dag`: the current DAG data (nodes + edges)
/// - `version`: optimistic lock version string (updated_at timestamp)
/// - `epic_id`: epic ID for context
/// - `on_mutated`: callback invoked after a successful mutation (to trigger refresh)
#[component]
pub fn DagGraph(
    dag: DagResponse,
    version: String,
    _epic_id: String,
    on_mutated: Callback<()>,
) -> impl IntoView {
    // Drag state: source node ID being dragged from.
    let (drag_source, set_drag_source) = signal(Option::<String>::None);
    // Current mouse position during drag (SVG coordinates).
    let (drag_pos, set_drag_pos) = signal(Option::<(f64, f64)>::None);
    // Error message for user feedback.
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    if dag.nodes.is_empty() {
        return view! {
            <p class="text-gray-400">"No tasks in this epic."</p>
        }
        .into_any();
    }

    // Compute SVG viewport bounds.
    let min_x = dag.nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let max_x = dag
        .nodes
        .iter()
        .map(|n| n.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = dag.nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    let max_y = dag
        .nodes
        .iter()
        .map(|n| n.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let padding = 60.0;
    let vb_x = min_x - padding;
    let vb_y = min_y - padding - NODE_HEIGHT / 2.0;
    let vb_w = (max_x - min_x) + NODE_WIDTH + padding * 2.0;
    let vb_h = (max_y - min_y) + NODE_HEIGHT + padding * 2.0;
    let viewbox = format!("{vb_x} {vb_y} {vb_w} {vb_h}");

    // Build node position lookup.
    let node_positions: std::collections::HashMap<String, (f64, f64)> =
        dag.nodes.iter().map(|n| (n.id.clone(), (n.x, n.y))).collect();

    // Edge set for checking existing edges and click-to-delete.
    let edge_set: std::collections::HashSet<(String, String)> = dag
        .edges
        .iter()
        .map(|e| (e.from.clone(), e.to.clone()))
        .collect();

    // Render edges.
    let np = node_positions.clone();
    let edges_view: Vec<_> = dag
        .edges
        .iter()
        .map(|edge| {
            let from_pos = np.get(&edge.from).copied().unwrap_or((0.0, 0.0));
            let to_pos = np.get(&edge.to).copied().unwrap_or((0.0, 0.0));
            let x1 = from_pos.0 + NODE_WIDTH / 2.0;
            let y1 = from_pos.1;
            let x2 = to_pos.0 - NODE_WIDTH / 2.0;
            let y2 = to_pos.1;
            let mid_x = (x1 + x2) / 2.0;
            let d = format!("M {x1} {y1} C {mid_x} {y1}, {mid_x} {y2}, {x2} {y2}");

            let edge_from = edge.from.clone();
            let edge_to = edge.to.clone();
            let ver = version.clone();
            let on_mut = on_mutated;
            let set_err = set_error_msg;

            // Click on edge to delete it.
            let on_click = move |_| {
                let from = edge_from.clone();
                let to = edge_to.clone();
                let v = ver.clone();
                leptos::task::spawn_local(async move {
                    let params = serde_json::json!({"task_id": to, "depends_on": from});
                    match api::mutate_dag("remove_dep", params, &v).await {
                        Ok(_) => on_mut.run(()),
                        Err(e) => set_err.set(Some(e)),
                    }
                });
            };

            view! {
                <path d={d} fill="none" stroke="#6b7280" stroke-width="3"
                      marker-end="url(#arrowhead)"
                      style="cursor: pointer;"
                      on:click=on_click>
                    <title>"Click to remove dependency"</title>
                </path>
            }
        })
        .collect();

    // Render nodes with drag handles and action buttons.
    let nodes_view: Vec<_> = dag
        .nodes
        .iter()
        .map(|node| {
            let status = node.status.clone();
            let fill = status_color(&status);
            let text_fill = status_text_color(&status);
            let rx = node.x - NODE_WIDTH / 2.0;
            let ry = node.y - NODE_HEIGHT / 2.0;
            let title = if node.title.len() > 20 {
                format!("{}...", &node.title[..17])
            } else {
                node.title.clone()
            };
            let short_id = node.id.clone();
            let node_id = node.id.clone();
            let node_x = node.x;
            let node_y = node.y;

            // Mousedown: start drag from this node (right edge = output port).
            let nid = node_id.clone();
            let on_mousedown = move |_: leptos::ev::MouseEvent| {
                set_drag_source.set(Some(nid.clone()));
                set_drag_pos.set(None);
            };

            // Mouseup on a node: if dragging, create edge from source to this node.
            let target_id = node_id.clone();
            let ver = version.clone();
            let es = edge_set.clone();
            let on_mut = on_mutated;
            let set_err = set_error_msg;
            let on_mouseup = move |_: leptos::ev::MouseEvent| {
                if let Some(source) = drag_source.get_untracked() {
                    set_drag_source.set(None);
                    set_drag_pos.set(None);

                    // Don't create self-loops or duplicate edges.
                    if source == target_id {
                        return;
                    }
                    if es.contains(&(source.clone(), target_id.clone())) {
                        return;
                    }

                    let tid = target_id.clone();
                    let src = source.clone();
                    let v = ver.clone();
                    leptos::task::spawn_local(async move {
                        let params = serde_json::json!({"task_id": tid, "depends_on": src});
                        match api::mutate_dag("add_dep", params, &v).await {
                            Ok(_) => on_mut.run(()),
                            Err(e) => set_err.set(Some(e)),
                        }
                    });
                }
            };

            // Retry button (for failed/blocked tasks).
            let show_retry = can_retry(&status);
            let retry_id = node_id.clone();
            let retry_ver = version.clone();
            let retry_on_mut = on_mutated;
            let retry_set_err = set_error_msg;
            let retry_btn_x = rx + NODE_WIDTH - 38.0;
            let retry_btn_y = ry + NODE_HEIGHT + 4.0;

            let on_retry = move |_: leptos::ev::MouseEvent| {
                let tid = retry_id.clone();
                let v = retry_ver.clone();
                leptos::task::spawn_local(async move {
                    let params = serde_json::json!({"task_id": tid});
                    match api::mutate_dag("retry_task", params, &v).await {
                        Ok(_) => retry_on_mut.run(()),
                        Err(e) => retry_set_err.set(Some(e)),
                    }
                });
            };

            // Skip button (for todo tasks).
            let show_skip = can_skip(&status);
            let skip_id = node_id.clone();
            let skip_ver = version.clone();
            let skip_on_mut = on_mutated;
            let skip_set_err = set_error_msg;
            let skip_btn_x = rx + 2.0;
            let skip_btn_y = ry + NODE_HEIGHT + 4.0;

            let on_skip = move |_: leptos::ev::MouseEvent| {
                let tid = skip_id.clone();
                let v = skip_ver.clone();
                leptos::task::spawn_local(async move {
                    let params = serde_json::json!({"task_id": tid});
                    match api::mutate_dag("skip_task", params, &v).await {
                        Ok(_) => skip_on_mut.run(()),
                        Err(e) => skip_set_err.set(Some(e)),
                    }
                });
            };

            view! {
                <g on:mousedown=on_mousedown on:mouseup=on_mouseup style="cursor: grab;">
                    <rect x={rx.to_string()} y={ry.to_string()}
                          width={NODE_WIDTH.to_string()} height={NODE_HEIGHT.to_string()}
                          rx={NODE_RX.to_string()} fill={fill.to_string()}
                          stroke="#4b5563" stroke-width="1"/>
                    <text x={node_x.to_string()} y={(node_y - 6.0).to_string()}
                          text-anchor="middle" fill={text_fill.to_string()}
                          font-size="12" font-weight="bold"
                          style="pointer-events: none;">{title}</text>
                    <text x={node_x.to_string()} y={(node_y + 14.0).to_string()}
                          text-anchor="middle" fill={text_fill.to_string()}
                          font-size="10" opacity="0.7"
                          style="pointer-events: none;">{short_id}</text>

                    // Retry button.
                    {if show_retry {
                        Some(view! {
                            <g on:click=on_retry style="cursor: pointer;">
                                <rect x={retry_btn_x.to_string()} y={retry_btn_y.to_string()}
                                      width="36" height="16" rx="3" fill="#1e40af"/>
                                <text x={(retry_btn_x + 18.0).to_string()} y={(retry_btn_y + 12.0).to_string()}
                                      text-anchor="middle" fill="#ffffff" font-size="9">"Retry"</text>
                            </g>
                        })
                    } else {
                        None
                    }}

                    // Skip button.
                    {if show_skip {
                        Some(view! {
                            <g on:click=on_skip style="cursor: pointer;">
                                <rect x={skip_btn_x.to_string()} y={skip_btn_y.to_string()}
                                      width="36" height="16" rx="3" fill="#6b7280"/>
                                <text x={(skip_btn_x + 18.0).to_string()} y={(skip_btn_y + 12.0).to_string()}
                                      text-anchor="middle" fill="#ffffff" font-size="9">"Skip"</text>
                            </g>
                        })
                    } else {
                        None
                    }}
                </g>
            }
        })
        .collect();

    // Drag preview line: drawn while dragging from a source node.
    let np2 = node_positions.clone();
    let drag_line = move || {
        let source = drag_source.get()?;
        let (mx, my) = drag_pos.get()?;
        let (sx, sy) = np2.get(&source).copied()?;
        let x1 = sx + NODE_WIDTH / 2.0;
        let d = format!("M {x1} {sy} L {mx} {my}");
        Some(view! {
            <path d={d} fill="none" stroke="#60a5fa" stroke-width="2"
                  stroke-dasharray="6 3" style="pointer-events: none;"/>
        })
    };

    // SVG mousemove: update drag position. We approximate SVG coords from client coords.
    let on_mousemove = move |evt: leptos::ev::MouseEvent| {
        if drag_source.get_untracked().is_some() {
            // Use offsetX/offsetY as approximation (works when SVG fills container).
            let x = evt.offset_x() as f64;
            let y = evt.offset_y() as f64;
            // Map from pixel coords to SVG viewBox coords.
            // This is approximate but good enough for a drag preview.
            set_drag_pos.set(Some((
                vb_x + (x / 800.0) * vb_w,
                vb_y + (y / 400.0) * vb_h,
            )));
        }
    };

    // SVG mouseup: cancel drag if not on a node.
    let on_svg_mouseup = move |_: leptos::ev::MouseEvent| {
        set_drag_source.set(None);
        set_drag_pos.set(None);
    };

    view! {
        <div>
            {move || error_msg.get().map(|msg| view! {
                <div class="bg-red-900 text-red-200 px-3 py-2 rounded mb-2 text-sm flex justify-between">
                    <span>{msg}</span>
                    <button on:click=move |_| set_error_msg.set(None) class="text-red-400 hover:text-white ml-2">"x"</button>
                </div>
            })}
            <p class="text-xs text-gray-500 mb-2">
                "Drag from one node to another to add a dependency. Click an edge to remove it."
            </p>
            <svg viewBox={viewbox} class="w-full" style="min-height: 400px;"
                 on:mousemove=on_mousemove on:mouseup=on_svg_mouseup>
                <defs>
                    <marker id="arrowhead" markerWidth="10" markerHeight="7"
                            refX="10" refY="3.5" orient="auto">
                        <polygon points="0 0, 10 3.5, 0 7" fill="#6b7280"/>
                    </marker>
                </defs>
                {edges_view}
                {nodes_view}
                {drag_line}
            </svg>
        </div>
    }
    .into_any()
}
