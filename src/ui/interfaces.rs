use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::Span,
    widgets::{
        Axis, Block, Borders, Cell, Chart, Dataset, GraphType, Paragraph, Row, Sparkline, Table,
    },
    Frame,
};

use crate::alerts::ThresholdConfig;
use crate::app::{AppState, InterfaceStats, UiState};

fn bw_color(kb: f64, threshold: Option<f64>) -> Color {
    let limit = threshold.unwrap_or(500.0);
    let ratio = kb / limit;
    if ratio >= 1.0 {
        Color::Red
    } else if ratio >= 0.6 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn threshold_for<'a>(name: &str, thresholds: &'a [ThresholdConfig]) -> Option<&'a ThresholdConfig> {
    thresholds
        .iter()
        .find(|t| t.interface == name || t.interface == "*")
}

pub fn draw(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    if ui.detail_open {
        let idx = ui.table_state_interfaces.selected().unwrap_or(0);
        if let Some(iface) = state.interfaces.get(idx) {
            draw_detail(f, area, iface);
            return;
        }
    }
    draw_table(f, area, state, ui);
}

fn draw_table(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &mut UiState) {
    let filter = ui.filter.to_lowercase();
    let interfaces: Vec<&InterfaceStats> = state
        .interfaces
        .iter()
        .filter(|i| filter.is_empty() || i.name.to_lowercase().contains(&filter))
        .collect();

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(area);

    let header = Row::new(vec![
        "Interface",
        "Type",
        "In KB/s",
        "Out KB/s",
        "Pkts In",
        "Pkts Out",
        "Err In",
        "Err Out",
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .height(1);

    let rows: Vec<Row> = interfaces
        .iter()
        .map(|iface| {
            let th = threshold_for(&iface.name, &state.thresholds);
            let in_color = bw_color(iface.speed_in, th.and_then(|t| t.inbound_kb));
            let out_color = bw_color(iface.speed_out, th.and_then(|t| t.outbound_kb));
            Row::new(vec![
                Cell::from(iface.name.clone()),
                Cell::from(iface.if_type.clone()),
                Cell::from(format!("{:.1}", iface.speed_in)).style(Style::default().fg(in_color)),
                Cell::from(format!("{:.1}", iface.speed_out)).style(Style::default().fg(out_color)),
                Cell::from(format!("{}", iface.packets_in)),
                Cell::from(format!("{}", iface.packets_out)),
                Cell::from(format!("{}", iface.errors_in)),
                Cell::from(format!("{}", iface.errors_out)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(10),
            Constraint::Percentage(12),
            Constraint::Percentage(12),
            Constraint::Percentage(13),
            Constraint::Percentage(13),
            Constraint::Percentage(8),
            Constraint::Percentage(7),
        ],
    )
    .header(header)
    .highlight_style(Style::default().bg(Color::DarkGray))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("Interfaces — Enter for detail"),
    );

    f.render_stateful_widget(table, chunks[0], &mut ui.table_state_interfaces);

    // ── Sparklines ───────────────────────────────────────────────────────────
    if interfaces.is_empty() {
        return;
    }

    let n = interfaces.len();
    let available_h = chunks[1].height.saturating_sub(2);
    let row_height = (available_h / n.max(1) as u16).max(2);

    let spark_constraints: Vec<Constraint> = interfaces
        .iter()
        .map(|_| Constraint::Length(row_height))
        .collect();

    let spark_outer = Rect {
        x: chunks[1].x,
        y: chunks[1].y + 1,
        width: chunks[1].width,
        height: chunks[1].height.saturating_sub(2),
    };

    let spark_areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints(spark_constraints)
        .split(spark_outer);

    for (idx, iface) in interfaces.iter().enumerate() {
        if idx >= spark_areas.len() {
            break;
        }
        let data_in: Vec<u64> = iface.history_in.iter().map(|v| *v as u64).collect();
        let spark = Sparkline::default()
            .block(Block::default().title(Span::styled(
                format!("{} ↓", iface.name),
                Style::default().fg(Color::Cyan),
            )))
            .data(&data_in)
            .style(Style::default().fg(Color::Cyan));
        f.render_widget(spark, spark_areas[idx]);
    }

    let border = Block::default()
        .borders(Borders::ALL)
        .title("Inbound Sparklines (60s)");
    f.render_widget(border, chunks[1]);
}

fn draw_detail(f: &mut Frame<'_>, area: Rect, iface: &InterfaceStats) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(10), Constraint::Min(0)])
        .split(area);

    let info = format!(
        "Name:        {}\nDescription: {}\nType:        {}\nStatus:      {}\n\
         In:          {:.2} KB/s  (total: {} bytes)\nOut:         {:.2} KB/s  (total: {} bytes)\n\
         Packets In:  {}     Packets Out: {}\nErrors In:   {}     Errors Out:  {}\n\
         [Esc] or [Enter] to go back",
        iface.name,
        iface.description,
        iface.if_type,
        iface.status,
        iface.speed_in,
        iface.bytes_in_total,
        iface.speed_out,
        iface.bytes_out_total,
        iface.packets_in,
        iface.packets_out,
        iface.errors_in,
        iface.errors_out,
    );
    let para = Paragraph::new(info).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("Detail: {}", iface.name)),
    );
    f.render_widget(para, chunks[0]);

    if iface.history_in.is_empty() {
        return;
    }

    let in_data: Vec<(f64, f64)> = iface
        .history_in
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();
    let out_data: Vec<(f64, f64)> = iface
        .history_out
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, *v))
        .collect();

    let max_y = in_data
        .iter()
        .chain(out_data.iter())
        .map(|(_, v)| *v)
        .fold(0.0f64, f64::max)
        .max(1.0);

    let datasets = vec![
        Dataset::default()
            .name("In KB/s")
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Cyan))
            .data(&in_data),
        Dataset::default()
            .name("Out KB/s")
            .marker(symbols::Marker::Dot)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(Color::Yellow))
            .data(&out_data),
    ];

    let x_len = (iface.history_in.len().max(iface.history_out.len()) as f64 - 1.0).max(1.0);

    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Bandwidth History (60s)"),
        )
        .x_axis(
            Axis::default()
                .title("Time →")
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, x_len])
                .labels(vec![Span::raw("60s ago"), Span::raw("now")]),
        )
        .y_axis(
            Axis::default()
                .title("KB/s")
                .style(Style::default().fg(Color::DarkGray))
                .bounds([0.0, max_y * 1.1])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw(format!("{:.0}", max_y / 2.0)),
                    Span::raw(format!("{:.0}", max_y)),
                ]),
        );

    f.render_widget(chart, chunks[1]);
}
