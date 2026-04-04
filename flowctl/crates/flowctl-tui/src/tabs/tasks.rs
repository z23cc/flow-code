//! Tasks tab - sortable table with fuzzy search and detail popup.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::Frame;

use flowctl_core::state_machine::Status;
use flowctl_core::types::Task;

use crate::action::{Action, ActionSender};
use crate::component::Component;

/// Column to sort by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortColumn {
    Status,
    Priority,
    Domain,
    Title,
}

impl SortColumn {
    fn next(self) -> Self {
        match self {
            SortColumn::Status => SortColumn::Priority,
            SortColumn::Priority => SortColumn::Domain,
            SortColumn::Domain => SortColumn::Title,
            SortColumn::Title => SortColumn::Status,
        }
    }
}

pub struct TasksTab {
    /// All loaded tasks.
    tasks: Vec<Task>,
    /// Indices into `tasks` after filtering+sorting.
    filtered: Vec<usize>,
    /// Table widget state (selected row).
    table_state: TableState,
    /// Current sort column.
    sort_col: SortColumn,
    /// Sort ascending.
    sort_asc: bool,
    /// Whether search mode is active.
    search_active: bool,
    /// Search query string.
    search_query: String,
    /// Whether the detail popup is open.
    detail_open: bool,
    /// Whether data has been loaded at least once.
    loaded: bool,
}

impl Default for TasksTab {
    fn default() -> Self {
        Self::new()
    }
}

impl TasksTab {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            filtered: Vec::new(),
            table_state: TableState::default(),
            sort_col: SortColumn::Status,
            sort_asc: true,
            search_active: false,
            search_query: String::new(),
            detail_open: false,
            loaded: false,
        }
    }

    /// Load tasks from the database connection.
    pub fn load_tasks(&mut self, tasks: Vec<Task>) {
        self.tasks = tasks;
        self.loaded = true;
        self.refilter_and_sort();
        // Preserve selection if possible.
        if !self.filtered.is_empty() && self.table_state.selected().is_none() {
            self.table_state.select(Some(0));
        }
    }

    fn refilter_and_sort(&mut self) {
        // Filter by search query (fuzzy: substring match, case-insensitive).
        let query_lower = self.search_query.to_lowercase();
        self.filtered = (0..self.tasks.len())
            .filter(|&i| {
                if query_lower.is_empty() {
                    return true;
                }
                let t = &self.tasks[i];
                let haystack = format!(
                    "{} {} {} {}",
                    t.id, t.title, t.status, t.domain
                )
                .to_lowercase();
                // Simple fuzzy: all query chars must appear in order.
                let mut hay_iter = haystack.chars();
                query_lower.chars().all(|qc| hay_iter.any(|hc| hc == qc))
            })
            .collect();

        // Sort.
        let tasks = &self.tasks;
        let col = self.sort_col;
        let asc = self.sort_asc;
        self.filtered.sort_by(|&a, &b| {
            let ta = &tasks[a];
            let tb = &tasks[b];
            let ord = match col {
                SortColumn::Status => status_rank(ta.status).cmp(&status_rank(tb.status)),
                SortColumn::Priority => ta.sort_priority().cmp(&tb.sort_priority()),
                SortColumn::Domain => ta.domain.to_string().cmp(&tb.domain.to_string()),
                SortColumn::Title => ta.title.cmp(&tb.title),
            };
            if asc { ord } else { ord.reverse() }
        });

        // Clamp selection.
        if let Some(sel) = self.table_state.selected() {
            if sel >= self.filtered.len() {
                self.table_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(self.filtered.len() - 1)
                });
            }
        }
    }

    fn selected_task(&self) -> Option<&Task> {
        self.table_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .map(|&idx| &self.tasks[idx])
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        let current = self.table_state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.filtered.len() as isize - 1) as usize;
        self.table_state.select(Some(next));
    }

    fn progress_stats(&self) -> (usize, usize, usize, usize) {
        let total = self.tasks.len();
        let done = self.tasks.iter().filter(|t| t.status == Status::Done).count();
        let in_progress = self.tasks.iter().filter(|t| t.status == Status::InProgress).count();
        let failed = self
            .tasks
            .iter()
            .filter(|t| t.status.is_failed())
            .count();
        (total, done, in_progress, failed)
    }
}

fn status_icon(status: Status) -> &'static str {
    match status {
        Status::Todo => "[ ]",
        Status::InProgress => "[~]",
        Status::Done => "[x]",
        Status::Blocked => "[!]",
        Status::Skipped => "[-]",
        Status::Failed => "[X]",
        Status::UpForRetry => "[?]",
        Status::UpstreamFailed => "[^]",
    }
}

fn status_color(status: Status) -> Color {
    match status {
        Status::Todo => Color::DarkGray,
        Status::InProgress => Color::Yellow,
        Status::Done => Color::Green,
        Status::Blocked => Color::Magenta,
        Status::Skipped => Color::DarkGray,
        Status::Failed => Color::Red,
        Status::UpForRetry => Color::LightYellow,
        Status::UpstreamFailed => Color::LightRed,
    }
}

/// Numeric rank for sorting statuses in a useful order.
fn status_rank(status: Status) -> u8 {
    match status {
        Status::InProgress => 0,
        Status::Failed => 1,
        Status::UpForRetry => 2,
        Status::Blocked => 3,
        Status::UpstreamFailed => 4,
        Status::Todo => 5,
        Status::Done => 6,
        Status::Skipped => 7,
    }
}

impl Component for TasksTab {
    fn handle_key_event(&mut self, key: KeyEvent, _tx: &ActionSender) -> Result<bool> {
        // Search mode captures all input.
        if self.search_active {
            match key.code {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.refilter_and_sort();
                }
                KeyCode::Enter => {
                    self.search_active = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.refilter_and_sort();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.refilter_and_sort();
                }
                _ => {}
            }
            return Ok(true);
        }

        // Detail popup mode.
        if self.detail_open {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.detail_open = false;
                }
                _ => {}
            }
            return Ok(true);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.move_selection(1);
                Ok(true)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.move_selection(-1);
                Ok(true)
            }
            KeyCode::Char('G') => {
                if !self.filtered.is_empty() {
                    self.table_state.select(Some(self.filtered.len() - 1));
                }
                Ok(true)
            }
            KeyCode::Char('g') => {
                if !self.filtered.is_empty() {
                    self.table_state.select(Some(0));
                }
                Ok(true)
            }
            KeyCode::Enter => {
                if self.selected_task().is_some() {
                    self.detail_open = true;
                }
                Ok(true)
            }
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                Ok(true)
            }
            KeyCode::Char('s') => {
                // Cycle sort column.
                let old_col = self.sort_col;
                self.sort_col = self.sort_col.next();
                if self.sort_col == old_col {
                    self.sort_asc = !self.sort_asc;
                } else {
                    self.sort_asc = true;
                }
                self.refilter_and_sort();
                Ok(true)
            }
            KeyCode::Char('S') => {
                // Toggle sort direction.
                self.sort_asc = !self.sort_asc;
                self.refilter_and_sort();
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn update(&mut self, action: &Action) -> Result<()> {
        if let Action::Tick = action {
            // Data loading happens externally via load_tasks();
            // Tick is a no-op until we wire up DB polling.
        }
        Ok(())
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.loaded || self.tasks.is_empty() {
            render_empty(frame, area);
            return;
        }

        // Layout: progress bar (2 rows) | search bar (1 row if active) | table (rest).
        let search_height = if self.search_active { 1 } else { 0 };
        let chunks = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(search_height),
            Constraint::Min(4),
        ])
        .split(area);

        // Progress bar.
        let (total, done, in_progress, failed) = self.progress_stats();
        let ratio = if total > 0 {
            done as f64 / total as f64
        } else {
            0.0
        };
        let label = format!(
            "{done}/{total} done, {in_progress} running, {failed} failed"
        );
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(
                Style::default()
                    .fg(Color::Green)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .ratio(ratio)
            .label(label);
        frame.render_widget(gauge, chunks[0]);

        // Search bar.
        if self.search_active {
            let search_line = Line::from(vec![
                Span::styled("/", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::raw(&self.search_query),
                Span::styled("_", Style::default().fg(Color::Yellow).add_modifier(Modifier::SLOW_BLINK)),
            ]);
            let search_bar = Paragraph::new(search_line)
                .style(Style::default().bg(Color::Black));
            frame.render_widget(search_bar, chunks[1]);
        }

        // Table header.
        let sort_indicator = |col: SortColumn| -> &str {
            if col == self.sort_col {
                if self.sort_asc { " ^" } else { " v" }
            } else {
                ""
            }
        };
        let header = Row::new(vec![
            Cell::from(format!("Status{}", sort_indicator(SortColumn::Status))),
            Cell::from(format!("Pri{}", sort_indicator(SortColumn::Priority))),
            Cell::from(format!("Domain{}", sort_indicator(SortColumn::Domain))),
            Cell::from("ID"),
            Cell::from(format!("Title{}", sort_indicator(SortColumn::Title))),
        ])
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

        let rows: Vec<Row> = self
            .filtered
            .iter()
            .map(|&idx| {
                let t = &self.tasks[idx];
                let sc = status_color(t.status);
                Row::new(vec![
                    Cell::from(status_icon(t.status)).style(Style::default().fg(sc)),
                    Cell::from(format!("{}", t.sort_priority()))
                        .style(Style::default().fg(Color::White)),
                    Cell::from(t.domain.to_string())
                        .style(Style::default().fg(Color::Blue)),
                    Cell::from(t.id.as_str())
                        .style(Style::default().fg(Color::DarkGray)),
                    Cell::from(t.title.as_str())
                        .style(Style::default().fg(Color::White)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(13),
            Constraint::Length(25),
            Constraint::Fill(1),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .title(format!(
                        " Tasks ({}/{}) ",
                        self.filtered.len(),
                        self.tasks.len()
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .row_highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut state = self.table_state.clone();
        frame.render_stateful_widget(table, chunks[2], &mut state);

        // Detail popup.
        if self.detail_open {
            if let Some(task) = self.selected_task() {
                render_detail_popup(frame, area, task);
            }
        }
    }

    fn keybindings(&self) -> Vec<(&str, &str)> {
        if self.search_active {
            return vec![("Esc", "cancel"), ("Enter", "apply")];
        }
        if self.detail_open {
            return vec![("Esc/Enter", "close")];
        }
        vec![
            ("j/k", "navigate"),
            ("Enter", "details"),
            ("/", "search"),
            ("s", "sort"),
            ("S", "reverse"),
        ]
    }
}

fn render_empty(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .title(" Tasks ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let empty_msg = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  No tasks loaded",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "  Run flowctl in a project with .flow/ to see tasks",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(empty_msg).block(block);
    frame.render_widget(paragraph, area);
}

fn render_detail_popup(frame: &mut Frame, area: Rect, task: &Task) {
    // Center a popup covering ~60% of the screen.
    let popup_width = (area.width * 3 / 5).max(40).min(area.width - 4);
    let popup_height = (area.height * 3 / 5).max(12).min(area.height - 4);
    let x = area.x + (area.width - popup_width) / 2;
    let y = area.y + (area.height - popup_height) / 2;
    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let deps = if task.depends_on.is_empty() {
        "none".to_string()
    } else {
        task.depends_on.join(", ")
    };
    let files = if task.files.is_empty() {
        "none".to_string()
    } else {
        task.files.join(", ")
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("ID:       ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(&task.id),
        ]),
        Line::from(vec![
            Span::styled("Title:    ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(&task.title),
        ]),
        Line::from(vec![
            Span::styled("Epic:     ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(&task.epic),
        ]),
        Line::from(vec![
            Span::styled("Status:   ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{} {}", status_icon(task.status), task.status),
                Style::default().fg(status_color(task.status)),
            ),
        ]),
        Line::from(vec![
            Span::styled("Priority: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(format!("{}", task.sort_priority())),
        ]),
        Line::from(vec![
            Span::styled("Domain:   ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(task.domain.to_string()),
        ]),
        Line::from(vec![
            Span::styled("Deps:     ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(deps),
        ]),
        Line::from(vec![
            Span::styled("Files:    ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(files),
        ]),
        Line::from(vec![
            Span::styled("Created:  ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(task.created_at.format("%Y-%m-%d %H:%M").to_string()),
        ]),
    ];

    let block = Block::default()
        .title(" Task Detail ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));

    let paragraph = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup_area);
}
