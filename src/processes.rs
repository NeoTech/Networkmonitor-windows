use std::collections::HashMap;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

/// Cache mapping PID -> process name to avoid redundant syscalls.
pub struct ProcessCache {
    cache: HashMap<u32, String>,
}

impl ProcessCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Return the process name for the given PID, using the cache if available.
    pub fn get_name(&mut self, pid: u32) -> String {
        if pid == 0 {
            return "[System Idle]".to_string();
        }
        if pid == 4 {
            return "[System]".to_string();
        }
        if let Some(name) = self.cache.get(&pid) {
            return name.clone();
        }
        let name = query_process_name(pid).unwrap_or_else(|| "[unknown]".to_string());
        self.cache.insert(pid, name.clone());
        name
    }

    /// Prune cache entries for PIDs that are no longer in the active connection list.
    pub fn prune(&mut self, active_pids: &[u32]) {
        self.cache.retain(|pid, _| active_pids.contains(pid));
    }
}

#[cfg(windows)]
fn query_process_name(pid: u32) -> Option<String> {
    let handle: HANDLE = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }

    let mut buf = [0u16; 1024];
    let mut size = buf.len() as u32;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size)
    };
    unsafe { CloseHandle(handle) };

    if ok == 0 || size == 0 {
        return None;
    }

    let path = String::from_utf16_lossy(&buf[..size as usize]);
    // Return only the filename component
    path.split('\\').last().map(|s| s.to_string())
}

#[cfg(not(windows))]
fn query_process_name(_pid: u32) -> Option<String> {
    None
}
