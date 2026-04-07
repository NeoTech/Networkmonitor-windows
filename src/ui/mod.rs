pub mod connections;
pub mod interfaces;
pub mod processes;
pub mod spy;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Tabs},
    Frame,
};

use crate::app::{AppState, SelectedTab, UiState};

/// Top-level draw function — called every render tick.
pub fn draw(f: &mut Frame<'_>, state: &AppState, ui: &mut UiState) {
    let area = f.size();

    // Main layout: tab bar | content | status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // tab bar
            Constraint::Min(0),    // content
            Constraint::Length(3), // status bar
        ])
        .split(area);

    draw_tabs(f, chunks[0], ui);

    match ui.selected_tab {
        SelectedTab::Interfaces => interfaces::draw(f, chunks[1], state, ui),
        SelectedTab::Connections => connections::draw(f, chunks[1], state, ui),
        SelectedTab::Processes => processes::draw(f, chunks[1], state, ui),
        SelectedTab::Spy => spy::draw(f, chunks[1], state, ui),
    }

    draw_status_bar(f, chunks[2], state, ui);
}

fn draw_tabs(f: &mut Frame<'_>, area: Rect, ui: &UiState) {
    let titles: Vec<Span> = SelectedTab::titles()
        .iter()
        .map(|t| Span::raw(*t))
        .collect();

    let tabs = Tabs::new(titles)
        .select(ui.selected_tab.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Network Monitor — press q to quit"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, area);
}

fn draw_status_bar(f: &mut Frame<'_>, area: Rect, state: &AppState, ui: &UiState) {
    let alert_count = state.alerts.len();
    let paused = if state.paused { "PAUSED  " } else { "" };

    let filter_hint = if ui.filter_active {
        format!("  Filter: {}█", ui.filter)
    } else if !ui.filter.is_empty() {
        format!("  Filter: {}", ui.filter)
    } else {
        String::new()
    };

    let spy_hint = match ui.selected_tab {
        SelectedTab::Connections | SelectedTab::Processes => "  S: spy",
        SelectedTab::Spy => "  Del: unpin",
        _ => "",
    };

    let keys = format!(
        "Tab/1-4: switch  ↑↓/PgUp/PgDn: select  Enter: detail  /: filter  p: pause  +/-: speed{}  q: quit",
        spy_hint
    );
    let status_text = format!(
        " {}{}Alerts: {}  {}",
        paused, filter_hint, alert_count, keys
    );

    let style = if alert_count > 0 {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let para = ratatui::widgets::Paragraph::new(status_text)
        .style(style)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(para, area);
}
