//! flowctl-tui: Terminal UI dashboard for flowctl.
//!
//! Provides a 4-tab TUI dashboard (Tasks, DAG, Logs, Stats) using
//! ratatui with component architecture and context-sensitive keybindings.

pub mod action;
pub mod app;
pub mod component;
pub mod tabs;
pub mod widgets;
pub mod ws_client;

pub use app::App;
pub use widgets::toast::{Toast, ToastLevel, ToastStack};
