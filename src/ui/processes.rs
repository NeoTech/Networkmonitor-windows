use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};
use std::collections::HashMap;

use crate::app::{AppState, ConnectionEntry, UiState};

// ── Data model ────────────────────────────────────────────────────────────────

struct ProcessSummary {
    pid: u32,
    name: String,
    conn_count: usize,
    tcp_established: usize,
    udp_count: usize,
}

/// Build the filtered, sorted list of unique processes from the connection list.
fn build_process_list<'a>(state: &'a AppState, filter: &str) -> Vec<ProcessSummary> {
    let mut map: HashMap<String, ProcessSummary> = HashMap::new();
    for conn in &state.connections {
        if !filter.is_empty() && !conn.process_name.to_lowercase().contains(filter) {
            continue;
        }
        let entry = map
            .entry(conn.process_name.clone())
            .or_insert_with(|| ProcessSummary {
                pid: conn.pid,
                name: conn.process_name.clone(),
                conn_count: 0,
                tcp_established: 0,
                udp_count: 0,
            });
        entry.conn_count += 1;
        if conn.protocol == "TCP" && conn.state == "ESTABLISHED" {
            entry.tcp_established += 1;
        }
        if conn.protocol == "UDP" {
            entry.udp_count += 1;
        }
    }

    let mut processes: Vec<ProcessSummary> = map.into_values().collect();
    processes.sort_by(|a, b| b.conn_count.cmp(&a.conn_count));
    processes
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    let filter = ui.filter.to_lowercase();
    let processes = build_process_list(state, &filter);

    if ui.detail_open {
        let idx = ui.table_state_processes.selected().unwrap_or(0);
        if let Some(proc) = processes.get(idx) {
            // Collect all connections belonging to this process
            let proc_conns: Vec<&ConnectionEntry> = state
                .connections
                .iter()
                .filter(|c| c.process_name == proc.name)
                .collect();
            draw_split(f, area, &processes, idx, &proc_conns, ui);
            return;
        }
    }

    draw_table(f, area, &processes, ui);
}

// ── Table-only view ───────────────────────────────────────────────────────────

fn draw_table(f: &mut Frame<'_>, area: Rect, processes: &[ProcessSummary], ui: &mut UiState) {
    let table = build_process_table(
        processes,
        format!(
            "Processes ({} with active connections) — Enter for detail",
            processes.len()
        ),
    );
    f.render_stateful_widget(table, area, &mut ui.table_state_processes);
}

// ── Split view (process table top + connections bottom) ───────────────────────

fn draw_split(
    f: &mut Frame<'_>,
    area: Rect,
    processes: &[ProcessSummary],
    selected_idx: usize,
    proc_conns: &[&ConnectionEntry],
    ui: &mut UiState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    // Top: process table (still navigable)
    let table = build_process_table(processes, format!("Processes ({} shown)", processes.len()));
    f.render_stateful_widget(table, chunks[0], &mut ui.table_state_processes);

    // Bottom: connections for the selected process
    draw_process_detail(f, chunks[1], &processes[selected_idx], proc_conns);
}

// ── Widget builders ───────────────────────────────────────────────────────────

fn build_process_table(processes: &[ProcessSummary], title: String) -> Table<'static> {
    let header = Row::new(vec![
        "PID",
        "Process",
        "Total Conns",
        "TCP Established",
        "UDP",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = processes
        .iter()
        .map(|p| {
            Row::new(vec![
                Cell::from(format!("{}", p.pid)),
                Cell::from(p.name.clone()),
                Cell::from(format!("{}", p.conn_count)),
                Cell::from(format!("{}", p.tcp_established))
                    .style(Style::default().fg(Color::Green)),
                Cell::from(format!("{}", p.udp_count)).style(Style::default().fg(Color::Cyan)),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Percentage(40),
            Constraint::Percentage(15),
            Constraint::Percentage(20),
            Constraint::Percentage(15),
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(Block::default().borders(Borders::ALL).title(title))
}

fn draw_process_detail(
    f: &mut Frame<'_>,
    area: Rect,
    proc: &ProcessSummary,
    conns: &[&ConnectionEntry],
) {
    // Split the detail area: summary line on top, connection table below
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(3)])
        .split(area);

    // Summary paragraph
    let summary = format!(
        "PID: {}   Total: {}   TCP Established: {}   UDP: {}   [Esc] to close",
        proc.pid, proc.conn_count, proc.tcp_established, proc.udp_count
    );
    let para = Paragraph::new(summary).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Process: {}", proc.name)),
    );
    f.render_widget(para, chunks[0]);

    // Connection sub-table (read-only, no TableState needed)
    let header = Row::new(vec!["Proto", "Local", "Remote", "State"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1);

    let rows: Vec<Row> = conns
        .iter()
        .map(|c| {
            let state_color = match c.state.as_str() {
                "ESTABLISHED" => Color::Green,
                "TIME_WAIT" | "CLOSE_WAIT" => Color::Yellow,
                "LISTEN" => Color::Cyan,
                _ => Color::White,
            };
            Row::new(vec![
                Cell::from(c.protocol.clone()),
                Cell::from(format!("{}:{}", c.local_addr, c.local_port)),
                Cell::from(format!("{}:{}", c.remote_addr, c.remote_port)),
                Cell::from(c.state.clone()).style(Style::default().fg(state_color)),
            ])
        })
        .collect();

    let conn_table = Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(30),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
        ],
    )
    .header(header)
    .block(Block::default().borders(Borders::ALL).title(format!(
        "Connections for {} ({})",
        proc.name,
        conns.len()
    )));

    f.render_widget(conn_table, chunks[1]);
}
