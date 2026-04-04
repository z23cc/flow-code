//! Logs tab - split-pane log viewer with level filtering and search.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use flowctl_db::EventRow;

use crate::action::{Action, ActionSender};
use crate::component::Component;

/// Log severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

impl LogLevel {
    fn from_event_type(event_type: &str) -> Self {
        if event_type.contains("failed") || event_type.contains("error") {
            LogLevel::Error
        } else if event_type.contains("blocked") || event_type.contains("retry") {
            LogLevel::Warn
        } else if event_type.contains("debug") || event_type.contains("heartbeat") {
            LogLevel::Debug
        } else {
            LogLevel::Info
        }
    }

    fn label(self) -> &'static str {
        match self {
            LogLevel::Error => "ERR",
            LogLevel::Warn => "WRN",
            LogLevel::Info => "INF",
            LogLevel::Debug => "DBG",
        }
    }

    fn color(self) -> Color {
        match self {
            LogLevel::Error => Color::Red,
            LogLevel::Warn => Color::Yellow,
            LogLevel::Info => Color::Cyan,
            LogLevel::Debug => Color::DarkGray,
        }
    }
}

/// A processed log entry ready for display.
#[derive(Debug, Clone)]
struct LogEntry {
    id: i64,
    timestamp: String,
    level: LogLevel,
    event_type: String,
    epic_id: String,
    task_id: Option<String>,
    actor: Option<String>,
    payload: Option<String>,
}

impl LogEntry {
    fn from_event_row(row: &EventRow) -> Self {
        Self {
            id: row.id,
            timestamp: row.timestamp.clone(),
            level: LogLevel::from_event_type(&row.event_type),
            event_type: row.event_type.clone(),
            epic_id: row.epic_id.clone(),
            task_id: row.task_id.clone(),
            actor: row.actor.clone(),
            payload: row.payload.clone(),
        }
    }

    fn summary_line(&self) -> String {
        let ts = if self.timestamp.len() > 19 {
            &self.timestamp[11..19] // HH:MM:SS
        } else {
            &self.timestamp
        };
        let task_part = self.task_id.as_deref().unwrap_or("");
        format!("{} [{}] {} {}", ts, self.level.label(), self.event_type, task_part)
    }
}

/// Focus pane in split view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pane {
    List,
    Detail,
}

pub struct LogsTab {
    /// All log entries.
    entries: Vec<LogEntry>,
    /// Indices into `entries` after filtering.
    filtered: Vec<usize>,
    /// List widget state.
    list_state: ListState,
    /// Level filter toggles.
    show_error: bool,
    show_warn: bool,
    show_info: bool,
    show_debug: bool,
    /// Search mode active.
    search_active: bool,
    /// Search query.
    search_query: String,
    /// Auto-scroll to bottom.
    auto_scroll: bool,
    /// Active pane.
    active_pane: Pane,
    /// Whether data has been loaded.
    loaded: bool,
}

impl Default for LogsTab {
    fn default() -> Self {
        Self::new()
    }
}

impl LogsTab {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            filtered: Vec::new(),
            list_state: ListState::default(),
            show_error: true,
            show_warn: true,
            show_info: true,
            show_debug: false,
            search_active: false,
            search_query: String::new(),
            auto_scroll: true,
            active_pane: Pane::List,
            loaded: false,
        }
    }

    /// Load log entries from EventRow data.
    pub fn load_events(&mut self, events: Vec<EventRow>) {
        self.entries = events.iter().map(LogEntry::from_event_row).collect();
        self.loaded = true;
        self.refilter();
        if self.auto_scroll && !self.filtered.is_empty() {
            self.list_state.select(Some(self.filtered.len() - 1));
        } else if self.list_state.selected().is_none() && !self.filtered.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn refilter(&mut self) {
        let query_lower = self.search_query.to_lowercase();
        self.filtered = (0..self.entries.len())
            .filter(|&i| {
                let entry = &self.entries[i];
                // Level filter.
                let level_ok = match entry.level {
                    LogLevel::Error => self.show_error,
                    LogLevel::Warn => self.show_warn,
                    LogLevel::Info => self.show_info,
                    LogLevel::Debug => self.show_debug,
                };
                if !level_ok {
                    return false;
                }
                // Search filter.
                if !query_lower.is_empty() {
                    let haystack = format!(
                        "{} {} {} {}",
                        entry.event_type,
                        entry.task_id.as_deref().unwrap_or(""),
                        entry.actor.as_deref().unwrap_or(""),
                        entry.payload.as_deref().unwrap_or(""),
                    )
                    .to_lowercase();
                    if !haystack.contains(&query_lower) {
                        return false;
                    }
                }
                true
            })
            .collect();

        // Clamp selection.
        if let Some(sel) = self.list_state.selected() {
            if sel >= self.filtered.len() {
                self.list_state.select(if self.filtered.is_empty() {
                    None
                } else {
                    Some(self.filtered.len() - 1)
                });
            }
        }
    }

    fn selected_entry(&self) -> Option<&LogEntry> {
        self.list_state
            .selected()
            .and_then(|i| self.filtered.get(i))
            .map(|&idx| &self.entries[idx])
    }

    fn move_selection(&mut self, delta: isize) {
        if self.filtered.is_empty() {
            return;
        }
        self.auto_scroll = false;
        let current = self.list_state.selected().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, self.filtered.len() as isize - 1) as usize;
        self.list_state.select(Some(next));
    }

    fn filter_status_line(&self) -> Line<'static> {
        let toggle = |on: bool, label: &str, color: Color| -> Vec<Span<'static>> {
            let style = if on {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            vec![
                Span::styled(format!("[{}]", if on { "x" } else { " " }), style),
                Span::styled(format!("{label} "), style),
            ]
        };

        let mut spans = vec![Span::styled(" Filter: ", Style::default().fg(Color::White))];
        spans.extend(toggle(self.show_error, "ERR", Color::Red));
        spans.extend(toggle(self.show_warn, "WRN", Color::Yellow));
        spans.extend(toggle(self.show_info, "INF", Color::Cyan));
        spans.extend(toggle(self.show_debug, "DBG", Color::DarkGray));
        spans.push(Span::styled(
            format!("  ({}/{})", self.filtered.len(), self.entries.len()),
            Style::default().fg(Color::White),
        ));
        if self.auto_scroll {
            spans.push(Span::styled(" AUTO", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)));
        }
        Line::from(spans)
    }
}

impl Component for LogsTab {
    fn handle_key_event(&mut self, key: KeyEvent, _tx: &ActionSender) -> Result<bool> {
        // Search mode.
        if self.search_active {
            match key.code {
                KeyCode::Esc => {
                    self.search_active = false;
                    self.search_query.clear();
                    self.refilter();
                }
                KeyCode::Enter => {
                    self.search_active = false;
                }
                KeyCode::Backspace => {
                    self.search_query.pop();
                    self.refilter();
                }
                KeyCode::Char(c) => {
                    self.search_query.push(c);
                    self.refilter();
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
                    self.list_state.select(Some(self.filtered.len() - 1));
                    self.auto_scroll = true;
                }
                Ok(true)
            }
            KeyCode::Char('g') => {
                if !self.filtered.is_empty() {
                    self.list_state.select(Some(0));
                    self.auto_scroll = false;
                }
                Ok(true)
            }
            KeyCode::Char('/') => {
                self.search_active = true;
                self.search_query.clear();
                Ok(true)
            }
            KeyCode::Char('e') => {
                self.show_error = !self.show_error;
                self.refilter();
                Ok(true)
            }
            KeyCode::Char('w') => {
                self.show_warn = !self.show_warn;
                self.refilter();
                Ok(true)
            }
            KeyCode::Char('i') => {
                self.show_info = !self.show_info;
                self.refilter();
                Ok(true)
            }
            KeyCode::Char('d') => {
                self.show_debug = !self.show_debug;
                self.refilter();
                Ok(true)
            }
            KeyCode::Char('a') => {
                self.auto_scroll = !self.auto_scroll;
                if self.auto_scroll && !self.filtered.is_empty() {
                    self.list_state.select(Some(self.filtered.len() - 1));
                }
                Ok(true)
            }
            KeyCode::Tab => {
                self.active_pane = match self.active_pane {
                    Pane::List => Pane::Detail,
                    Pane::Detail => Pane::List,
                };
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn update(&mut self, _action: &Action) -> Result<()> {
        Ok(())
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.loaded || self.entries.is_empty() {
            let block = Block::default()
                .title(" Logs ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow));

            let empty_msg = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No log events",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Events from flowctl operations will appear here",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let paragraph = Paragraph::new(empty_msg).block(block);
            frame.render_widget(paragraph, area);
            return;
        }

        // Layout: filter bar (1) | search bar (0-1) | split pane (rest)
        let search_height = if self.search_active { 1 } else { 0 };
        let chunks = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(search_height),
            Constraint::Min(4),
        ])
        .split(area);

        // Filter status bar.
        let filter_line = self.filter_status_line();
        let filter_bar = Paragraph::new(filter_line)
            .style(Style::default().bg(Color::Black));
        frame.render_widget(filter_bar, chunks[0]);

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

        // Split pane: log list (left 60%) | detail (right 40%).
        let panes = Layout::horizontal([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(chunks[2]);

        // Log list.
        let list_border_color = if self.active_pane == Pane::List {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .map(|&idx| {
                let entry = &self.entries[idx];
                let line = Line::from(vec![
                    Span::styled(
                        format!("[{}] ", entry.level.label()),
                        Style::default().fg(entry.level.color()),
                    ),
                    Span::styled(
                        entry.summary_line(),
                        Style::default().fg(Color::White),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" Logs ({}) ", self.filtered.len()))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(list_border_color)),
            )
            .highlight_style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> ");

        let mut state = self.list_state.clone();
        frame.render_stateful_widget(list, panes[0], &mut state);

        // Detail pane.
        let detail_border_color = if self.active_pane == Pane::Detail {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let detail_lines = if let Some(entry) = self.selected_entry() {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("ID:        ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(format!("{}", entry.id)),
                ]),
                Line::from(vec![
                    Span::styled("Time:      ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(entry.timestamp.clone()),
                ]),
                Line::from(vec![
                    Span::styled("Level:     ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::styled(entry.level.label().to_string(), Style::default().fg(entry.level.color())),
                ]),
                Line::from(vec![
                    Span::styled("Type:      ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(entry.event_type.clone()),
                ]),
                Line::from(vec![
                    Span::styled("Epic:      ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(entry.epic_id.clone()),
                ]),
                Line::from(vec![
                    Span::styled("Task:      ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(entry.task_id.clone().unwrap_or_else(|| "-".to_string())),
                ]),
                Line::from(vec![
                    Span::styled("Actor:     ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                    Span::raw(entry.actor.clone().unwrap_or_else(|| "-".to_string())),
                ]),
            ];
            if let Some(payload) = &entry.payload {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Payload:",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                )));
                for pl in payload.lines() {
                    lines.push(Line::from(Span::raw(format!("  {pl}"))));
                }
            }
            lines
        } else {
            vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Select a log entry",
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                )),
            ]
        };

        let detail = Paragraph::new(detail_lines)
            .block(
                Block::default()
                    .title(" Detail ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(detail_border_color)),
            )
            .wrap(Wrap { trim: false });
        frame.render_widget(detail, panes[1]);
    }

    fn keybindings(&self) -> Vec<(&str, &str)> {
        if self.search_active {
            return vec![("Esc", "cancel"), ("Enter", "apply")];
        }
        vec![
            ("j/k", "navigate"),
            ("/", "search"),
            ("e/w/i/d", "filter"),
            ("a", "auto-scroll"),
            ("G/g", "bottom/top"),
        ]
    }
}
