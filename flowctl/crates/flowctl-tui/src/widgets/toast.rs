//! Toast notification system - bottom-right corner stack with auto-expire.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

/// Toast severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Success,
    Error,
    Warning,
}

impl ToastLevel {
    fn color(self) -> Color {
        match self {
            ToastLevel::Success => Color::Green,
            ToastLevel::Error => Color::Red,
            ToastLevel::Warning => Color::Yellow,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            ToastLevel::Success => "[ok]",
            ToastLevel::Error => "[!!]",
            ToastLevel::Warning => "[!]",
        }
    }
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub level: ToastLevel,
    pub message: String,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl Toast {
    /// Create a new toast with default TTL (4 seconds).
    pub fn new(level: ToastLevel, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            created_at: Instant::now(),
            ttl: Duration::from_secs(4),
        }
    }

    /// Create a toast with custom TTL.
    pub fn with_ttl(level: ToastLevel, message: impl Into<String>, ttl: Duration) -> Self {
        Self {
            level,
            message: message.into(),
            created_at: Instant::now(),
            ttl,
        }
    }

    /// Check if the toast has expired.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

/// A stack of toast notifications rendered in the bottom-right corner.
#[derive(Debug, Default)]
pub struct ToastStack {
    toasts: Vec<Toast>,
}

const MAX_TOASTS: usize = 5;
const TOAST_WIDTH: u16 = 40;
const TOAST_HEIGHT: u16 = 3;

impl ToastStack {
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    /// Push a new toast onto the stack.
    pub fn push(&mut self, toast: Toast) {
        self.toasts.push(toast);
        // Cap the visible stack.
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    /// Remove expired toasts. Call on every tick.
    pub fn gc(&mut self) {
        self.toasts.retain(|t| !t.is_expired());
    }

    /// Whether there are any active toasts.
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// Render the toast stack in the bottom-right corner of the given area.
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if self.toasts.is_empty() {
            return;
        }

        let toast_w = TOAST_WIDTH.min(area.width.saturating_sub(2));
        let max_visible = ((area.height.saturating_sub(2)) / TOAST_HEIGHT) as usize;
        let visible_toasts: Vec<&Toast> = self
            .toasts
            .iter()
            .rev()
            .take(max_visible)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        for (i, toast) in visible_toasts.iter().enumerate() {
            let bottom_offset = (visible_toasts.len() - 1 - i) as u16 * TOAST_HEIGHT;
            let y = area.y + area.height.saturating_sub(TOAST_HEIGHT + bottom_offset + 1);
            let x = area.x + area.width.saturating_sub(toast_w + 1);

            let toast_area = Rect::new(x, y, toast_w, TOAST_HEIGHT);

            frame.render_widget(Clear, toast_area);

            let color = toast.level.color();
            let icon = toast.level.icon();

            let line = Line::from(vec![
                Span::styled(
                    format!("{icon} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    toast.message.clone(),
                    Style::default().fg(Color::White),
                ),
            ]);

            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));

            let paragraph = Paragraph::new(line).block(block).wrap(Wrap { trim: true });
            frame.render_widget(paragraph, toast_area);
        }
    }
}
