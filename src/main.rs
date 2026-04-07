use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::task;

mod alerts;
mod app;
mod collector;
mod config;
mod conn_collector;
mod connections;
mod processes;
mod spy_collector;
mod ui;

use app::{AppState, ConnectionEntry, SelectedTab, SpyEntry, UiState};
use collector::run_data_collection;
use conn_collector::run_connection_collection;
use config::load_config;
use spy_collector::run_spy_collection;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = load_config();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = Arc::new(RwLock::new(AppState {
        refresh_interval_ms: cfg.refresh_interval_ms,
        thresholds: cfg.thresholds,
        ..AppState::default()
    }));

    let state_iface = Arc::clone(&state);
    let refresh_ms = cfg.refresh_interval_ms;
    task::spawn(async move {
        run_data_collection(state_iface, refresh_ms).await;
    });

    let state_conn = Arc::clone(&state);
    let conn_ms = cfg.connection_refresh_ms;
    task::spawn(async move {
        run_connection_collection(state_conn, conn_ms).await;
    });

    let state_spy = Arc::clone(&state);
    let spy_ms = cfg.connection_refresh_ms;
    task::spawn(async move {
        run_spy_collection(state_spy, spy_ms).await;
    });

    let mut ui = UiState::default();

    loop {
        // Draw — ui is &mut so stateful tables can update their scroll offset
        {
            let app_state = state.read().unwrap();
            terminal.draw(|f| ui::draw(f, &app_state, &mut ui))?;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // ── Filter-mode input ────────────────────────────────────────
                if ui.filter_active {
                    match key.code {
                        KeyCode::Esc => {
                            ui.filter_active = false;
                            ui.filter.clear();
                            ui.reset_selection();
                        }
                        KeyCode::Enter => {
                            ui.filter_active = false;
                        }
                        KeyCode::Backspace => {
                            ui.filter.pop();
                            ui.reset_selection();
                        }
                        KeyCode::Char(c) => {
                            ui.filter.push(c);
                            ui.reset_selection();
                        }
                        _ => {}
                    }
                    continue;
                }

                // ── Normal key handling ──────────────────────────────────────
                match key.code {
                    KeyCode::Char('q') => break,

                    // Tab switching
                    KeyCode::Char('1') => {
                        ui.selected_tab = SelectedTab::Interfaces;
                        ui.detail_open = false;
                    }
                    KeyCode::Char('2') => {
                        ui.selected_tab = SelectedTab::Connections;
                        ui.detail_open = false;
                    }
                    KeyCode::Char('3') => {
                        ui.selected_tab = SelectedTab::Processes;
                        ui.detail_open = false;
                    }
                    KeyCode::Char('4') => {
                        ui.selected_tab = SelectedTab::Spy;
                        ui.detail_open = false;
                    }
                    KeyCode::Tab => {
                        ui.selected_tab = match ui.selected_tab {
                            SelectedTab::Interfaces  => SelectedTab::Connections,
                            SelectedTab::Connections => SelectedTab::Processes,
                            SelectedTab::Processes   => SelectedTab::Spy,
                            SelectedTab::Spy         => SelectedTab::Interfaces,
                        };
                        ui.detail_open = false;
                    }

                    // Row navigation
                    KeyCode::Up => {
                        ui.select_prev();
                    }
                    KeyCode::Down => {
                        let max = filtered_max(&state, &ui);
                        ui.select_next(max);
                    }
                    KeyCode::PageUp => {
                        ui.select_prev_page(10);
                    }
                    KeyCode::PageDown => {
                        let max = filtered_max(&state, &ui);
                        ui.select_next_page(max, 10);
                    }

                    // Detail panel (all tabs)
                    KeyCode::Enter => {
                        ui.detail_open = !ui.detail_open;
                    }
                    KeyCode::Esc => {
                        if ui.detail_open {
                            ui.detail_open = false;
                        } else {
                            ui.filter.clear();
                            ui.reset_selection();
                        }
                    }

                    // Filter mode
                    KeyCode::Char('/') => {
                        ui.filter_active = true;
                        ui.filter.clear();
                        ui.reset_selection();
                    }

                    // Pause / resume
                    KeyCode::Char('p') => {
                        let mut s = state.write().unwrap();
                        s.paused = !s.paused;
                    }

                    // Adjust refresh speed
                    KeyCode::Char('+') | KeyCode::Char('=') => {
                        let mut s = state.write().unwrap();
                        s.refresh_interval_ms =
                            (s.refresh_interval_ms.saturating_sub(100)).max(100);
                    }
                    KeyCode::Char('-') => {
                        let mut s = state.write().unwrap();
                        s.refresh_interval_ms = (s.refresh_interval_ms + 100).min(5000);
                    }

                    // ── Spy: pin connection(s) ───────────────────────────────
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        let to_pin: Vec<ConnectionEntry> = {
                            let s = state.read().unwrap();
                            let filter = ui.filter.to_lowercase();
                            match ui.selected_tab {
                                SelectedTab::Connections => {
                                    // Pin the selected connection
                                    let visible: Vec<&ConnectionEntry> = s
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
                                    let idx = ui
                                        .table_state_connections
                                        .selected()
                                        .unwrap_or(0);
                                    visible.get(idx).map(|c| vec![(*c).clone()]).unwrap_or_default()
                                }
                                SelectedTab::Processes => {
                                    // Pin ALL connections of the selected process
                                    use std::collections::HashMap;
                                    let proc_names: Vec<String> = {
                                        let mut map: HashMap<String, ()> = HashMap::new();
                                        for c in &s.connections {
                                            if filter.is_empty()
                                                || c.process_name.to_lowercase().contains(&filter)
                                            {
                                                map.insert(c.process_name.clone(), ());
                                            }
                                        }
                                        let mut v: Vec<String> = map.into_keys().collect();
                                        // Sort by conn count descending to match display order.
                                        let mut counts: HashMap<String, usize> = HashMap::new();
                                        for c in &s.connections {
                                            *counts.entry(c.process_name.clone()).or_insert(0) += 1;
                                        }
                                        v.sort_by(|a, b| {
                                            counts.get(b).unwrap_or(&0)
                                                .cmp(counts.get(a).unwrap_or(&0))
                                        });
                                        v
                                    };
                                    let idx = ui
                                        .table_state_processes
                                        .selected()
                                        .unwrap_or(0);
                                    if let Some(proc_name) = proc_names.get(idx) {
                                        s.connections
                                            .iter()
                                            .filter(|c| &c.process_name == proc_name)
                                            .cloned()
                                            .collect()
                                    } else {
                                        vec![]
                                    }
                                }
                                _ => vec![],
                            }
                        };

                        if !to_pin.is_empty() {
                            // Compute the new selection index before touching state.
                            let new_spy_idx = {
                                let s = state.read().unwrap();
                                // After insertion the last entry will be at:
                                // existing count + number of newly pinned entries - 1
                                let new_entries = to_pin
                                    .iter()
                                    .filter(|c| {
                                        !s.spy_entries.iter().any(|e| e.conn.key() == c.key())
                                    })
                                    .count();
                                (s.spy_entries.len() + new_entries).saturating_sub(1)
                            };
                            // Now acquire the write lock (no read lock alive).
                            {
                                let mut s = state.write().unwrap();
                                for conn in to_pin {
                                    let key = conn.key();
                                    // Don't add duplicates
                                    if !s.spy_entries.iter().any(|e| e.conn.key() == key) {
                                        s.spy_entries.push(SpyEntry::new(conn));
                                    }
                                }
                            } // write lock released here
                            // Switch to Spy tab so the user sees the result.
                            ui.selected_tab = SelectedTab::Spy;
                            ui.detail_open = false;
                            ui.table_state_spy.select(Some(new_spy_idx));
                        }
                    }

                    // ── Spy: unpin selected entry ────────────────────────────
                    KeyCode::Delete => {
                        if ui.selected_tab == SelectedTab::Spy {
                            let mut s = state.write().unwrap();
                            if !s.spy_entries.is_empty() {
                                let idx = ui.table_state_spy.selected().unwrap_or(0);
                                if idx < s.spy_entries.len() {
                                    s.spy_entries.remove(idx);
                                }
                                // Clamp selection after removal
                                let new_max = s.spy_entries.len().saturating_sub(1);
                                ui.table_state_spy.select(Some(idx.min(new_max)));
                            }
                        }
                    }

                    _ => {}
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

/// Return the index of the last visible row for the active tab, respecting the
/// current filter. Used to clamp Down / PageDown navigation.
fn filtered_max(state: &std::sync::Arc<std::sync::RwLock<AppState>>, ui: &UiState) -> usize {
    let s = state.read().unwrap();
    let f = ui.filter.to_lowercase();
    match ui.selected_tab {
        SelectedTab::Interfaces => s
            .interfaces
            .iter()
            .filter(|i| f.is_empty() || i.name.to_lowercase().contains(&f))
            .count()
            .saturating_sub(1),
        SelectedTab::Connections => s
            .connections
            .iter()
            .filter(|c| {
                f.is_empty()
                    || c.process_name.to_lowercase().contains(&f)
                    || c.remote_addr.contains(&f)
                    || c.local_addr.contains(&f)
                    || c.state.to_lowercase().contains(&f)
            })
            .count()
            .saturating_sub(1),
        SelectedTab::Processes => {
            let mut names = std::collections::HashSet::new();
            for c in &s.connections {
                if f.is_empty() || c.process_name.to_lowercase().contains(&f) {
                    names.insert(c.process_name.clone());
                }
            }
            names.len().saturating_sub(1)
        }
        SelectedTab::Spy => s.spy_entries.len().saturating_sub(1),
    }
}
