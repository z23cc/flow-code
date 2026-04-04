//! Actions for inter-component communication via mpsc channels.
//!
//! Follows the Ratatui TEA (The Elm Architecture) pattern where
//! components emit actions instead of mutating state directly.

use tokio::sync::mpsc;

use flowctl_scheduler::TimestampedEvent;

/// Actions that flow between components through the event loop.
#[derive(Debug, Clone)]
pub enum Action {
    /// Periodic tick (250ms) for background polling.
    Tick,
    /// Render the UI (30fps timer).
    Render,
    /// Switch to a specific tab by index.
    SwitchTab(usize),
    /// Cycle to the next tab.
    NextTab,
    /// Cycle to the previous tab.
    PrevTab,
    /// Quit the application.
    Quit,
    /// Resize the terminal (cols, rows).
    Resize(u16, u16),
    /// A key event that wasn't handled at the app level.
    Key(crossterm::event::KeyEvent),
    /// A live event from the daemon's event bus.
    FlowEvent(TimestampedEvent),
}

/// Convenience type for the action sender half.
pub type ActionSender = mpsc::UnboundedSender<Action>;

/// Convenience type for the action receiver half.
pub type ActionReceiver = mpsc::UnboundedReceiver<Action>;
