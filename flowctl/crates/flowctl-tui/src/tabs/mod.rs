//! Tab components for the TUI dashboard.
//!
//! Each tab is a Component that renders into the main content area.

mod tasks;
mod dag;
mod logs;
mod stats;

pub use tasks::TasksTab;
pub use dag::DagTab;
pub use logs::LogsTab;
pub use stats::{StatsData, StatsTab};

/// The four dashboard tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tasks = 0,
    Dag = 1,
    Logs = 2,
    Stats = 3,
}

impl Tab {
    pub const ALL: [Tab; 4] = [Tab::Tasks, Tab::Dag, Tab::Logs, Tab::Stats];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Tasks => "Tasks",
            Tab::Dag => "DAG",
            Tab::Logs => "Logs",
            Tab::Stats => "Stats",
        }
    }

    pub fn from_index(i: usize) -> Tab {
        Tab::ALL[i % Tab::ALL.len()]
    }
}
