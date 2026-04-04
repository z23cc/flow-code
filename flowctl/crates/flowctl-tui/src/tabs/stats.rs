//! Stats tab - sparklines, bar charts, gauges for task/epic metrics.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Bar, BarChart, BarGroup, Block, Borders, Gauge, Paragraph, Sparkline,
};
use ratatui::Frame;

use flowctl_db::metrics::{Summary, TokenBreakdown, WeeklyTrend};

use crate::action::{Action, ActionSender};
use crate::component::Component;

/// Stats data loaded from the database.
#[derive(Debug, Default)]
pub struct StatsData {
    pub summary: Option<Summary>,
    pub weekly_trends: Vec<WeeklyTrend>,
    pub token_breakdown: Vec<TokenBreakdown>,
}

pub struct StatsTab {
    data: StatsData,
    loaded: bool,
}

impl StatsTab {
    pub fn new() -> Self {
        Self {
            data: StatsData::default(),
            loaded: false,
        }
    }

    /// Load stats data from pre-queried results.
    pub fn load_stats(&mut self, data: StatsData) {
        self.data = data;
        self.loaded = true;
    }

    fn render_summary_gauge(&self, frame: &mut Frame, area: Rect) {
        let Some(summary) = &self.data.summary else {
            let block = Block::default()
                .title(" Progress ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));
            frame.render_widget(block, area);
            return;
        };

        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

        // Task completion gauge.
        let task_ratio = if summary.total_tasks > 0 {
            summary.done_tasks as f64 / summary.total_tasks as f64
        } else {
            0.0
        };
        let task_label = format!(
            "Tasks: {}/{} done, {} running, {} blocked",
            summary.done_tasks, summary.total_tasks,
            summary.in_progress_tasks, summary.blocked_tasks,
        );
        let task_gauge = Gauge::default()
            .block(
                Block::default()
                    .title(" Task Progress ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green)),
            )
            .gauge_style(
                Style::default()
                    .fg(Color::Green)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .ratio(task_ratio)
            .label(task_label);
        frame.render_widget(task_gauge, chunks[0]);

        // Epic completion gauge.
        let epic_ratio = if summary.total_epics > 0 {
            (summary.total_epics - summary.open_epics) as f64 / summary.total_epics as f64
        } else {
            0.0
        };
        let epic_label = format!(
            "Epics: {}/{} done, {} open",
            summary.total_epics - summary.open_epics,
            summary.total_epics,
            summary.open_epics,
        );
        let epic_gauge = Gauge::default()
            .block(
                Block::default()
                    .title(" Epic Progress ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .gauge_style(
                Style::default()
                    .fg(Color::Cyan)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )
            .ratio(epic_ratio)
            .label(epic_label);
        frame.render_widget(epic_gauge, chunks[1]);

        // Summary stats text.
        let stats_lines = vec![
            Line::from(vec![
                Span::styled("  Events: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(format!("{}", summary.total_events)),
                Span::styled("    Tokens: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(format_tokens(summary.total_tokens)),
                Span::styled("    Cost: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(format!("${:.4}", summary.total_cost_usd)),
            ]),
        ];
        let stats_para = Paragraph::new(stats_lines);
        frame.render_widget(stats_para, chunks[2]);
    }

    fn render_throughput_sparkline(&self, frame: &mut Frame, area: Rect) {
        if self.data.weekly_trends.is_empty() {
            let block = Block::default()
                .title(" Throughput (weekly) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));
            let empty = Paragraph::new(Span::styled(
                "  No trend data",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ))
            .block(block);
            frame.render_widget(empty, area);
            return;
        }

        let data: Vec<u64> = self
            .data
            .weekly_trends
            .iter()
            .map(|w| w.tasks_completed as u64)
            .collect();

        let sparkline = Sparkline::default()
            .block(
                Block::default()
                    .title(" Throughput (tasks/week) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .data(&data)
            .style(Style::default().fg(Color::Green));
        frame.render_widget(sparkline, area);
    }

    fn render_duration_barchart(&self, frame: &mut Frame, area: Rect) {
        if self.data.weekly_trends.is_empty() {
            let block = Block::default()
                .title(" Activity (weekly) ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));
            let empty = Paragraph::new(Span::styled(
                "  No activity data",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ))
            .block(block);
            frame.render_widget(empty, area);
            return;
        }

        let bars: Vec<Bar> = self
            .data
            .weekly_trends
            .iter()
            .map(|w| {
                let label = if w.week.len() > 6 {
                    w.week[5..].to_string() // "W03" from "2025-W03"
                } else {
                    w.week.clone()
                };
                Bar::default()
                    .value(w.tasks_completed as u64)
                    .label(Line::from(label))
                    .style(Style::default().fg(Color::Green))
            })
            .collect();

        // Also show failed as a second set overlay info.
        let barchart = BarChart::default()
            .block(
                Block::default()
                    .title(" Completed Tasks (weekly) ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Magenta)),
            )
            .data(BarGroup::default().bars(&bars))
            .bar_width(5)
            .bar_gap(1)
            .bar_style(Style::default().fg(Color::Green))
            .value_style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD));

        frame.render_widget(barchart, area);
    }

    fn render_token_usage(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" Token Usage ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Magenta));

        if self.data.token_breakdown.is_empty() {
            let empty = Paragraph::new(Span::styled(
                "  No token data",
                Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
            ))
            .block(block);
            frame.render_widget(empty, area);
            return;
        }

        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    format!("{:<15} {:<12} {:>10} {:>10} {:>10}",
                        "Epic", "Model", "Input", "Output", "Cost"),
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        for tb in &self.data.token_breakdown {
            let epic_short = if tb.epic_id.len() > 14 {
                format!("{}...", &tb.epic_id[..11])
            } else {
                tb.epic_id.clone()
            };
            let model_short = if tb.model.len() > 11 {
                format!("{}...", &tb.model[..8])
            } else {
                tb.model.clone()
            };
            lines.push(Line::from(vec![
                Span::raw(format!(
                    "{:<15} {:<12} {:>10} {:>10} {:>10}",
                    epic_short,
                    model_short,
                    format_tokens(tb.input_tokens),
                    format_tokens(tb.output_tokens),
                    format!("${:.4}", tb.estimated_cost),
                )),
            ]));
        }

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
    }
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

impl Component for StatsTab {
    fn handle_key_event(&mut self, key: KeyEvent, _tx: &ActionSender) -> Result<bool> {
        match key.code {
            KeyCode::Char('r') => {
                // Refresh placeholder -- data loading is external.
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn update(&mut self, _action: &Action) -> Result<()> {
        Ok(())
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.loaded {
            let block = Block::default()
                .title(" Stats ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Magenta));

            let empty_msg = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No statistics available",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Completion rates, velocity, and burndown will show here",
                    Style::default().fg(Color::DarkGray),
                )),
            ];

            let paragraph = Paragraph::new(empty_msg).block(block);
            frame.render_widget(paragraph, area);
            return;
        }

        // Layout: top row (gauges + sparkline) | bottom row (barchart + tokens)
        let rows = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(area);

        let top_cols = Layout::horizontal([
            Constraint::Percentage(55),
            Constraint::Percentage(45),
        ])
        .split(rows[0]);

        let bottom_cols = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ])
        .split(rows[1]);

        self.render_summary_gauge(frame, top_cols[0]);
        self.render_throughput_sparkline(frame, top_cols[1]);
        self.render_duration_barchart(frame, bottom_cols[0]);
        self.render_token_usage(frame, bottom_cols[1]);
    }

    fn keybindings(&self) -> Vec<(&str, &str)> {
        vec![("r", "refresh")]
    }
}
