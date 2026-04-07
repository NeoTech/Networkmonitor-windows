use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::interval;

use crate::app::AppState;

/// Background task: poll ESTATS for every pinned spy entry and update rates.
pub async fn run_spy_collection(state: Arc<RwLock<AppState>>, refresh_ms: u64) {
    let mut ticker = interval(Duration::from_millis(refresh_ms.max(200)));
    loop {
        ticker.tick().await;

        // Check pause without holding the lock long
        {
            let s = state.read().unwrap();
            if s.paused || s.spy_entries.is_empty() {
                continue;
            }
        }

        // Collect all spy entries to work on (clone to avoid holding the lock).
        let entries_snapshot = {
            let s = state.read().unwrap();
            s.spy_entries.clone()
        };

        let mut updated = entries_snapshot;

        #[cfg(windows)]
        {
            poll_estats_windows(&mut updated);
        }

        #[cfg(not(windows))]
        {
            // On non-Windows we just mark everything alive with zeroed stats.
            for e in updated.iter_mut() {
                e.alive = true;
            }
        }

        // Write results back.
        let mut s = state.write().unwrap();
        // Merge: preserve pinned_at and any entries added while we were polling.
        // Strategy: match by connection key; if a new entry was added while we
        // were polling, it stays with zeroed stats until the next tick.
        for e in updated {
            let key = e.conn.key();
            if let Some(existing) = s.spy_entries.iter_mut().find(|x| x.conn.key() == key) {
                *existing = e;
            }
        }
    }
}

// ── Windows ESTATS implementation ────────────────────────────────────────────

#[cfg(windows)]
fn poll_estats_windows(entries: &mut Vec<crate::app::SpyEntry>) {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetPerTcpConnectionEStats, SetPerTcpConnectionEStats,
        TCP_ESTATS_BANDWIDTH_ROD_v0, TCP_ESTATS_BANDWIDTH_RW_v0,
        TCP_ESTATS_PATH_ROD_v0, TCP_ESTATS_PATH_RW_v0,
        TcpConnectionEstatsBandwidth, TcpConnectionEstatsPath,
        MIB_TCPROW_LH, MIB_TCPROW_LH_0,
    };

    for entry in entries.iter_mut() {
        if entry.conn.protocol != "TCP" {
            // UDP: no byte-level stats available; just mark alive and push zeroes.
            entry.alive = true;
            push_history(entry, 0.0, 0.0);
            continue;
        }

        // Build the MIB_TCPROW_LH key (network byte order, matching GetExtendedTcpTable).
        let local_addr = ipv4_to_u32_le(&entry.conn.local_addr);
        let remote_addr = ipv4_to_u32_le(&entry.conn.remote_addr);
        let local_port = port_to_be(entry.conn.local_port) as u32;
        let remote_port = port_to_be(entry.conn.remote_port) as u32;

        let row = MIB_TCPROW_LH {
            Anonymous: MIB_TCPROW_LH_0 {
                dwState: 5, // MIB_TCP_STATE_ESTAB — used as a key, value is ignored
            },
            dwLocalAddr: local_addr,
            dwLocalPort: local_port,
            dwRemoteAddr: remote_addr,
            dwRemotePort: remote_port,
        };

        // ── Enable bandwidth stats (idempotent; fails silently if already on) ──
        let mut bw_rw = TCP_ESTATS_BANDWIDTH_RW_v0 {
            EnableCollectionInbound: 1,   // TcpBoolOptEnabled = 1
            EnableCollectionOutbound: 1,
        };
        unsafe {
            SetPerTcpConnectionEStats(
                &row,
                TcpConnectionEstatsBandwidth,
                &mut bw_rw as *mut _ as *const u8,
                0,
                std::mem::size_of::<TCP_ESTATS_BANDWIDTH_RW_v0>() as u32,
                0,
            );
        }

        // ── Enable path stats (for RTT / retransmits) ──────────────────────────
        let mut path_rw = TCP_ESTATS_PATH_RW_v0 { EnableCollection: 1 };
        unsafe {
            SetPerTcpConnectionEStats(
                &row,
                TcpConnectionEstatsPath,
                &mut path_rw as *mut _ as *const u8,
                0,
                std::mem::size_of::<TCP_ESTATS_PATH_RW_v0>() as u32,
                0,
            );
        }

        // ── Read bandwidth ROD ─────────────────────────────────────────────────
        let mut bw_rod: TCP_ESTATS_BANDWIDTH_ROD_v0 = unsafe { std::mem::zeroed() };
        let rc_bw = unsafe {
            GetPerTcpConnectionEStats(
                &row,
                TcpConnectionEstatsBandwidth,
                std::ptr::null_mut(),
                0,
                0,
                std::ptr::null_mut(),
                0,
                0,
                &mut bw_rod as *mut _ as *mut u8,
                0,
                std::mem::size_of::<TCP_ESTATS_BANDWIDTH_ROD_v0>() as u32,
            )
        };

        if rc_bw != 0 {
            // ESTATS not available (connection closed or insufficient privileges).
            entry.alive = false;
            entry.estats_available = false;
            push_history(entry, 0.0, 0.0);
            continue;
        }

        entry.estats_available = true;
        entry.alive = true;

        // InboundBandwidth / OutboundBandwidth are instantaneous bit/s estimates.
        // Convert to KB/s.
        let speed_in_kbps = (bw_rod.InboundBandwidth as f64) / 8.0 / 1024.0;
        let speed_out_kbps = (bw_rod.OutboundBandwidth as f64) / 8.0 / 1024.0;

        entry.speed_in = speed_in_kbps;
        entry.speed_out = speed_out_kbps;

        // ── Read path ROD (RTT, retransmits) ───────────────────────────────────
        let mut path_rod: TCP_ESTATS_PATH_ROD_v0 = unsafe { std::mem::zeroed() };
        let rc_path = unsafe {
            GetPerTcpConnectionEStats(
                &row,
                TcpConnectionEstatsPath,
                std::ptr::null_mut(),
                0,
                0,
                std::ptr::null_mut(),
                0,
                0,
                &mut path_rod as *mut _ as *mut u8,
                0,
                std::mem::size_of::<TCP_ESTATS_PATH_ROD_v0>() as u32,
            )
        };

        if rc_path == 0 {
            entry.rtt_us = path_rod.SmoothedRtt;
            // PktsRetrans = cumulative retransmitted segments
            entry.retransmits = path_rod.PktsRetrans as u64;
        }

        push_history(entry, speed_in_kbps, speed_out_kbps);
    }
}

/// Push a new sample to the 60-point rolling history buffers.
fn push_history(entry: &mut crate::app::SpyEntry, speed_in: f64, speed_out: f64) {
    if entry.history_in.len() == 60 {
        entry.history_in.pop_front();
    }
    entry.history_in.push_back(speed_in);

    if entry.history_out.len() == 60 {
        entry.history_out.pop_front();
    }
    entry.history_out.push_back(speed_out);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Convert a dotted-decimal IPv4 string to a little-endian u32 (same format
/// used by GetExtendedTcpTable / MIB_TCPROW_LH).
#[cfg(windows)]
fn ipv4_to_u32_le(addr: &str) -> u32 {
    let mut parts = [0u8; 4];
    for (i, octet) in addr.split('.').enumerate() {
        if i >= 4 { break; }
        parts[i] = octet.parse().unwrap_or(0);
    }
    u32::from_le_bytes(parts)
}

/// Convert a host-order port to the big-endian u16 (network order) as stored
/// in MIB_TCPROW_LH.
#[cfg(windows)]
fn port_to_be(port: u16) -> u16 {
    ((port & 0xFF) << 8) | (port >> 8)
}
