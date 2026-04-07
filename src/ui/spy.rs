use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Axis, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Table},
    Frame,
};

use crate::app::{AppState, SpyEntry, UiState};

// ── Public entry point ────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    if state.spy_entries.is_empty() {
        draw_empty(f, area);
        return;
    }

    let idx = ui.table_state_spy.selected().unwrap_or(0);
    let selected = state.spy_entries.get(idx);

    match selected {
        Some(entry) => draw_split(f, area, state, ui, entry),
        None => draw_table_only(f, area, state, ui),
    }
}

// ── Empty state ───────────────────────────────────────────────────────────────

fn draw_empty(f: &mut Frame<'_>, area: Rect) {
    let msg = vec![
        Line::from(""),
        Line::from(Span::styled(
            "No connections pinned for spying.",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Go to the Connections tab (2) or Processes tab (3),",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(Span::styled(
            "select a row and press  S  to start spying on it.",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "On the Processes tab, S will pin ALL connections for that process.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let para = Paragraph::new(msg)
        .alignment(ratatui::layout::Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Spy — Connection Monitor"),
        );
    f.render_widget(para, area);
}

// ── Table-only (no selection with detail) ────────────────────────────────────

fn draw_table_only(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    let table = build_spy_table(&state.spy_entries);
    f.render_stateful_widget(table, area, &mut ui.table_state_spy);
}

// ── Split view: table top + detail bottom ─────────────────────────────────────

fn draw_split(
    f: &mut Frame<'_>,
    area: Rect,
    state: &AppState,
    ui: &mut UiState,
    selected: &SpyEntry,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let table = build_spy_table(&state.spy_entries);
    f.render_stateful_widget(table, chunks[0], &mut ui.table_state_spy);

    draw_detail(f, chunks[1], selected);
}

// ── Spy table widget ──────────────────────────────────────────────────────────

fn build_spy_table(entries: &[SpyEntry]) -> Table<'static> {
    let header = Row::new(vec![
        "Proto", "Local", "Remote", "Process", "In KB/s", "Out KB/s", "RTT (ms)", "Retr", "Status",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = entries
        .iter()
        .map(|e| {
            let status_style = if !e.alive {
                Style::default().fg(Color::Red)
            } else if !e.estats_available && e.conn.protocol == "TCP" {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Green)
            };

            let status = if !e.alive {
                "CLOSED"
            } else if !e.estats_available && e.conn.protocol == "TCP" {
                "no data"
            } else {
                "live"
            };

            let rtt_str = if e.conn.protocol == "TCP" && e.rtt_us > 0 {
                format!("{:.1}", e.rtt_us as f64 / 1000.0)
            } else {
                "—".to_string()
            };

            let retr_str = if e.conn.protocol == "TCP" {
                format!("{}", e.retransmits)
            } else {
                "—".to_string()
            };

            Row::new(vec![
                Cell::from(e.conn.protocol.clone()),
                Cell::from(format!("{}:{}", e.conn.local_addr, e.conn.local_port)),
                Cell::from(format!("{}:{}", e.conn.remote_addr, e.conn.remote_port)),
                Cell::from(e.conn.process_name.clone()),
                Cell::from(format!("{:.1}", e.speed_in)).style(Style::default().fg(Color::Cyan)),
                Cell::from(format!("{:.1}", e.speed_out))
                    .style(Style::default().fg(Color::Magenta)),
                Cell::from(rtt_str),
                Cell::from(retr_str),
                Cell::from(status).style(status_style),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(18),
            Constraint::Percentage(18),
            Constraint::Percentage(20),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Percentage(10),
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(Block::default().borders(Borders::ALL).title(format!(
        "Spy — {} connection(s) pinned  [S to pin, Del to unpin, Enter for detail]",
        entries.len()
    )))
}

// ── Detail panel (sparklines + stats) ────────────────────────────────────────

fn draw_detail(f: &mut Frame<'_>, area: Rect, entry: &SpyEntry) {
    // Split detail into: charts row (left in / right out) | stats paragraph
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(6)])
        .split(area);

    let chart_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[0]);

    draw_sparkline(f, chart_cols[0], &entry.history_in, "In KB/s", Color::Cyan);
    draw_sparkline(
        f,
        chart_cols[1],
        &entry.history_out,
        "Out KB/s",
        Color::Magenta,
    );

    draw_stats(f, rows[1], entry);
}

fn draw_sparkline(
    f: &mut Frame<'_>,
    area: Rect,
    history: &std::collections::VecDeque<f64>,
    label: &str,
    color: Color,
) {
    let data: Vec<(f64, f64)> = history
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as f64, v))
        .collect();

    let max_val = history.iter().cloned().fold(0.0_f64, f64::max).max(1.0);

    let dataset = Dataset::default()
        .name(label)
        .marker(symbols::Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(&data);

    let chart = Chart::new(vec![dataset])
        .block(Block::default().borders(Borders::ALL).title(format!(
            "{} (last {}s)",
            label,
            history.len()
        )))
        .x_axis(
            Axis::default()
                .bounds([0.0, 60.0])
                .style(Style::default().fg(Color::DarkGray)),
        )
        .y_axis(
            Axis::default()
                .bounds([0.0, max_val])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{:.0}", max_val / 2.0)),
                    Span::raw(format!("{:.0}", max_val)),
                ])
                .style(Style::default().fg(Color::DarkGray)),
        );

    f.render_widget(chart, area);
}

fn draw_stats(f: &mut Frame<'_>, area: Rect, entry: &SpyEntry) {
    let rtt_str = if entry.conn.protocol == "TCP" && entry.rtt_us > 0 {
        format!("{:.2} ms", entry.rtt_us as f64 / 1000.0)
    } else {
        "—".to_string()
    };

    let retr_str = if entry.conn.protocol == "TCP" {
        format!("{}", entry.retransmits)
    } else {
        "—".to_string()
    };

    let alive_str = if entry.alive { "yes" } else { "CLOSED" };

    let estats_str = if entry.conn.protocol != "TCP" {
        "N/A (UDP)"
    } else if entry.estats_available {
        "available"
    } else {
        "unavailable (run as admin?)"
    };

    let pinned_secs = entry.pinned_at.elapsed().as_secs();

    let text = format!(
        "Connection : {} {}:{} → {}:{}\n\
         Process    : {} (PID {})\n\
         In         : {:.2} KB/s    Out: {:.2} KB/s\n\
         RTT        : {}    Retransmits: {}\n\
         Status     : {}    ESTATS: {}\n\
         Pinned for : {}m {}s  [Del] to unpin",
        entry.conn.protocol,
        entry.conn.local_addr,
        entry.conn.local_port,
        entry.conn.remote_addr,
        entry.conn.remote_port,
        entry.conn.process_name,
        entry.conn.pid,
        entry.speed_in,
        entry.speed_out,
        rtt_str,
        retr_str,
        alive_str,
        estats_str,
        pinned_secs / 60,
        pinned_secs % 60,
    );

    let style = if !entry.alive {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::White)
    };

    let para =
        Paragraph::new(text)
            .style(style)
            .block(Block::default().borders(Borders::ALL).title(format!(
                "Details — {} → {}:{}",
                entry.conn.process_name, entry.conn.remote_addr, entry.conn.remote_port
            )));
    f.render_widget(para, area);
}
