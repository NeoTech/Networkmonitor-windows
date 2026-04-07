use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use tokio::time::interval;
use std::time::Duration;

use crate::app::{AppState, InterfaceStats};
use crate::alerts::check_thresholds;

#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, MIB_IF_TABLE2, MIB_IF_ROW2,
};
#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;

// Interface type constants (subset of IF_TYPE)
const IF_TYPE_ETHERNET: u32 = 6;
const IF_TYPE_IEEE80211: u32 = 71;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;
const IF_TYPE_TUNNEL: u32 = 131;

fn if_type_str(t: u32) -> &'static str {
    match t {
        IF_TYPE_ETHERNET => "Ethernet",
        IF_TYPE_IEEE80211 => "Wi-Fi",
        IF_TYPE_SOFTWARE_LOOPBACK => "Loopback",
        IF_TYPE_TUNNEL => "Tunnel",
        _ => "Other",
    }
}

/// Per-interface byte counters from the previous tick, used to compute KB/s deltas.
struct PrevCounters {
    bytes_in: u64,
    bytes_out: u64,
}

pub async fn run_data_collection(state: Arc<RwLock<AppState>>, refresh_ms: u64) {
    let mut ticker = interval(Duration::from_millis(refresh_ms));
    let mut prev: HashMap<String, PrevCounters> = HashMap::new();

    loop {
        ticker.tick().await;

        // Check if paused
        {
            let s = state.read().unwrap();
            if s.paused {
                continue;
            }
        }

        // Collect fresh interface data
        let mut new_interfaces = collect_interfaces(&mut prev);

        // Merge history from old state before acquiring write lock
        {
            let old_state = state.read().unwrap();
            merge_history(&old_state.interfaces, &mut new_interfaces);
        }

        let mut s = state.write().unwrap();

        // Check thresholds
        let thresholds = s.thresholds.clone();
        let new_alerts = check_thresholds(&new_interfaces, &thresholds);
        if !new_alerts.is_empty() {
            s.alerts.extend(new_alerts);
            let len = s.alerts.len();
            if len > 200 {
                s.alerts.drain(0..len - 200);
            }
        }

        s.interfaces = new_interfaces;
    }
}

#[cfg(windows)]
fn collect_interfaces(prev: &mut HashMap<String, PrevCounters>) -> Vec<InterfaceStats> {
    use std::slice;

    let mut result = Vec::new();
    let mut table_ptr: *mut MIB_IF_TABLE2 = std::ptr::null_mut();

    let rc = unsafe { GetIfTable2(&mut table_ptr) };
    if rc != 0 || table_ptr.is_null() {
        return result;
    }

    let table = unsafe { &*table_ptr };
    let rows: &[MIB_IF_ROW2] =
        unsafe { slice::from_raw_parts(table.Table.as_ptr(), table.NumEntries as usize) };

    for row in rows {
        // Skip interfaces that are not operationally up
        if row.OperStatus != IfOperStatusUp {
            continue;
        }

        // Decode the UTF-16 alias (friendly name)
        let name = decode_utf16(&row.Alias);
        if name.is_empty() {
            continue;
        }

        let desc = decode_utf16(&row.Description);
        let bytes_in = row.InOctets;
        let bytes_out = row.OutOctets;

        let (speed_in, speed_out) = if let Some(p) = prev.get(&name) {
            let din = bytes_in.saturating_sub(p.bytes_in) as f64 / 1024.0;
            let dout = bytes_out.saturating_sub(p.bytes_out) as f64 / 1024.0;
            (din, dout)
        } else {
            (0.0, 0.0)
        };

        prev.insert(name.clone(), PrevCounters { bytes_in, bytes_out });

        result.push(InterfaceStats {
            name,
            description: desc,
            speed_in,
            speed_out,
            bytes_in_total: bytes_in,
            bytes_out_total: bytes_out,
            packets_in: row.InUcastPkts.saturating_add(row.InNUcastPkts),
            packets_out: row.OutUcastPkts.saturating_add(row.OutNUcastPkts),
            errors_in: row.InErrors,
            errors_out: row.OutErrors,
            status: "Up".to_string(),
            if_type: if_type_str(row.Type).to_string(),
            history_in: VecDeque::with_capacity(60),
            history_out: VecDeque::with_capacity(60),
        });
    }

    unsafe { FreeMibTable(table_ptr as *mut _) };
    result
}

#[cfg(not(windows))]
fn collect_interfaces(_prev: &mut HashMap<String, PrevCounters>) -> Vec<InterfaceStats> {
    vec![]
}

#[cfg(windows)]
fn decode_utf16(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end]).to_string()
}

/// Merge rolling history from the previous state into newly collected interfaces.
pub fn merge_history(old: &[InterfaceStats], new: &mut Vec<InterfaceStats>) {
    for iface in new.iter_mut() {
        if let Some(old_iface) = old.iter().find(|o| o.name == iface.name) {
            let mut hin = old_iface.history_in.clone();
            hin.push_back(iface.speed_in);
            if hin.len() > 60 {
                hin.pop_front();
            }
            let mut hout = old_iface.history_out.clone();
            hout.push_back(iface.speed_out);
            if hout.len() > 60 {
                hout.pop_front();
            }
            iface.history_in = hin;
            iface.history_out = hout;
        } else {
            // First time seeing this interface
            iface.history_in.push_back(iface.speed_in);
            iface.history_out.push_back(iface.speed_out);
        }
    }
}
