//! Snapshot tests for TUI components using insta.
//!
//! Each test renders a component into a ratatui Buffer and snapshots
//! the resulting text output.

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::Terminal;

use flowctl_core::state_machine::Status;
use flowctl_core::types::{Domain, Task};
use flowctl_db::EventRow;
use flowctl_db::metrics::{Summary, TokenBreakdown, WeeklyTrend};

use flowctl_tui::app::App;
use flowctl_tui::tabs::{LogsTab, StatsTab, TasksTab};
use flowctl_tui::widgets::toast::{Toast, ToastLevel, ToastStack};
use flowctl_tui::component::Component;

fn render_to_string(width: u16, height: u16, render_fn: impl FnOnce(&mut ratatui::Frame, Rect)) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            render_fn(frame, frame.area());
        })
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    buffer_to_string(&buffer)
}

fn buffer_to_string(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut output = String::new();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    // Trim trailing whitespace from each line for cleaner snapshots.
    output
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn make_test_tasks() -> Vec<Task> {
    let now = chrono::Utc::now();
    vec![
        Task {
            schema_version: 1,
            id: "fn-1-test.1".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Setup database".to_string(),
            status: Status::Done,
            priority: Some(1),
            domain: Domain::Backend,
            depends_on: vec![],
            files: vec!["src/db.rs".to_string()],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: now,
            updated_at: now,
        },
        Task {
            schema_version: 1,
            id: "fn-1-test.2".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Build API".to_string(),
            status: Status::InProgress,
            priority: Some(2),
            domain: Domain::Backend,
            depends_on: vec!["fn-1-test.1".to_string()],
            files: vec!["src/api.rs".to_string()],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: now,
            updated_at: now,
        },
        Task {
            schema_version: 1,
            id: "fn-1-test.3".to_string(),
            epic: "fn-1-test".to_string(),
            title: "Write tests".to_string(),
            status: Status::Todo,
            priority: Some(3),
            domain: Domain::Testing,
            depends_on: vec!["fn-1-test.2".to_string()],
            files: vec![],
            r#impl: None,
            review: None,
            sync: None,
            file_path: None,
            created_at: now,
            updated_at: now,
        },
    ]
}

fn make_test_events() -> Vec<EventRow> {
    vec![
        EventRow {
            id: 1,
            timestamp: "2025-01-15T10:30:00.000Z".to_string(),
            epic_id: "fn-1-test".to_string(),
            task_id: Some("fn-1-test.1".to_string()),
            event_type: "task_started".to_string(),
            actor: Some("worker-1".to_string()),
            payload: None,
            session_id: Some("sess-001".to_string()),
        },
        EventRow {
            id: 2,
            timestamp: "2025-01-15T10:35:00.000Z".to_string(),
            epic_id: "fn-1-test".to_string(),
            task_id: Some("fn-1-test.1".to_string()),
            event_type: "task_completed".to_string(),
            actor: Some("worker-1".to_string()),
            payload: Some("{\"summary\": \"done\"}".to_string()),
            session_id: Some("sess-001".to_string()),
        },
        EventRow {
            id: 3,
            timestamp: "2025-01-15T10:36:00.000Z".to_string(),
            epic_id: "fn-1-test".to_string(),
            task_id: Some("fn-1-test.2".to_string()),
            event_type: "task_failed".to_string(),
            actor: Some("worker-2".to_string()),
            payload: Some("build error".to_string()),
            session_id: Some("sess-002".to_string()),
        },
        EventRow {
            id: 4,
            timestamp: "2025-01-15T10:37:00.000Z".to_string(),
            epic_id: "fn-1-test".to_string(),
            task_id: Some("fn-1-test.2".to_string()),
            event_type: "task_blocked".to_string(),
            actor: None,
            payload: Some("waiting on review".to_string()),
            session_id: None,
        },
    ]
}

// ── Tasks Tab ──────────────────────────────────────────────────

#[test]
fn test_tasks_tab_empty() {
    let tab = TasksTab::new();
    let output = render_to_string(80, 20, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("tasks_tab_empty", output);
}

#[test]
fn test_tasks_tab_with_data() {
    let mut tab = TasksTab::new();
    tab.load_tasks(make_test_tasks());
    let output = render_to_string(80, 20, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("tasks_tab_with_data", output);
}

// ── Logs Tab ───────────────────────────────────────────────────

#[test]
fn test_logs_tab_empty() {
    let tab = LogsTab::new();
    let output = render_to_string(80, 20, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("logs_tab_empty", output);
}

#[test]
fn test_logs_tab_with_events() {
    let mut tab = LogsTab::new();
    tab.load_events(make_test_events());
    let output = render_to_string(100, 20, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("logs_tab_with_events", output);
}

// ── Stats Tab ──────────────────────────────────────────────────

#[test]
fn test_stats_tab_empty() {
    let tab = StatsTab::new();
    let output = render_to_string(80, 20, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("stats_tab_empty", output);
}

#[test]
fn test_stats_tab_with_data() {
    use flowctl_tui::tabs::StatsData;

    let mut tab = StatsTab::new();
    tab.load_stats(StatsData {
        summary: Some(Summary {
            total_epics: 3,
            open_epics: 1,
            total_tasks: 15,
            done_tasks: 10,
            in_progress_tasks: 3,
            blocked_tasks: 1,
            total_events: 42,
            total_tokens: 1_500_000,
            total_cost_usd: 2.35,
        }),
        weekly_trends: vec![
            WeeklyTrend { week: "2025-W01".to_string(), tasks_started: 5, tasks_completed: 3, tasks_failed: 0 },
            WeeklyTrend { week: "2025-W02".to_string(), tasks_started: 8, tasks_completed: 7, tasks_failed: 1 },
            WeeklyTrend { week: "2025-W03".to_string(), tasks_started: 4, tasks_completed: 4, tasks_failed: 0 },
        ],
        token_breakdown: vec![
            TokenBreakdown {
                epic_id: "fn-1-test".to_string(),
                model: "claude-sonnet".to_string(),
                input_tokens: 800_000,
                output_tokens: 200_000,
                cache_read: 100_000,
                cache_write: 50_000,
                estimated_cost: 1.50,
            },
        ],
    });
    let output = render_to_string(100, 24, |frame, area| {
        tab.render(frame, area);
    });
    insta::assert_snapshot!("stats_tab_with_data", output);
}

// ── Toast ──────────────────────────────────────────────────────

#[test]
fn test_toast_stack_render() {
    let mut stack = ToastStack::new();
    stack.push(Toast::new(ToastLevel::Success, "Task completed!"));
    stack.push(Toast::new(ToastLevel::Error, "Build failed"));
    stack.push(Toast::new(ToastLevel::Warning, "Low disk space"));

    let output = render_to_string(80, 20, |frame, area| {
        stack.render(frame, area);
    });
    insta::assert_snapshot!("toast_stack", output);
}

// ── Full App ───────────────────────────────────────────────────

#[test]
fn test_app_default_render() {
    let app = App::new();
    let output = render_to_string(100, 30, |frame, area| {
        app.render(frame, area);
    });
    insta::assert_snapshot!("app_default", output);
}
