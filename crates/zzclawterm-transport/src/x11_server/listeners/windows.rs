use std::{io, mem, net::Ipv4Addr, ptr};

use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    GetTcpTable2, IP_ADAPTER_ADDRESSES_LH, MIB_TCP_STATE_LISTEN, MIB_TCPROW2, MIB_TCPTABLE2,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

use super::listening_displays;

/// Aligned storage for Windows variable-length tables, with bounded retries if
/// adapters or connections change between the size query and the actual query.
fn query_buffer(mut query: impl FnMut(*mut u8, &mut u32) -> u32) -> io::Result<Vec<u64>> {
    let mut size = 16_384u32;
    for _ in 0..3 {
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let result = query(buffer.as_mut_ptr().cast(), &mut size);
        match result {
            NO_ERROR => return Ok(buffer),
            ERROR_BUFFER_OVERFLOW | ERROR_INSUFFICIENT_BUFFER => continue,
            _ => return Err(io::Error::from_raw_os_error(result as i32)),
        }
    }
    Err(io::Error::other("network tables kept changing"))
}

pub(in crate::x11_server) fn query(display: u16, pid: Option<u32>) -> io::Result<Vec<String>> {
    let port = 6000u16
        .checked_add(display)
        .ok_or_else(|| io::Error::other("invalid display"))?;
    let buffer = query_buffer(|buffer, size| unsafe { GetTcpTable2(buffer.cast(), size, 0) })?;
    let mut bindings = Vec::new();
    // The OS initializes the table and count in our aligned allocation. Validate
    // the variable-length tail before making a slice of it.
    unsafe {
        let table = buffer.as_ptr().cast::<MIB_TCPTABLE2>();
        let count = (*table).dwNumEntries as usize;
        let offset = mem::offset_of!(MIB_TCPTABLE2, table);
        if count > (buffer.len() * 8 - offset) / mem::size_of::<MIB_TCPROW2>() {
            return Err(io::Error::other("invalid TCP table length"));
        }
        let rows =
            std::slice::from_raw_parts(ptr::addr_of!((*table).table).cast::<MIB_TCPROW2>(), count);
        for row in rows {
            if row.dwState == MIB_TCP_STATE_LISTEN as u32
                && u16::from_be(row.dwLocalPort as u16) == port
                && pid.is_none_or(|pid| row.dwOwningPid == pid)
            {
                bindings.push(Ipv4Addr::from(row.dwLocalAddr.to_ne_bytes()));
            }
        }
    }
    let buffer = query_buffer(|buffer, size| unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER,
            ptr::null(),
            buffer.cast(),
            size,
        )
    })?;
    let mut adapters = Vec::new();
    // GetAdaptersAddresses owns the linked-list layout within this allocation;
    // all pointers are used only while its backing buffer remains alive.
    unsafe {
        let mut adapter = buffer.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        while !adapter.is_null() {
            let item = &*adapter;
            if item.OperStatus == IfOperStatusUp {
                let name = if item.FriendlyName.is_null() {
                    String::new()
                } else {
                    let mut len = 0;
                    while *item.FriendlyName.add(len) != 0 {
                        len += 1;
                    }
                    String::from_utf16_lossy(std::slice::from_raw_parts(item.FriendlyName, len))
                };
                let mut address = item.FirstUnicastAddress;
                while !address.is_null() {
                    let socket = (*address).Address;
                    if !socket.lpSockaddr.is_null()
                        && socket.iSockaddrLength as usize >= mem::size_of::<SOCKADDR_IN>()
                        && (*socket.lpSockaddr).sa_family == AF_INET
                    {
                        let ipv4 = &*socket.lpSockaddr.cast::<SOCKADDR_IN>();
                        adapters.push((
                            Ipv4Addr::from(ipv4.sin_addr.S_un.S_addr.to_ne_bytes()),
                            name.clone(),
                        ));
                    }
                    address = (*address).Next;
                }
            }
            adapter = item.Next;
        }
    }
    Ok(listening_displays(&bindings, &adapters, display))
}
