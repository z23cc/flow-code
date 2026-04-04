//! DAG tab - ASCII dependency graph with status-colored nodes.

use std::collections::{HashMap, HashSet};

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use flowctl_core::dag::TaskDag;
use flowctl_core::state_machine::Status;
use flowctl_core::types::Task;

use crate::action::{Action, ActionSender};
use crate::component::Component;

/// A positioned node in the graph layout.
#[derive(Debug, Clone)]
struct LayoutNode {
    id: String,
    /// Column (layer/depth from sources).
    col: usize,
    /// Row within the column.
    row: usize,
    status: Status,
    on_critical_path: bool,
}

pub struct DagTab {
    /// Rendered graph lines (cached).
    lines: Vec<Line<'static>>,
    /// All node IDs in layout order for navigation.
    node_ids: Vec<String>,
    /// Currently selected node index.
    selected: usize,
    /// Scroll offset (vertical).
    scroll_y: u16,
    /// Scroll offset (horizontal).
    scroll_x: u16,
    /// Whether data has been loaded.
    loaded: bool,
    /// Status map for coloring.
    statuses: HashMap<String, Status>,
    /// Critical path node IDs.
    critical_path: HashSet<String>,
    /// Layout nodes for navigation.
    layout_nodes: Vec<LayoutNode>,
}

impl Default for DagTab {
    fn default() -> Self {
        Self::new()
    }
}

impl DagTab {
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            node_ids: Vec::new(),
            selected: 0,
            scroll_y: 0,
            scroll_x: 0,
            loaded: false,
            statuses: HashMap::new(),
            critical_path: HashSet::new(),
            layout_nodes: Vec::new(),
        }
    }

    /// Load tasks and rebuild the ASCII graph.
    pub fn load_tasks(&mut self, tasks: &[Task]) {
        self.loaded = true;
        if tasks.is_empty() {
            self.lines = Vec::new();
            self.node_ids = Vec::new();
            self.layout_nodes = Vec::new();
            return;
        }

        self.statuses = tasks
            .iter()
            .map(|t| (t.id.clone(), t.status))
            .collect();

        let dag = match TaskDag::from_tasks(tasks) {
            Ok(d) => d,
            Err(e) => {
                self.lines = vec![Line::from(Span::styled(
                    format!("  DAG error: {e}"),
                    Style::default().fg(Color::Red),
                ))];
                return;
            }
        };

        self.critical_path = dag.critical_path().into_iter().collect();
        self.build_layout(tasks, &dag);
        self.render_graph(tasks, &dag);
    }

    /// Assign nodes to columns (layers) using topological depth.
    fn build_layout(&mut self, _tasks: &[Task], dag: &TaskDag) {
        // Compute depth for each node (longest path from any source).
        let mut depth: HashMap<String, usize> = HashMap::new();
        let topo_ids = dag.task_ids();

        // BFS-like layering: depth = max(depth of deps) + 1.
        for id in &topo_ids {
            let deps = dag.dependencies(id);
            let d = if deps.is_empty() {
                0
            } else {
                deps.iter()
                    .filter_map(|dep| depth.get(dep))
                    .max()
                    .copied()
                    .unwrap_or(0)
                    + 1
            };
            depth.insert(id.clone(), d);
        }

        // Group by depth and assign rows within each column.
        let max_depth = depth.values().copied().max().unwrap_or(0);
        let mut col_counts = vec![0usize; max_depth + 1];
        let mut nodes = Vec::new();

        // Sort by depth then ID for deterministic layout.
        let mut sorted: Vec<&String> = topo_ids.iter().collect();
        sorted.sort_by_key(|id| (depth.get(id.as_str()).copied().unwrap_or(0), id.as_str()));

        for id in sorted {
            let col = depth.get(id.as_str()).copied().unwrap_or(0);
            let row = col_counts[col];
            col_counts[col] += 1;

            let status = self.statuses.get(id.as_str()).copied().unwrap_or(Status::Todo);
            nodes.push(LayoutNode {
                id: id.clone(),
                col,
                row,
                status,
                on_critical_path: self.critical_path.contains(id.as_str()),
            });
        }

        self.node_ids = nodes.iter().map(|n| n.id.clone()).collect();
        self.layout_nodes = nodes;
        if self.selected >= self.node_ids.len() {
            self.selected = 0;
        }
    }

    /// Render the ASCII graph into cached lines.
    #[allow(clippy::needless_range_loop)]
    fn render_graph(&mut self, _tasks: &[Task], dag: &TaskDag) {
        // Build a position map: id -> (col, row).
        let pos_map: HashMap<&str, (usize, usize)> = self
            .layout_nodes
            .iter()
            .map(|n| (n.id.as_str(), (n.col, n.row)))
            .collect();

        // Compute column widths (max label length + padding).
        let max_col = self.layout_nodes.iter().map(|n| n.col).max().unwrap_or(0);
        let col_widths: Vec<usize> = (0..=max_col)
            .map(|c| {
                self.layout_nodes
                    .iter()
                    .filter(|n| n.col == c)
                    .map(|n| short_label(&n.id).len() + 6) // "[x] label" + padding
                    .max()
                    .unwrap_or(10)
                    .max(10)
            })
            .collect();

        // Compute x-offsets for each column.
        let mut col_x: Vec<usize> = Vec::with_capacity(max_col + 1);
        let mut x = 2;
        for w in &col_widths {
            col_x.push(x);
            x += w + 4; // gap between columns for arrows
        }

        // Compute max rows per column.
        let max_rows = self
            .layout_nodes
            .iter()
            .map(|n| n.row)
            .max()
            .unwrap_or(0)
            + 1;

        // Row height = 2 lines per node (node line + spacing).
        let total_height = max_rows * 2 + 1;
        let total_width = x + 4;

        // Initialize a grid of spans.
        let mut grid: Vec<Vec<char>> = vec![vec![' '; total_width]; total_height];

        // Place nodes.
        let selected_id = self.node_ids.get(self.selected).cloned().unwrap_or_default();

        // We build Line<'static> directly instead of using the grid for coloring.
        let mut output_lines: Vec<Line<'static>> = Vec::new();

        // First pass: draw edges as characters on the grid.
        for node in &self.layout_nodes {
            let (nc, nr) = (node.col, node.row);
            let node_y = nr * 2;
            let dependents = dag.dependents(&node.id);
            for dep_id in &dependents {
                if let Some(&(dc, dr)) = pos_map.get(dep_id.as_str()) {
                    let dep_y = dr * 2;
                    let dep_x_start = col_x[dc];
                    let arrow_start_x = col_x[nc] + col_widths[nc] + 1;

                    // Horizontal line from source node to midpoint.
                    if dc > nc {
                        let mid_x = col_x[dc] - 2;
                        for cx in arrow_start_x..mid_x {
                            if cx < total_width && node_y < total_height {
                                grid[node_y][cx] = '-';
                            }
                        }
                        // Vertical connector if rows differ.
                        if dep_y != node_y {
                            let (y_start, y_end) = if dep_y > node_y {
                                (node_y, dep_y)
                            } else {
                                (dep_y, node_y)
                            };
                            if mid_x < total_width {
                                for y in y_start..=y_end {
                                    if y < total_height {
                                        grid[y][mid_x] = '|';
                                    }
                                }
                            }
                        }
                        // Horizontal line from midpoint to target.
                        let mid_x = col_x[dc] - 2;
                        for cx in mid_x..dep_x_start {
                            if cx < total_width && dep_y < total_height {
                                grid[dep_y][cx] = '-';
                            }
                        }
                        // Arrow head.
                        if dep_x_start > 0 && dep_y < total_height && (dep_x_start - 1) < total_width {
                            grid[dep_y][dep_x_start - 1] = '>';
                        }
                    }
                }
            }
        }

        // Second pass: build colored lines with nodes overlaid.
        for y in 0..total_height {
            let mut spans: Vec<Span<'static>> = Vec::new();

            // Check if any node is on this row.
            let nodes_on_row: Vec<&LayoutNode> = self
                .layout_nodes
                .iter()
                .filter(|n| n.row * 2 == y)
                .collect();

            if nodes_on_row.is_empty() {
                // Just edge characters.
                let line_str: String = grid[y].iter().collect();
                let trimmed = line_str.trim_end().to_string();
                if !trimmed.is_empty() {
                    spans.push(Span::styled(trimmed, Style::default().fg(Color::DarkGray)));
                }
            } else {
                // Build the line char by char, inserting colored nodes.
                let mut cursor = 0;

                for node in &nodes_on_row {
                    let nx = col_x[node.col];
                    // Edges before this node.
                    if cursor < nx {
                        let edge_part: String = grid[y][cursor..nx].iter().collect();
                        spans.push(Span::styled(edge_part, Style::default().fg(Color::DarkGray)));
                    }

                    let label = short_label(&node.id);
                    let icon = status_icon_short(node.status);
                    let node_str = format!("{icon} {label}");
                    let node_len = node_str.len();

                    let is_selected = node.id == selected_id;
                    let fg = node_color(node.status);
                    let mut style = Style::default().fg(fg);
                    if node.on_critical_path {
                        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    if is_selected {
                        style = style.bg(Color::DarkGray).add_modifier(Modifier::REVERSED);
                    }

                    spans.push(Span::styled(node_str, style));
                    cursor = nx + node_len;

                    // Pad to column width.
                    let col_end = nx + col_widths[node.col];
                    if cursor < col_end {
                        let pad: String = " ".repeat(col_end - cursor);
                        spans.push(Span::raw(pad));
                        cursor = col_end;
                    }
                }

                // Remaining edge characters.
                if cursor < total_width {
                    let rest: String = grid[y][cursor..].iter().collect();
                    let trimmed = rest.trim_end().to_string();
                    if !trimmed.is_empty() {
                        spans.push(Span::styled(trimmed, Style::default().fg(Color::DarkGray)));
                    }
                }
            }

            output_lines.push(Line::from(spans));
        }

        // Add legend at the bottom.
        output_lines.push(Line::from(""));
        output_lines.push(Line::from(vec![
            Span::styled("  Legend: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("* done  ", Style::default().fg(Color::Green)),
            Span::styled("~ running  ", Style::default().fg(Color::Yellow)),
            Span::styled("X failed  ", Style::default().fg(Color::Red)),
            Span::styled("o todo  ", Style::default().fg(Color::DarkGray)),
            Span::styled("underline = critical path", Style::default().add_modifier(Modifier::UNDERLINED)),
        ]));

        self.lines = output_lines;
    }
}

fn status_icon_short(status: Status) -> &'static str {
    match status {
        Status::Done => "*",
        Status::InProgress => "~",
        Status::Failed | Status::UpstreamFailed => "X",
        Status::Blocked => "!",
        Status::Skipped => "-",
        Status::UpForRetry => "?",
        Status::Todo => "o",
    }
}

fn node_color(status: Status) -> Color {
    match status {
        Status::Done => Color::Green,
        Status::InProgress => Color::Yellow,
        Status::Failed => Color::Red,
        Status::UpstreamFailed => Color::LightRed,
        Status::Blocked => Color::Magenta,
        Status::Skipped => Color::DarkGray,
        Status::UpForRetry => Color::LightYellow,
        Status::Todo => Color::Gray,
    }
}

/// Shorten a task ID for display (e.g. "fn-2-rewrite.14" -> ".14").
fn short_label(id: &str) -> String {
    if let Some(dot_pos) = id.rfind('.') {
        id[dot_pos..].to_string()
    } else {
        id.to_string()
    }
}

impl Component for DagTab {
    fn handle_key_event(&mut self, key: KeyEvent, _tx: &ActionSender) -> Result<bool> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.node_ids.is_empty() {
                    self.selected = (self.selected + 1).min(self.node_ids.len() - 1);
                }
                // Re-render to update selection highlight.
                // (We can't call render_graph here without tasks/dag,
                //  so we toggle the selected state in the cached lines.)
                Ok(true)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                Ok(true)
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.scroll_x = self.scroll_x.saturating_sub(4);
                Ok(true)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.scroll_x = self.scroll_x.saturating_add(4);
                Ok(true)
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                // Zoom not implemented yet; scroll faster.
                self.scroll_y = self.scroll_y.saturating_sub(3);
                Ok(true)
            }
            KeyCode::Char('-') => {
                self.scroll_y = self.scroll_y.saturating_add(3);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn update(&mut self, _action: &Action) -> Result<()> {
        Ok(())
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(format!(" DAG ({} nodes) ", self.node_ids.len()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green));

        if !self.loaded || self.lines.is_empty() {
            let empty_msg = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No dependency graph available",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Task dependencies will be visualized here",
                    Style::default().fg(Color::DarkGray),
                )),
            ];
            let paragraph = Paragraph::new(empty_msg).block(block);
            frame.render_widget(paragraph, area);
            return;
        }

        // Selected node info line at top.
        let info_line = if let Some(node) = self.layout_nodes.get(self.selected) {
            Line::from(vec![
                Span::styled(
                    format!("  Selected: {} ", node.id),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("[{}]", node.status),
                    Style::default().fg(node_color(node.status)),
                ),
                if node.on_critical_path {
                    Span::styled(" (critical path)", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
                } else {
                    Span::raw("")
                },
            ])
        } else {
            Line::from("")
        };

        let mut all_lines = vec![info_line, Line::from("")];
        all_lines.extend(self.lines.clone());

        let paragraph = Paragraph::new(all_lines)
            .block(block)
            .scroll((self.scroll_y, self.scroll_x));
        frame.render_widget(paragraph, area);
    }

    fn keybindings(&self) -> Vec<(&str, &str)> {
        vec![
            ("j/k", "select"),
            ("h/l", "pan"),
            ("arrows", "navigate"),
        ]
    }
}
