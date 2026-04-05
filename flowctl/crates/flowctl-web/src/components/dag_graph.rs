//! Interactive DAG graph component using Canvas 2D rendering.
//!
//! Features:
//! - Canvas 2D node/edge rendering with zoom (wheel) and pan (background drag)
//! - Hover tooltip showing task title, status, depends_on, domain
//! - Drag-to-create/delete dependency edges and retry/skip action buttons
//! - WebSocket real-time updates on task status changes

use leptos::prelude::*;

use crate::api::{self, DagResponse};

/// Base node dimensions.
const NODE_WIDTH: f64 = 120.0;
const NODE_HEIGHT: f64 = 40.0;
#[allow(dead_code)]
const NODE_RADIUS: f64 = 8.0;
/// Arrowhead size.
#[allow(dead_code)]
const ARROW_SIZE: f64 = 8.0;
/// Canvas pixel ratio scaling (set at runtime).
const CANVAS_PADDING: f64 = 60.0;

/// Zoom limits.
const MIN_ZOOM: f64 = 0.3;
const MAX_ZOOM: f64 = 3.0;
const ZOOM_STEP: f64 = 0.1;

/// Tooltip dimensions.
#[allow(dead_code)]
const TOOLTIP_PADDING: f64 = 10.0;
#[allow(dead_code)]
const TOOLTIP_LINE_HEIGHT: f64 = 16.0;
#[allow(dead_code)]
const TOOLTIP_MAX_WIDTH: f64 = 240.0;

/// Color for a task status (task spec colors).
#[allow(dead_code)]
fn status_color(status: &str) -> &'static str {
    match status {
        "done" => "#10b981",
        "in_progress" => "#f59e0b",
        "blocked" | "failed" | "upstream_failed" => "#ef4444",
        "skipped" => "#6b7280",
        "up_for_retry" => "#ea580c",
        _ => "#6b7280", // todo
    }
}

#[allow(dead_code)]
fn status_text_color(status: &str) -> &'static str {
    match status {
        "done" | "in_progress" | "blocked" | "failed" | "upstream_failed" | "up_for_retry" => {
            "#ffffff"
        }
        _ => "#d1d5db",
    }
}

/// Status icon character.
#[allow(dead_code)]
fn status_icon(status: &str) -> &'static str {
    match status {
        "done" => "OK",
        "in_progress" => ">>",
        "blocked" | "failed" | "upstream_failed" => "!!",
        "skipped" => "--",
        "up_for_retry" => "RT",
        _ => "..",
    }
}

/// Whether a task status supports retry.
fn can_retry(status: &str) -> bool {
    matches!(status, "failed" | "blocked")
}

/// Whether a task status supports skip.
fn can_skip(status: &str) -> bool {
    status == "todo"
}

/// Draw a rounded rectangle on the canvas context.
#[cfg(feature = "hydrate")]
fn draw_rounded_rect(
    ctx: &web_sys::CanvasRenderingContext2d,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    r: f64,
) {
    ctx.begin_path();
    ctx.move_to(x + r, y);
    ctx.line_to(x + w - r, y);
    ctx.arc_to(x + w, y, x + w, y + r, r).unwrap_or(());
    ctx.line_to(x + w, y + h - r);
    ctx.arc_to(x + w, y + h, x + w - r, y + h, r).unwrap_or(());
    ctx.line_to(x + r, y + h);
    ctx.arc_to(x, y + h, x, y + h - r, r).unwrap_or(());
    ctx.line_to(x, y + r);
    ctx.arc_to(x, y, x + r, y, r).unwrap_or(());
    ctx.close_path();
}

/// Draw a quadratic bezier edge with arrowhead.
#[cfg(feature = "hydrate")]
fn draw_edge(
    ctx: &web_sys::CanvasRenderingContext2d,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
) {
    let cp_x = (x1 + x2) / 2.0;

    // Draw curve.
    ctx.begin_path();
    ctx.move_to(x1, y1);
    ctx.bezier_curve_to(cp_x, y1, cp_x, y2, x2, y2);
    ctx.set_stroke_style_str("#6b7280");
    ctx.set_line_width(2.0);
    ctx.stroke();

    // Arrowhead.
    let dx = x2 - cp_x;
    let dy = y2 - y2; // 0
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let ux = dx / len;
    let uy = dy / len;

    let bx1 = x2 - ux * ARROW_SIZE + uy * ARROW_SIZE * 0.5;
    let by1 = y2 - uy * ARROW_SIZE - ux * ARROW_SIZE * 0.5;
    let bx2 = x2 - ux * ARROW_SIZE - uy * ARROW_SIZE * 0.5;
    let by2 = y2 - uy * ARROW_SIZE + ux * ARROW_SIZE * 0.5;

    ctx.begin_path();
    ctx.move_to(x2, y2);
    ctx.line_to(bx1, by1);
    ctx.line_to(bx2, by2);
    ctx.close_path();
    ctx.set_fill_style_str("#6b7280");
    ctx.fill();
}

/// Draw a hover tooltip for a node.
#[cfg(feature = "hydrate")]
fn draw_tooltip(
    ctx: &web_sys::CanvasRenderingContext2d,
    node: &crate::api::DagNode,
    deps: &[String],
) {
    // Build tooltip lines.
    let lines: Vec<String> = vec![
        node.title.clone(),
        format!("Status: {}", node.status),
        format!("Domain: {}", if node.domain.is_empty() { "general" } else { &node.domain }),
        if deps.is_empty() {
            "Deps: none".to_string()
        } else {
            format!("Deps: {}", deps.join(", "))
        },
    ];

    // Measure text width.
    ctx.set_font("12px sans-serif");
    let mut max_w: f64 = 0.0;
    for line in &lines {
        if let Ok(metrics) = ctx.measure_text(line) {
            max_w = max_w.max(metrics.width());
        }
    }
    let box_w = (max_w + TOOLTIP_PADDING * 2.0).min(TOOLTIP_MAX_WIDTH);
    let box_h = TOOLTIP_PADDING * 2.0 + lines.len() as f64 * TOOLTIP_LINE_HEIGHT;

    // Position tooltip to the right and slightly above the node.
    let tx = node.x + NODE_WIDTH / 2.0 + 8.0;
    let ty = node.y - box_h / 2.0;

    // Background.
    draw_rounded_rect(ctx, tx, ty, box_w, box_h, 6.0);
    ctx.set_fill_style_str("#1f2937");
    ctx.fill();
    ctx.set_stroke_style_str("#374151");
    ctx.set_line_width(1.0);
    ctx.stroke();

    // Text.
    ctx.set_fill_style_str("#e5e7eb");
    ctx.set_text_align("left");
    ctx.set_text_baseline("top");
    for (i, line) in lines.iter().enumerate() {
        let ly = ty + TOOLTIP_PADDING + i as f64 * TOOLTIP_LINE_HEIGHT;
        if i == 0 {
            ctx.set_font("bold 12px sans-serif");
        } else {
            ctx.set_font("11px sans-serif");
            ctx.set_fill_style_str("#9ca3af");
        }
        let _ = ctx.fill_text(line, tx + TOOLTIP_PADDING, ly);
    }
}

/// Full canvas redraw.
#[cfg(feature = "hydrate")]
fn redraw_canvas(
    ctx: &web_sys::CanvasRenderingContext2d,
    dag: &DagResponse,
    canvas_w: f64,
    canvas_h: f64,
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    drag_source: Option<&str>,
    drag_pos: Option<(f64, f64)>,
    hover_node_id: Option<&str>,
) {
    use std::collections::HashMap;

    // Clear.
    ctx.clear_rect(0.0, 0.0, canvas_w, canvas_h);

    ctx.save();
    ctx.translate(offset_x, offset_y).unwrap_or(());
    ctx.scale(scale, scale).unwrap_or(());

    let node_positions: HashMap<&str, (f64, f64)> =
        dag.nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();

    // Draw edges.
    for edge in &dag.edges {
        let from = node_positions.get(edge.from.as_str()).copied().unwrap_or((0.0, 0.0));
        let to = node_positions.get(edge.to.as_str()).copied().unwrap_or((0.0, 0.0));
        let x1 = from.0 + NODE_WIDTH / 2.0;
        let y1 = from.1;
        let x2 = to.0 - NODE_WIDTH / 2.0;
        let y2 = to.1;
        draw_edge(ctx, x1, y1, x2, y2);
    }

    // Draw drag preview line.
    if let (Some(src), Some((mx, my))) = (drag_source, drag_pos) {
        if let Some(&(sx, sy)) = node_positions.get(src) {
            let x1 = sx + NODE_WIDTH / 2.0;
            ctx.begin_path();
            ctx.move_to(x1, sy);
            ctx.line_to(mx, my);
            ctx.set_stroke_style_str("#60a5fa");
            ctx.set_line_width(2.0);
            ctx.set_line_dash(
                &js_sys::Array::of2(&wasm_bindgen::JsValue::from_f64(6.0), &wasm_bindgen::JsValue::from_f64(3.0)),
            ).unwrap_or(());
            ctx.stroke();
            ctx.set_line_dash(&js_sys::Array::new()).unwrap_or(());
        }
    }

    // Draw nodes.
    for node in &dag.nodes {
        let rx = node.x - NODE_WIDTH / 2.0;
        let ry = node.y - NODE_HEIGHT / 2.0;
        let fill = status_color(&node.status);
        let text_fill = status_text_color(&node.status);
        let icon = status_icon(&node.status);

        // Highlight hovered node.
        let is_hovered = hover_node_id == Some(node.id.as_str());

        // Node background.
        draw_rounded_rect(ctx, rx, ry, NODE_WIDTH, NODE_HEIGHT, NODE_RADIUS);
        ctx.set_fill_style_str(fill);
        ctx.fill();
        if is_hovered {
            ctx.set_stroke_style_str("#60a5fa");
            ctx.set_line_width(2.0);
        } else {
            ctx.set_stroke_style_str("#4b5563");
            ctx.set_line_width(1.0);
        }
        ctx.stroke();

        // Short ID + status icon inside node.
        let short_id = if let Some(dot_pos) = node.id.rfind('.') {
            &node.id[dot_pos..]
        } else {
            &node.id
        };
        let label_inside = format!("{} {}", short_id, icon);
        ctx.set_fill_style_str(text_fill);
        ctx.set_font("bold 11px monospace");
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&label_inside, node.x, node.y);

        // Title below node (truncated to 20 chars).
        let title = if node.title.len() > 20 {
            format!("{}...", &node.title[..17])
        } else {
            node.title.clone()
        };
        ctx.set_fill_style_str("#d1d5db");
        ctx.set_font("10px sans-serif");
        let _ = ctx.fill_text(&title, node.x, node.y + NODE_HEIGHT / 2.0 + 12.0);

        // Retry button for failed/blocked.
        if can_retry(&node.status) {
            let bx = rx + NODE_WIDTH - 38.0;
            let by = ry + NODE_HEIGHT + 4.0;
            draw_rounded_rect(ctx, bx, by, 36.0, 16.0, 3.0);
            ctx.set_fill_style_str("#1e40af");
            ctx.fill();
            ctx.set_fill_style_str("#ffffff");
            ctx.set_font("9px sans-serif");
            let _ = ctx.fill_text("Retry", bx + 18.0, by + 9.0);
        }

        // Skip button for todo.
        if can_skip(&node.status) {
            let bx = rx + 2.0;
            let by = ry + NODE_HEIGHT + 4.0;
            draw_rounded_rect(ctx, bx, by, 36.0, 16.0, 3.0);
            ctx.set_fill_style_str("#6b7280");
            ctx.fill();
            ctx.set_fill_style_str("#ffffff");
            ctx.set_font("9px sans-serif");
            let _ = ctx.fill_text("Skip", bx + 18.0, by + 9.0);
        }
    }

    // Draw tooltip for hovered node (drawn last so it's on top).
    if let Some(hid) = hover_node_id {
        if let Some(node) = dag.nodes.iter().find(|n| n.id == hid) {
            // Derive depends_on from edges.
            let deps: Vec<String> = dag.edges.iter()
                .filter(|e| e.to == node.id)
                .map(|e| {
                    // Show short ID for deps.
                    if let Some(dot_pos) = e.from.rfind('.') {
                        e.from[dot_pos..].to_string()
                    } else {
                        e.from.clone()
                    }
                })
                .collect();
            draw_tooltip(ctx, node, &deps);
        }
    }

    ctx.restore();
}

/// Interactive Canvas DAG graph component.
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
    let (error_msg, set_error_msg) = signal(Option::<String>::None);

    if dag.nodes.is_empty() {
        return view! {
            <p class="text-gray-400">"No tasks in this epic."</p>
        }
        .into_any();
    }

    // Compute world bounds for the graph.
    let min_x = dag.nodes.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
    let max_x = dag.nodes.iter().map(|n| n.x).fold(f64::NEG_INFINITY, f64::max);
    let min_y = dag.nodes.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
    let max_y = dag.nodes.iter().map(|n| n.y).fold(f64::NEG_INFINITY, f64::max);
    let world_w = (max_x - min_x) + NODE_WIDTH + CANVAS_PADDING * 2.0;
    let world_h = (max_y - min_y) + NODE_HEIGHT + CANVAS_PADDING * 2.0 + 30.0;
    let world_origin_x = min_x - CANVAS_PADDING;
    let world_origin_y = min_y - CANVAS_PADDING - NODE_HEIGHT / 2.0;

    // Canvas pixel size.
    let canvas_w = 800.0_f64;
    let canvas_h = (canvas_w * world_h / world_w).max(400.0);
    let initial_scale = canvas_w / world_w;
    let initial_offset_x = -world_origin_x * initial_scale;
    let initial_offset_y = -world_origin_y * initial_scale;

    // Store DAG in a signal so create_effect can capture it.
    let dag_signal = StoredValue::new(dag);
    let version_stored = StoredValue::new(version);

    // Drag state.
    let (drag_source, set_drag_source) = signal(Option::<String>::None);
    let (drag_pos, set_drag_pos) = signal(Option::<(f64, f64)>::None);

    // Zoom/pan state.
    let (zoom_level, set_zoom_level) = signal(initial_scale);
    let (view_offset_x, set_view_offset_x) = signal(initial_offset_x);
    let (view_offset_y, set_view_offset_y) = signal(initial_offset_y);

    // Pan state (background drag).
    let (is_panning, set_is_panning) = signal(false);
    let (pan_start, set_pan_start) = signal((0.0_f64, 0.0_f64));
    let (pan_offset_start, set_pan_offset_start) = signal((0.0_f64, 0.0_f64));

    // Hover state.
    let (hover_node, set_hover_node) = signal(Option::<String>::None);

    // Canvas ref -- client-only rendering.
    #[cfg(feature = "hydrate")]
    let canvas_ref = leptos::prelude::NodeRef::<leptos::html::Canvas>::new();

    // Redraw trigger signal.
    let (redraw_tick, set_redraw_tick) = signal(0u32);

    // Canvas redraw effect (client-side only).
    #[cfg(feature = "hydrate")]
    {
        let cr = canvas_ref;
        Effect::new(move |_| {
            // Subscribe to all reactive state.
            let _ = redraw_tick.get();
            let ds = drag_source.get();
            let dp = drag_pos.get();
            let scale = zoom_level.get();
            let ox = view_offset_x.get();
            let oy = view_offset_y.get();
            let hn = hover_node.get();

            if let Some(canvas_el) = cr.get() {
                use wasm_bindgen::JsCast;
                let canvas: &web_sys::HtmlCanvasElement = canvas_el.as_ref();
                if let Ok(Some(ctx_obj)) = canvas.get_context("2d") {
                    if let Ok(ctx) = ctx_obj.dyn_into::<web_sys::CanvasRenderingContext2d>() {
                        dag_signal.with_value(|dag| {
                            redraw_canvas(
                                &ctx,
                                dag,
                                canvas_w,
                                canvas_h,
                                ox,
                                oy,
                                scale,
                                ds.as_deref(),
                                dp,
                                hn.as_deref(),
                            );
                        });
                    }
                }
            }
        });
    }

    // WebSocket real-time updates (client-side only).
    #[cfg(feature = "hydrate")]
    {
        let set_rt = set_redraw_tick;
        let on_mut = on_mutated;
        Effect::new(move |_| {
            spawn_ws_updater(on_mut, set_rt);
        });
    }

    // Edge set for checking existing edges.
    let edge_set = StoredValue::new({
        let d = dag_signal.get_value();
        d.edges
            .iter()
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect::<std::collections::HashSet<(String, String)>>()
    });

    // Convert pixel coords to world coords using current zoom/pan.
    let pixel_to_world = move |px: f64, py: f64| -> (f64, f64) {
        let scale = zoom_level.get_untracked();
        let ox = view_offset_x.get_untracked();
        let oy = view_offset_y.get_untracked();
        ((px - ox) / scale, (py - oy) / scale)
    };

    // Hit-test: find which node (if any) is at world coords.
    let hit_test_node = move |wx: f64, wy: f64| -> Option<String> {
        dag_signal.with_value(|dag| {
            for node in dag.nodes.iter().rev() {
                let rx = node.x - NODE_WIDTH / 2.0;
                let ry = node.y - NODE_HEIGHT / 2.0;
                if wx >= rx && wx <= rx + NODE_WIDTH && wy >= ry && wy <= ry + NODE_HEIGHT {
                    return Some(node.id.clone());
                }
            }
            None
        })
    };

    // Hit-test for retry/skip buttons.
    let hit_test_button = move |wx: f64, wy: f64| -> Option<(&'static str, String)> {
        dag_signal.with_value(|dag| {
            for node in &dag.nodes {
                let rx = node.x - NODE_WIDTH / 2.0;
                let ry = node.y - NODE_HEIGHT / 2.0;

                if can_retry(&node.status) {
                    let bx = rx + NODE_WIDTH - 38.0;
                    let by = ry + NODE_HEIGHT + 4.0;
                    if wx >= bx && wx <= bx + 36.0 && wy >= by && wy <= by + 16.0 {
                        return Some(("retry_task", node.id.clone()));
                    }
                }
                if can_skip(&node.status) {
                    let bx = rx + 2.0;
                    let by = ry + NODE_HEIGHT + 4.0;
                    if wx >= bx && wx <= bx + 36.0 && wy >= by && wy <= by + 16.0 {
                        return Some(("skip_task", node.id.clone()));
                    }
                }
            }
            None
        })
    };

    // Hit-test edges (approximate: check distance to the bezier midpoint region).
    let hit_test_edge = move |wx: f64, wy: f64| -> Option<(String, String)> {
        dag_signal.with_value(|dag| {
            let node_positions: std::collections::HashMap<&str, (f64, f64)> =
                dag.nodes.iter().map(|n| (n.id.as_str(), (n.x, n.y))).collect();

            for edge in &dag.edges {
                let from = node_positions.get(edge.from.as_str()).copied().unwrap_or((0.0, 0.0));
                let to = node_positions.get(edge.to.as_str()).copied().unwrap_or((0.0, 0.0));
                let x1 = from.0 + NODE_WIDTH / 2.0;
                let y1 = from.1;
                let x2 = to.0 - NODE_WIDTH / 2.0;
                let y2 = to.1;

                for i in 0..=20 {
                    let t = i as f64 / 20.0;
                    let cp_x = (x1 + x2) / 2.0;
                    let u = 1.0 - t;
                    let px = u * u * u * x1 + 3.0 * u * u * t * cp_x + 3.0 * u * t * t * cp_x + t * t * t * x2;
                    let py = u * u * u * y1 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y2;
                    let dx = wx - px;
                    let dy = wy - py;
                    if (dx * dx + dy * dy).sqrt() < 6.0 {
                        return Some((edge.from.clone(), edge.to.clone()));
                    }
                }
            }
            None
        })
    };

    // Wheel event: zoom in/out centered on mouse position.
    let on_wheel = move |evt: leptos::ev::WheelEvent| {
        evt.prevent_default();
        let delta = if evt.delta_y() > 0.0 { -ZOOM_STEP } else { ZOOM_STEP };
        let old_scale = zoom_level.get_untracked();
        let new_scale = (old_scale + delta).clamp(MIN_ZOOM, MAX_ZOOM);
        if (new_scale - old_scale).abs() < 0.001 {
            return;
        }

        // Zoom toward mouse position.
        let mx = evt.offset_x() as f64;
        let my = evt.offset_y() as f64;
        let ox = view_offset_x.get_untracked();
        let oy = view_offset_y.get_untracked();

        // World point under mouse stays fixed.
        let new_ox = mx - (mx - ox) * (new_scale / old_scale);
        let new_oy = my - (my - oy) * (new_scale / old_scale);

        set_zoom_level.set(new_scale);
        set_view_offset_x.set(new_ox);
        set_view_offset_y.set(new_oy);
    };

    // Mouse handlers.
    let on_mousedown = move |evt: leptos::ev::MouseEvent| {
        let (wx, wy) = pixel_to_world(evt.offset_x() as f64, evt.offset_y() as f64);

        // Check buttons first.
        if let Some((action, task_id)) = hit_test_button(wx, wy) {
            let ver = version_stored.get_value();
            let set_err = set_error_msg;
            let on_mut = on_mutated;
            leptos::task::spawn_local(async move {
                let params = serde_json::json!({"task_id": task_id});
                match api::mutate_dag(action, params, &ver).await {
                    Ok(_) => on_mut.run(()),
                    Err(e) => set_err.set(Some(e)),
                }
            });
            return;
        }

        // Check node hit for drag start.
        if let Some(node_id) = hit_test_node(wx, wy) {
            set_drag_source.set(Some(node_id));
            set_drag_pos.set(None);
            return;
        }

        // Background click: start panning.
        set_is_panning.set(true);
        set_pan_start.set((evt.offset_x() as f64, evt.offset_y() as f64));
        set_pan_offset_start.set((
            view_offset_x.get_untracked(),
            view_offset_y.get_untracked(),
        ));
    };

    let on_mousemove = move |evt: leptos::ev::MouseEvent| {
        let px = evt.offset_x() as f64;
        let py = evt.offset_y() as f64;

        // Panning.
        if is_panning.get_untracked() {
            let (sx, sy) = pan_start.get_untracked();
            let (osx, osy) = pan_offset_start.get_untracked();
            set_view_offset_x.set(osx + (px - sx));
            set_view_offset_y.set(osy + (py - sy));
            return;
        }

        // Drag preview.
        if drag_source.get_untracked().is_some() {
            let (wx, wy) = pixel_to_world(px, py);
            set_drag_pos.set(Some((wx, wy)));
            return;
        }

        // Hover detection.
        let (wx, wy) = pixel_to_world(px, py);
        let hovered = hit_test_node(wx, wy);
        let current = hover_node.get_untracked();
        if hovered != current {
            set_hover_node.set(hovered);
        }
    };

    let on_mouseup = move |evt: leptos::ev::MouseEvent| {
        // End panning.
        if is_panning.get_untracked() {
            set_is_panning.set(false);
            return;
        }

        let (wx, wy) = pixel_to_world(evt.offset_x() as f64, evt.offset_y() as f64);

        if let Some(source) = drag_source.get_untracked() {
            set_drag_source.set(None);
            set_drag_pos.set(None);

            // Check if dropped on a target node.
            if let Some(target_id) = hit_test_node(wx, wy) {
                if source != target_id {
                    let es = edge_set.get_value();
                    if !es.contains(&(source.clone(), target_id.clone())) {
                        let ver = version_stored.get_value();
                        let set_err = set_error_msg;
                        let on_mut = on_mutated;
                        leptos::task::spawn_local(async move {
                            let params = serde_json::json!({"task_id": target_id, "depends_on": source});
                            match api::mutate_dag("add_dep", params, &ver).await {
                                Ok(_) => on_mut.run(()),
                                Err(e) => set_err.set(Some(e)),
                            }
                        });
                    }
                }
            }
            // Trigger redraw to clear drag line.
            set_redraw_tick.update(|v| *v += 1);
            return;
        }

        // Click on edge to delete.
        if let Some((from, to)) = hit_test_edge(wx, wy) {
            let ver = version_stored.get_value();
            let set_err = set_error_msg;
            let on_mut = on_mutated;
            leptos::task::spawn_local(async move {
                let params = serde_json::json!({"task_id": to, "depends_on": from});
                match api::mutate_dag("remove_dep", params, &ver).await {
                    Ok(_) => on_mut.run(()),
                    Err(e) => set_err.set(Some(e)),
                }
            });
        }
    };

    // Mouse leave: clear hover and end pan.
    let on_mouseleave = move |_: leptos::ev::MouseEvent| {
        set_hover_node.set(None);
        set_is_panning.set(false);
    };

    // SSR: render a placeholder div. Hydrate: render a canvas.
    #[cfg(feature = "hydrate")]
    let canvas_view = view! {
        <canvas
            node_ref=canvas_ref
            width={canvas_w.to_string()}
            height={canvas_h.to_string()}
            style="width: 100%; cursor: grab;"
            on:mousedown=on_mousedown
            on:mousemove=on_mousemove
            on:mouseup=on_mouseup
            on:mouseleave=on_mouseleave
            on:wheel=on_wheel
        />
    }
    .into_any();

    #[cfg(not(feature = "hydrate"))]
    let canvas_view = {
        // Suppress unused warnings in SSR mode.
        let _ = on_mousedown;
        let _ = on_mousemove;
        let _ = on_mouseup;
        let _ = on_mouseleave;
        let _ = on_wheel;
        let _ = set_redraw_tick;
        let _ = redraw_tick;
        let _ = drag_source;
        let _ = set_drag_source;
        let _ = drag_pos;
        let _ = set_drag_pos;
        let _ = zoom_level;
        let _ = set_zoom_level;
        let _ = view_offset_x;
        let _ = set_view_offset_x;
        let _ = view_offset_y;
        let _ = set_view_offset_y;
        let _ = is_panning;
        let _ = set_is_panning;
        let _ = pan_start;
        let _ = set_pan_start;
        let _ = pan_offset_start;
        let _ = set_pan_offset_start;
        let _ = hover_node;
        let _ = set_hover_node;
        let _ = pixel_to_world;
        let _ = hit_test_node;
        let _ = hit_test_button;
        let _ = hit_test_edge;
        let _ = edge_set;
        view! {
            <div style={format!("width:100%;height:{}px;background:#1f2937;border-radius:8px;display:flex;align-items:center;justify-content:center;", canvas_h)}>
                <p class="text-gray-400">"Loading DAG canvas..."</p>
            </div>
        }
        .into_any()
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
                "Drag between nodes to add deps. Click edge to remove. Scroll to zoom. Drag background to pan. Hover for details."
            </p>
            {canvas_view}
        </div>
    }
    .into_any()
}

/// Spawn a WebSocket listener that triggers DAG refresh on task status changes.
///
/// Connects to /api/v1/events and calls on_mutated + bumps redraw tick on updates.
#[cfg(feature = "hydrate")]
fn spawn_ws_updater(on_mutated: Callback<()>, set_redraw_tick: WriteSignal<u32>) {
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    let window = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let location = window.location();
    let protocol = location.protocol().unwrap_or_else(|_| "http:".to_string());
    let ws_protocol = if protocol == "https:" { "wss:" } else { "ws:" };
    let host = location.host().unwrap_or_else(|_| "localhost:17319".to_string());
    let ws_url = format!("{ws_protocol}//{host}/api/v1/events");

    let ws = match web_sys::WebSocket::new(&ws_url) {
        Ok(ws) => ws,
        Err(_) => return,
    };

    let on_message = Closure::<dyn Fn(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        if let Some(text) = event.data().as_string() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                let evt = value.get("event").unwrap_or(&value);
                // Refresh on any task status change event.
                if evt.get("task_id").is_some() && evt.get("new_status").is_some() {
                    on_mutated.run(());
                    set_redraw_tick.update(|v| *v += 1);
                }
            }
        }
    });
    ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    on_message.forget();
}
