use crate::app::ConnectionEntry;

#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetExtendedTcpTable, GetExtendedUdpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
    MIB_UDPROW_OWNER_PID, MIB_UDPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
};
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::AF_INET;

fn tcp_state_str(state: u32) -> &'static str {
    // MIB_TCP_STATE values
    match state {
        1 => "CLOSED",
        2 => "LISTEN",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "ESTABLISHED",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "CLOSE_WAIT",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "TIME_WAIT",
        12 => "DELETE_TCB",
        _ => "UNKNOWN",
    }
}

#[cfg(windows)]
fn u32_to_ipv4(addr: u32) -> String {
    let bytes = addr.to_le_bytes();
    format!("{}.{}.{}.{}", bytes[0], bytes[1], bytes[2], bytes[3])
}

#[cfg(windows)]
fn be_port(port: u32) -> u16 {
    ((port & 0xFF) << 8 | (port >> 8 & 0xFF)) as u16
}

/// Enumerate active TCP connections (IPv4) with owning PIDs.
pub fn collect_connections() -> Vec<ConnectionEntry> {
    #[cfg(windows)]
    {
        collect_tcp_windows()
            .into_iter()
            .chain(collect_udp_windows())
            .collect()
    }
    #[cfg(not(windows))]
    {
        vec![]
    }
}

#[cfg(windows)]
fn collect_tcp_windows() -> Vec<ConnectionEntry> {
    let mut size: u32 = 0;
    // First call: get required buffer size
    unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        );
    }
    if size == 0 {
        return vec![];
    }

    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let rc = unsafe {
        GetExtendedTcpTable(
            buf.as_mut_ptr() as *mut _,
            &mut size,
            0,
            AF_INET as u32,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc != 0 {
        return vec![];
    }

    let table = unsafe { &*(buf.as_ptr() as *const MIB_TCPTABLE_OWNER_PID) };
    let count = table.dwNumEntries as usize;
    let rows: &[MIB_TCPROW_OWNER_PID] =
        unsafe { std::slice::from_raw_parts(table.table.as_ptr(), count) };

    let mut result = Vec::with_capacity(count);
    for row in rows {
        result.push(ConnectionEntry {
            protocol: "TCP".to_string(),
            local_addr: u32_to_ipv4(row.dwLocalAddr),
            local_port: be_port(row.dwLocalPort),
            remote_addr: u32_to_ipv4(row.dwRemoteAddr),
            remote_port: be_port(row.dwRemotePort),
            state: tcp_state_str(row.dwState).to_string(),
            pid: row.dwOwningPid,
            process_name: String::new(), // filled in by processes module
        });
    }
    result
}

#[cfg(windows)]
fn collect_udp_windows() -> Vec<ConnectionEntry> {
    let mut size: u32 = 0;
    unsafe {
        GetExtendedUdpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            AF_INET as u32,
            UDP_TABLE_OWNER_PID,
            0,
        );
    }
    if size == 0 {
        return vec![];
    }

    let mut buf: Vec<u8> = vec![0u8; size as usize];
    let rc = unsafe {
        GetExtendedUdpTable(
            buf.as_mut_ptr() as *mut _,
            &mut size,
            0,
            AF_INET as u32,
            UDP_TABLE_OWNER_PID,
            0,
        )
    };
    if rc != 0 {
        return vec![];
    }

    let table = unsafe { &*(buf.as_ptr() as *const MIB_UDPTABLE_OWNER_PID) };
    let count = table.dwNumEntries as usize;
    let rows: &[MIB_UDPROW_OWNER_PID] =
        unsafe { std::slice::from_raw_parts(table.table.as_ptr(), count) };

    let mut result = Vec::with_capacity(count);
    for row in rows {
        result.push(ConnectionEntry {
            protocol: "UDP".to_string(),
            local_addr: u32_to_ipv4(row.dwLocalAddr),
            local_port: be_port(row.dwLocalPort),
            remote_addr: String::from("*"),
            remote_port: 0,
            state: String::from("—"),
            pid: row.dwOwningPid,
            process_name: String::new(),
        });
    }
    result
}
