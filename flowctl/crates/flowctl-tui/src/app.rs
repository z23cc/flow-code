//! Main TUI application with event loop and tab navigation.
//!
//! Architecture: tokio::select! multiplexes three event sources:
//!   - Render timer (33ms / ~30fps)
//!   - Tick timer (250ms for background polling)
//!   - Crossterm keyboard events
//!
//! Daemon auto-detection: on startup, checks whether the flowctl daemon
//! is running. If yes, connects via broadcast channel for live event
//! streaming. If no, falls back to SQLite polling on tick.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, cursor};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Tabs};
use ratatui::Terminal;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::action::{Action, ActionSender};
use crate::component::Component;
use crate::tabs::{self, DagTab, LogsTab, StatsTab, Tab, TasksTab};
use crate::widgets::toast::ToastStack;

/// How the TUI connects to the daemon for event data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataSource {
    /// No daemon running -- poll SQLite on tick timer.
    SqlitePolling,
    /// Daemon detected -- receiving live events via broadcast channel.
    DaemonEvents,
}

const MIN_COLS: u16 = 80;
const MIN_ROWS: u16 = 24;
const RENDER_INTERVAL: Duration = Duration::from_millis(33); // ~30fps
const TICK_INTERVAL: Duration = Duration::from_millis(250);

/// The top-level TUI application.
pub struct App {
    active_tab: usize,
    tasks_tab: TasksTab,
    dag_tab: DagTab,
    logs_tab: LogsTab,
    stats_tab: StatsTab,
    should_quit: bool,
    term_cols: u16,
    term_rows: u16,
    toasts: ToastStack,
    data_source: DataSource,
}

impl App {
    pub fn new() -> Self {
        Self {
            active_tab: 0,
            tasks_tab: TasksTab::new(),
            dag_tab: DagTab::new(),
            logs_tab: LogsTab::new(),
            stats_tab: StatsTab::new(),
            should_quit: false,
            term_cols: 0,
            term_rows: 0,
            toasts: ToastStack::new(),
            data_source: DataSource::SqlitePolling,
        }
    }

    /// Detect whether the daemon is running by checking the socket file.
    ///
    /// If a `.flow/.state/flowctl.sock` exists, we assume the daemon is
    /// reachable and switch to live event mode.
    pub fn detect_daemon(&mut self, flow_dir: Option<&PathBuf>) -> DataSource {
        let socket_exists = flow_dir
            .map(|dir| dir.join(".state").join("flowctl.sock").exists())
            .unwrap_or(false);

        if socket_exists {
            info!("daemon detected, using live event streaming");
            self.data_source = DataSource::DaemonEvents;
        } else {
            debug!("no daemon detected, using SQLite polling");
            self.data_source = DataSource::SqlitePolling;
        }

        self.data_source.clone()
    }

    /// Connect to the daemon's event bus via a broadcast receiver.
    ///
    /// Returns a task handle that forwards events into the action channel.
    /// The caller should hold onto this handle for the lifetime of the app.
    pub fn connect_event_bus(
        &self,
        event_bus: &flowctl_scheduler::EventBus,
        action_tx: ActionSender,
    ) -> tokio::task::JoinHandle<()> {
        let mut rx = event_bus.subscribe();

        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if action_tx.send(Action::FlowEvent(event)).is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(skipped = n, "TUI event receiver lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        debug!("event bus closed");
                        break;
                    }
                }
            }
        })
    }

    /// Get the current data source mode.
    pub fn data_source(&self) -> &DataSource {
        &self.data_source
    }

    /// Run the TUI event loop. Takes ownership of the terminal until quit.
    pub async fn run(&mut self) -> Result<()> {
        // Check minimum terminal size before entering alternate screen.
        let (cols, rows) = terminal::size()?;
        if cols < MIN_COLS || rows < MIN_ROWS {
            bail!(
                "Terminal too small: {}x{} (minimum {}x{})",
                cols,
                rows,
                MIN_COLS,
                MIN_ROWS,
            );
        }
        self.term_cols = cols;
        self.term_rows = rows;

        // Set up terminal.
        terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal).await;

        // Restore terminal (always, even on error).
        terminal::disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;

        result
    }

    async fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel::<Action>();

        let mut render_interval = tokio::time::interval(RENDER_INTERVAL);
        let mut tick_interval = tokio::time::interval(TICK_INTERVAL);

        // Initial render.
        terminal.draw(|frame| self.render(frame, frame.area()))?;

        loop {
            tokio::select! {
                _ = render_interval.tick() => {
                    terminal.draw(|frame| self.render(frame, frame.area()))?;
                }
                _ = tick_interval.tick() => {
                    tx.send(Action::Tick)?;
                }
                // Crossterm events (keyboard, resize) -- polled with zero timeout
                // so we don't block the select.
                _ = tokio::task::yield_now() => {
                    while event::poll(Duration::ZERO)? {
                        match event::read()? {
                            Event::Key(key) => {
                                if !self.handle_key_event(key, &tx)? {
                                    // Forward unhandled keys to active tab.
                                    self.active_component_mut().handle_key_event(key, &tx)?;
                                }
                            }
                            Event::Resize(cols, rows) => {
                                tx.send(Action::Resize(cols, rows))?;
                            }
                            _ => {}
                        }
                    }
                }
            }

            // Drain all pending actions.
            while let Ok(action) = rx.try_recv() {
                self.update(&action)?;
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn active_component_mut(&mut self) -> &mut dyn Component {
        match tabs::Tab::from_index(self.active_tab) {
            Tab::Tasks => &mut self.tasks_tab,
            Tab::Dag => &mut self.dag_tab,
            Tab::Logs => &mut self.logs_tab,
            Tab::Stats => &mut self.stats_tab,
        }
    }

    fn active_component(&self) -> &dyn Component {
        match tabs::Tab::from_index(self.active_tab) {
            Tab::Tasks => &self.tasks_tab,
            Tab::Dag => &self.dag_tab,
            Tab::Logs => &self.logs_tab,
            Tab::Stats => &self.stats_tab,
        }
    }

    /// Access the toast stack for pushing notifications.
    pub fn toasts_mut(&mut self) -> &mut ToastStack {
        &mut self.toasts
    }

    /// Render the tab bar, status bar, and keybinding hints.
    fn render_chrome(&self, frame: &mut ratatui::Frame, area: Rect) -> Rect {
        // Layout: tab bar (3 rows) | content (flex) | status bar (1 row).
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

        // Tab bar.
        let tab_titles: Vec<Line> = Tab::ALL
            .iter()
            .map(|t| Line::from(format!(" {} ", t.title())))
            .collect();

        let title = match self.data_source {
            DataSource::DaemonEvents => " flowctl [live] ",
            DataSource::SqlitePolling => " flowctl ",
        };

        let tabs_widget = Tabs::new(tab_titles)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White)),
            )
            .select(self.active_tab)
            .style(Style::default().fg(Color::DarkGray))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(Span::raw("|"));

        frame.render_widget(tabs_widget, chunks[0]);

        // Status bar with keybinding hints.
        let mut hints: Vec<Span> = vec![
            Span::styled(" 1-4", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":tab  "),
            Span::styled("Tab", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":next  "),
            Span::styled("q", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw(":quit"),
        ];

        // Add tab-specific keybindings.
        let tab_bindings = self.active_component().keybindings();
        if !tab_bindings.is_empty() {
            hints.push(Span::raw("  |  "));
            for (i, (key, desc)) in tab_bindings.iter().enumerate() {
                if i > 0 {
                    hints.push(Span::raw("  "));
                }
                hints.push(Span::styled(
                    *key,
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
                ));
                hints.push(Span::raw(format!(":{desc}")));
            }
        }

        let status_bar = ratatui::widgets::Paragraph::new(Line::from(hints))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
        frame.render_widget(status_bar, chunks[2]);

        // Return the content area.
        chunks[1]
    }
}

impl Component for App {
    fn handle_key_event(&mut self, key: event::KeyEvent, tx: &ActionSender) -> Result<bool> {
        // Global keybindings (Lazygit pattern: app-level first).
        match key.code {
            KeyCode::Char('q') => {
                tx.send(Action::Quit)?;
                Ok(true)
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                tx.send(Action::Quit)?;
                Ok(true)
            }
            KeyCode::Char('1') => {
                tx.send(Action::SwitchTab(0))?;
                Ok(true)
            }
            KeyCode::Char('2') => {
                tx.send(Action::SwitchTab(1))?;
                Ok(true)
            }
            KeyCode::Char('3') => {
                tx.send(Action::SwitchTab(2))?;
                Ok(true)
            }
            KeyCode::Char('4') => {
                tx.send(Action::SwitchTab(3))?;
                Ok(true)
            }
            KeyCode::Tab => {
                tx.send(Action::NextTab)?;
                Ok(true)
            }
            KeyCode::BackTab => {
                tx.send(Action::PrevTab)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn update(&mut self, action: &Action) -> Result<()> {
        match action {
            Action::Quit => {
                self.should_quit = true;
            }
            Action::SwitchTab(idx) => {
                self.active_tab = *idx % Tab::ALL.len();
            }
            Action::NextTab => {
                self.active_tab = (self.active_tab + 1) % Tab::ALL.len();
            }
            Action::PrevTab => {
                self.active_tab = (self.active_tab + Tab::ALL.len() - 1) % Tab::ALL.len();
            }
            Action::Resize(cols, rows) => {
                self.term_cols = *cols;
                self.term_rows = *rows;
            }
            Action::Tick => {
                self.toasts.gc();
            }
            _ => {}
        }

        // Forward to active tab.
        self.active_component_mut().update(action)?;
        Ok(())
    }

    fn render(&self, frame: &mut ratatui::Frame, area: Rect) {
        // Check terminal size.
        if area.width < MIN_COLS || area.height < MIN_ROWS {
            let msg = format!(
                "Terminal too small: {}x{} (need {}x{})",
                area.width, area.height, MIN_COLS, MIN_ROWS,
            );
            let para = ratatui::widgets::Paragraph::new(msg)
                .style(Style::default().fg(Color::Red));
            frame.render_widget(para, area);
            return;
        }

        let content_area = self.render_chrome(frame, area);
        self.active_component().render(frame, content_area);

        // Toast overlay (rendered last, on top of everything).
        self.toasts.render(frame, area);
    }
}
