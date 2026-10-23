//! Who owns a loopback TCP connection: the local endpoint of a friend's world takes only the
//! connection of the game the launcher started (HOST1, `crates/pl-host/PROTOCOL.md` §8). Windows
//! reads the TCP table with owning processes; elsewhere nothing is known and the caller decides.

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

#[cfg(windows)]
fn windows_owner(client_port: u16) -> Option<u32> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
        TCP_TABLE_OWNER_PID_CONNECTIONS,
    };
    /// `AF_INET` (the table of IPv4 connections).
    const AF_INET: u32 = 2;
    const LOOPBACK: u32 = u32::from_ne_bytes([127, 0, 0, 1]);

    // The table can grow between the size question and the read: a few tries.
    let mut size: u32 = 0;
    for _ in 0..4 {
        let mut buf = vec![0u64; (size as usize).div_ceil(8).max(1)];
        // SAFETY: `buf` holds `size` writable bytes, aligned for the table's u32 fields.
        let status = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr().cast()),
                &mut size,
                false,
                AF_INET,
                TCP_TABLE_OWNER_PID_CONNECTIONS,
                0,
            )
        };
        if status != 0 {
            // ERROR_INSUFFICIENT_BUFFER: `size` now says how much is needed.
            continue;
        }
        let bytes = (size as usize).min(buf.len() * 8);
        if bytes < std::mem::size_of::<u32>() {
            return None;
        }
        // SAFETY: the call wrote a MIB_TCPTABLE_OWNER_PID of `size` bytes into `buf`.
        let table = unsafe { &*buf.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>() };
        let header = std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table);
        let fits = (bytes.saturating_sub(header)) / std::mem::size_of::<MIB_TCPROW_OWNER_PID>();
        let count = (table.dwNumEntries as usize).min(fits);
        // SAFETY: `count` rows follow the header inside the bytes the call wrote.
        let rows = unsafe {
            let first = buf
                .as_ptr()
                .cast::<u8>()
                .add(header)
                .cast::<MIB_TCPROW_OWNER_PID>();
            std::slice::from_raw_parts(first, count)
        };
        // Ports sit in the low 16 bits in network byte order.
        let port = |raw: u32| u16::from_be((raw & 0xFFFF) as u16);
        return rows
            .iter()
            .find(|row| {
                row.dwLocalAddr == LOOPBACK
                    && row.dwRemoteAddr == LOOPBACK
                    && port(row.dwLocalPort) == client_port
            })
            .map(|row| row.dwOwningPid);
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
