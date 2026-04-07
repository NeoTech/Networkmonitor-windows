use std::sync::{Arc, RwLock};
use tokio::time::interval;
use std::time::Duration;

use crate::app::AppState;
use crate::connections::collect_connections;
use crate::processes::ProcessCache;

pub async fn run_connection_collection(state: Arc<RwLock<AppState>>, refresh_ms: u64) {
    let mut ticker = interval(Duration::from_millis(refresh_ms));
    let mut proc_cache = ProcessCache::new();

    loop {
        ticker.tick().await;

        {
            let s = state.read().unwrap();
            if s.paused {
                continue;
            }
        }

        let mut conns = collect_connections();

        // Resolve process names
        for conn in conns.iter_mut() {
            conn.process_name = proc_cache.get_name(conn.pid);
        }

        // Prune process cache to active PIDs
        let active_pids: Vec<u32> = conns.iter().map(|c| c.pid).collect();
        proc_cache.prune(&active_pids);

        let mut s = state.write().unwrap();
        s.connections = conns;
    }
}
