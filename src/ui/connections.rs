use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::app::{AppState, ConnectionEntry, UiState};

pub fn draw(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    let filter = ui.filter.to_lowercase();
    let conns: Vec<&ConnectionEntry> = state
        .connections
        .iter()
        .filter(|c| {
            filter.is_empty()
                || c.process_name.to_lowercase().contains(&filter)
                || c.remote_addr.contains(&filter)
                || c.local_addr.contains(&filter)
                || c.state.to_lowercase().contains(&filter)
        })
        .collect();

    if ui.detail_open {
        let idx = ui.table_state_connections.selected().unwrap_or(0);
        if let Some(conn) = conns.get(idx) {
            draw_split(f, area, &conns, conn, ui);
            return;
        }
    }

    draw_table(f, area, &conns, ui);
}

fn draw_table(f: &mut Frame<'_>, area: Rect, conns: &[&ConnectionEntry], ui: &mut UiState) {
    let table = build_table(
        conns,
        format!("Connections ({} shown) — Enter for detail", conns.len()),
    );
    f.render_stateful_widget(table, area, &mut ui.table_state_connections);
}

fn draw_split(
    f: &mut Frame<'_>,
    area: Rect,
    conns: &[&ConnectionEntry],
    selected: &ConnectionEntry,
    ui: &mut UiState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(area);

    let table = build_table(conns, format!("Connections ({} shown)", conns.len()));
    f.render_stateful_widget(table, chunks[0], &mut ui.table_state_connections);

    draw_detail(f, chunks[1], selected);
}

fn build_table(conns: &[&ConnectionEntry], title: String) -> Table<'static> {
    let header = Row::new(vec!["Proto", "Local", "Remote", "State", "PID", "Process"])
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
                Cell::from(format!("{}", c.pid)),
                Cell::from(c.process_name.clone()),
            ])
        })
        .collect();

    Table::new(
        rows,
        [
            Constraint::Length(5),
            Constraint::Percentage(22),
            Constraint::Percentage(22),
            Constraint::Percentage(14),
            Constraint::Length(7),
            Constraint::Percentage(30),
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(Block::default().borders(Borders::ALL).title(title))
}

fn draw_detail(f: &mut Frame<'_>, area: Rect, c: &ConnectionEntry) {
    let state_color = match c.state.as_str() {
        "ESTABLISHED" => Color::Green,
        "TIME_WAIT" | "CLOSE_WAIT" => Color::Yellow,
        "LISTEN" => Color::Cyan,
        _ => Color::White,
    };

    let text = format!(
        "Protocol : {}\n\
         Local    : {}:{}\n\
         Remote   : {}:{}\n\
         State    : {}\n\
         PID      : {}\n\
         Process  : {}\n\
         \n\
         [Enter] or [Esc] to close",
        c.protocol,
        c.local_addr,
        c.local_port,
        c.remote_addr,
        c.remote_port,
        c.state,
        c.pid,
        c.process_name,
    );

    let para = Paragraph::new(text)
        .style(Style::default().fg(state_color))
        .block(Block::default().borders(Borders::ALL).title(format!(
            "Detail — {} {}:{} → {}:{}",
            c.protocol, c.local_addr, c.local_port, c.remote_addr, c.remote_port
        )));
    f.render_widget(para, area);
}
