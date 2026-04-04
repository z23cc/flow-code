//! Component trait for TUI widgets.
//!
//! Each tab and sub-widget implements this trait. The pattern follows
//! Ratatui's recommended TEA architecture:
//!   1. `handle_key_event` - convert key input into Actions
//!   2. `update` - process Actions and mutate state
//!   3. `render` - draw the component (immutable borrow)

use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::Frame;

use crate::action::{Action, ActionSender};

/// A renderable, interactive TUI component.
pub trait Component {
    /// Handle a key event, optionally emitting Actions via the sender.
    ///
    /// Return `true` if the event was consumed (prevents propagation).
    fn handle_key_event(&mut self, key: KeyEvent, tx: &ActionSender) -> Result<bool> {
        let _ = (key, tx);
        Ok(false)
    }

    /// Process an action and update internal state.
    fn update(&mut self, action: &Action) -> Result<()> {
        let _ = action;
        Ok(())
    }

    /// Render the component into the given area.
    fn render(&self, frame: &mut Frame, area: ratatui::layout::Rect);

    /// Return context-sensitive keybinding hints for the status bar.
    fn keybindings(&self) -> Vec<(&str, &str)> {
        vec![]
    }
}
