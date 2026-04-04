//! SVG bar chart component showing token consumption per task.

use leptos::prelude::*;

use crate::api::TaskTokenSummary;

/// Bar colors for input vs output tokens.
const INPUT_COLOR: &str = "#06b6d4";  // cyan-500
const OUTPUT_COLOR: &str = "#8b5cf6"; // violet-500

/// SVG bar chart showing token consumption per task.
#[component]
pub fn TokenChart(
    #[prop(into)] data: Vec<TaskTokenSummary>,
) -> impl IntoView {
    if data.is_empty() {
        return view! {
            <p class="text-gray-400 text-sm">"No token data available."</p>
        }
        .into_any();
    }

    let max_tokens = data
        .iter()
        .map(|d| d.input_tokens + d.output_tokens)
        .max()
        .unwrap_or(1)
        .max(1);

    let bar_height = 28.0_f64;
    let gap = 8.0_f64;
    let label_width = 140.0_f64;
    let chart_width = 500.0_f64;
    let total_width = label_width + chart_width + 80.0;
    let total_height = data.len() as f64 * (bar_height + gap) + gap + 30.0;

    let bars: Vec<_> = data
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let y = gap + i as f64 * (bar_height + gap);
            let input_w = (item.input_tokens as f64 / max_tokens as f64) * chart_width;
            let output_w = (item.output_tokens as f64 / max_tokens as f64) * chart_width;
            let total = item.input_tokens + item.output_tokens;
            // Truncate long task IDs for display.
            let label = if item.task_id.len() > 18 {
                format!("{}...", &item.task_id[..15])
            } else {
                item.task_id.clone()
            };
            let cost_label = format!("${:.3}", item.estimated_cost);
            view! {
                <g>
                    // Task ID label
                    <text x={(label_width - 8.0).to_string()} y={(y + bar_height / 2.0 + 4.0).to_string()}
                          text-anchor="end" fill="#d1d5db" font-size="11">{label}</text>
                    // Input tokens bar
                    <rect x={label_width.to_string()} y={y.to_string()}
                          width={input_w.to_string()} height={bar_height.to_string()}
                          rx="3" fill={INPUT_COLOR}/>
                    // Output tokens bar (stacked after input)
                    <rect x={(label_width + input_w).to_string()} y={y.to_string()}
                          width={output_w.to_string()} height={bar_height.to_string()}
                          rx="3" fill={OUTPUT_COLOR}/>
                    // Total label
                    <text x={(label_width + input_w + output_w + 6.0).to_string()}
                          y={(y + bar_height / 2.0 + 4.0).to_string()}
                          fill="#9ca3af" font-size="10">
                        {format!("{total} tok / {cost_label}")}
                    </text>
                </g>
            }
        })
        .collect();

    // Legend at the bottom.
    let legend_y = data.len() as f64 * (bar_height + gap) + gap + 10.0;

    view! {
        <svg viewBox={format!("0 0 {total_width} {total_height}")} class="w-full" style="min-height: 200px;">
            {bars}
            // Legend
            <rect x={label_width.to_string()} y={legend_y.to_string()} width="12" height="12" rx="2" fill={INPUT_COLOR}/>
            <text x={(label_width + 18.0).to_string()} y={(legend_y + 10.0).to_string()} fill="#9ca3af" font-size="11">"Input"</text>
            <rect x={(label_width + 70.0).to_string()} y={legend_y.to_string()} width="12" height="12" rx="2" fill={OUTPUT_COLOR}/>
            <text x={(label_width + 88.0).to_string()} y={(legend_y + 10.0).to_string()} fill="#9ca3af" font-size="11">"Output"</text>
        </svg>
    }
    .into_any()
}
