//! Who owns a loopback TCP connection: the local endpoint of a friend's world takes only the
//! connection of the game the launcher started (HOST1, `crates/pl-host/PROTOCOL.md` §8). Windows
//! reads the TCP table with owning processes (IP Helper's `GetExtendedTcpTable`, declared here so
//! the shared `windows` build keeps its features); elsewhere nothing is known and the caller
//! decides.

/// The process that owns the client end of a loopback connection from `127.0.0.1:client_port`
/// (the address an accepting socket sees), when the system tells.
pub fn loopback_owner(client_port: u16) -> Option<u32> {
    #[cfg(windows)]
    {
        windows_owner(client_port)
    }
    #[cfg(not(windows))]
    {
        let _ = client_port;
        None
    }
}

/// `MIB_TCPROW_OWNER_PID`: addresses and ports as the table stores them (ports in network byte
/// order in the low 16 bits).
#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy)]
struct TcpRowOwnerPid {
    state: u32,
    local_addr: u32,
    local_port: u32,
    remote_addr: u32,
    remote_port: u32,
    owning_pid: u32,
}

#[cfg(windows)]
#[link(name = "iphlpapi")]
unsafe extern "system" {
    /// IP Helper (iphlpapi.dll), on every Windows the launcher runs on.
    fn GetExtendedTcpTable(
        table: *mut core::ffi::c_void,
        size: *mut u32,
        order: i32,
        family: u32,
        class: i32,
        reserved: u32,
    ) -> u32;
}

#[cfg(windows)]
fn windows_owner(client_port: u16) -> Option<u32> {
    /// `AF_INET` (the table of IPv4 connections).
    const AF_INET: u32 = 2;
    /// `TCP_TABLE_OWNER_PID_CONNECTIONS`.
    const OWNER_PID_CONNECTIONS: i32 = 4;
    const NO_ERROR: u32 = 0;
    const LOOPBACK: u32 = u32::from_ne_bytes([127, 0, 0, 1]);
    /// The table starts with its row count (`dwNumEntries`), then the rows.
    const HEADER: usize = std::mem::size_of::<u32>();

    // The table can grow between the size question and the read: a few tries.
    let mut size: u32 = 0;
    for _ in 0..4 {
        let mut buf = vec![0u32; (size as usize).div_ceil(4).max(1)];
        let mut asked = u32::try_from(buf.len() * 4).ok()?;
        // SAFETY: `buf` holds `asked` writable bytes, aligned for the table's u32 fields; the
        // call writes at most that many and says how many it needs otherwise.
        let status = unsafe {
            GetExtendedTcpTable(
                buf.as_mut_ptr().cast(),
                &mut asked,
                0,
                AF_INET,
                OWNER_PID_CONNECTIONS,
                0,
            )
        };
        if status != NO_ERROR {
            // ERROR_INSUFFICIENT_BUFFER: `asked` says how much is needed now.
            size = asked;
            continue;
        }
        let written = (asked as usize).min(buf.len() * 4);
        if written < HEADER {
            return None;
        }
        let count = buf[0] as usize;
        let fits = (written - HEADER) / std::mem::size_of::<TcpRowOwnerPid>();
        // SAFETY: `min(count, fits)` rows of plain u32 fields follow the count inside the bytes
        // the call wrote, aligned like `buf`.
        let rows = unsafe {
            std::slice::from_raw_parts(
                buf.as_ptr().add(1).cast::<TcpRowOwnerPid>(),
                count.min(fits),
            )
        };
        // Ports sit in the low 16 bits in network byte order.
        let port = |raw: u32| u16::from_be((raw & 0xFFFF) as u16);
        return rows
            .iter()
            .find(|row| {
                row.local_addr == LOOPBACK
                    && row.remote_addr == LOOPBACK
                    && port(row.local_port) == client_port
            })
            .map(|row| row.owning_pid);
    }
    None
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn a_loopback_connection_of_this_process_is_its_own() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let server_port = listener.local_addr().unwrap().port();
        let client = std::net::TcpStream::connect(("127.0.0.1", server_port)).unwrap();
        let (_server_side, _) = listener.accept().unwrap();
        let client_port = client.local_addr().unwrap().port();
        assert_eq!(loopback_owner(client_port), Some(std::process::id()));
        // A port no connection uses.
        let unused = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let unused_port = unused.local_addr().unwrap().port();
        drop(unused);
        assert_eq!(loopback_owner(unused_port), None);
    }
}
