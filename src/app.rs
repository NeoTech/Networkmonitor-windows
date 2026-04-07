use crate::alerts::{AlertEvent, ThresholdConfig};
use ratatui::widgets::TableState;
use std::collections::VecDeque;
use std::time::Instant;

// ── Tabs ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedTab {
    Interfaces,
    Connections,
    Processes,
    Spy,
}

impl SelectedTab {
    pub fn titles() -> Vec<&'static str> {
        vec!["1: Interfaces", "2: Connections", "3: Processes", "4: Spy"]
    }
    pub fn index(self) -> usize {
        match self {
            SelectedTab::Interfaces => 0,
            SelectedTab::Connections => 1,
            SelectedTab::Processes => 2,
            SelectedTab::Spy => 3,
        }
    }
}

// ── Interface stats ──────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct InterfaceStats {
    pub name: String,
    pub description: String,
    pub speed_in: f64,  // KB/s
    pub speed_out: f64, // KB/s
    pub bytes_in_total: u64,
    pub bytes_out_total: u64,
    pub packets_in: u64,
    pub packets_out: u64,
    pub errors_in: u64,
    pub errors_out: u64,
    pub status: String,
    pub if_type: String,
    pub history_in: VecDeque<f64>, // last 60 samples
    pub history_out: VecDeque<f64>,
}

impl Default for InterfaceStats {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            speed_in: 0.0,
            speed_out: 0.0,
            bytes_in_total: 0,
            bytes_out_total: 0,
            packets_in: 0,
            packets_out: 0,
            errors_in: 0,
            errors_out: 0,
            status: String::new(),
            if_type: String::new(),
            history_in: VecDeque::with_capacity(60),
            history_out: VecDeque::with_capacity(60),
        }
    }
}

// ── Connection entry ─────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct ConnectionEntry {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
    pub pid: u32,
    pub process_name: String,
}

impl ConnectionEntry {
    /// Returns a stable key string to identify this connection 5-tuple.
    pub fn key(&self) -> String {
        format!(
            "{}|{}:{}|{}:{}",
            self.protocol, self.local_addr, self.local_port, self.remote_addr, self.remote_port
        )
    }
}

// ── Spy entry ────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct SpyEntry {
    /// The connection being monitored (holds the 5-tuple + process info).
    pub conn: ConnectionEntry,

    // Cumulative counters (bytes / packets) — reserved for future use.
    #[allow(dead_code)]
    pub bytes_in_total: u64,
    #[allow(dead_code)]
    pub bytes_out_total: u64,
    #[allow(dead_code)]
    pub pkts_in_total: u64,
    #[allow(dead_code)]
    pub pkts_out_total: u64,

    // Deltas computed each tick.
    pub speed_in: f64,  // KB/s inbound
    pub speed_out: f64, // KB/s outbound

    // TCP-only stats (populated via GetPerTcpConnectionEStats).
    pub rtt_us: u32,      // mean RTT in microseconds (0 if unavailable)
    pub retransmits: u64, // cumulative retransmit count

    // 60-sample rolling history for sparklines.
    pub history_in: VecDeque<f64>,
    pub history_out: VecDeque<f64>,

    /// Whether the connection is still alive according to the last poll.
    pub alive: bool,

    /// Whether ESTATS is available (requires admin + enabled per-connection).
    pub estats_available: bool,

    /// Time this entry was added to the spy list.
    pub pinned_at: Instant,
}

impl SpyEntry {
    pub fn new(conn: ConnectionEntry) -> Self {
        Self {
            conn,
            bytes_in_total: 0,
            bytes_out_total: 0,
            pkts_in_total: 0,
            pkts_out_total: 0,
            speed_in: 0.0,
            speed_out: 0.0,
            rtt_us: 0,
            retransmits: 0,
            history_in: VecDeque::with_capacity(60),
            history_out: VecDeque::with_capacity(60),
            alive: true,
            estats_available: false,
            pinned_at: Instant::now(),
        }
    }
}

// ── App state ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct AppState {
    pub interfaces: Vec<InterfaceStats>,
    pub connections: Vec<ConnectionEntry>,
    pub alerts: Vec<AlertEvent>,
    pub thresholds: Vec<ThresholdConfig>,
    pub refresh_interval_ms: u64,
    pub paused: bool,
    pub spy_entries: Vec<SpyEntry>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            interfaces: Vec::new(),
            connections: Vec::new(),
            alerts: Vec::new(),
            thresholds: Vec::new(),
            refresh_interval_ms: 1000,
            paused: false,
            spy_entries: Vec::new(),
        }
    }
}

// ── UI state (not shared — only touched by main thread) ──────────────────────

pub struct UiState {
    pub selected_tab: SelectedTab,
    /// Per-tab TableState — owns both the selected index and the scroll offset.
    pub table_state_interfaces: TableState,
    pub table_state_connections: TableState,
    pub table_state_processes: TableState,
    pub table_state_spy: TableState,
    pub filter: String,
    pub filter_active: bool,
    pub detail_open: bool,
}

impl UiState {
    #[allow(dead_code)]
    /// Return the selected row index for the currently active tab.
    pub fn selected_row(&self) -> usize {
        self.active_table_state().selected().unwrap_or(0)
    }

    #[allow(dead_code)]
    pub fn active_table_state(&self) -> &TableState {
        match self.selected_tab {
            SelectedTab::Interfaces => &self.table_state_interfaces,
            SelectedTab::Connections => &self.table_state_connections,
            SelectedTab::Processes => &self.table_state_processes,
            SelectedTab::Spy => &self.table_state_spy,
        }
    }

    pub fn active_table_state_mut(&mut self) -> &mut TableState {
        match self.selected_tab {
            SelectedTab::Interfaces => &mut self.table_state_interfaces,
            SelectedTab::Connections => &mut self.table_state_connections,
            SelectedTab::Processes => &mut self.table_state_processes,
            SelectedTab::Spy => &mut self.table_state_spy,
        }
    }

    /// Move selection up by one, clamping at 0.
    pub fn select_prev(&mut self) {
        let ts = self.active_table_state_mut();
        let next = ts.selected().unwrap_or(0).saturating_sub(1);
        ts.select(Some(next));
    }

    /// Move selection down by one, clamping at `max_idx`.
    pub fn select_next(&mut self, max_idx: usize) {
        let ts = self.active_table_state_mut();
        let next = (ts.selected().unwrap_or(0) + 1).min(max_idx);
        ts.select(Some(next));
    }

    /// Move selection up by `page` rows, clamping at 0.
    pub fn select_prev_page(&mut self, page: usize) {
        let ts = self.active_table_state_mut();
        let next = ts.selected().unwrap_or(0).saturating_sub(page);
        ts.select(Some(next));
    }

    /// Move selection down by `page` rows, clamping at `max_idx`.
    pub fn select_next_page(&mut self, max_idx: usize, page: usize) {
        let ts = self.active_table_state_mut();
        let next = (ts.selected().unwrap_or(0) + page).min(max_idx);
        ts.select(Some(next));
    }

    /// Reset selection to row 0 for the active tab.
    pub fn reset_selection(&mut self) {
        self.active_table_state_mut().select(Some(0));
    }
}

impl Default for UiState {
    fn default() -> Self {
        let mut ts_iface = TableState::default();
        ts_iface.select(Some(0));
        let mut ts_conn = TableState::default();
        ts_conn.select(Some(0));
        let mut ts_proc = TableState::default();
        ts_proc.select(Some(0));
        let mut ts_spy = TableState::default();
        ts_spy.select(Some(0));
        Self {
            selected_tab: SelectedTab::Interfaces,
            table_state_interfaces: ts_iface,
            table_state_connections: ts_conn,
            table_state_processes: ts_proc,
            table_state_spy: ts_spy,
            filter: String::new(),
            filter_active: false,
            detail_open: false,
        }
    }
}
