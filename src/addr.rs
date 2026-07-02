//! `SocketAddr` ↔ raw `sockaddr` conversions shared by the listener, server, and
//! association code (SCTP multihoming passes contiguous, mixed-family sockaddr
//! arrays to `sctp_bindx`/`sctp_connectx`, and `sctp_getpaddrs`/`getladdrs`
//! return the same shape).

use std::net::SocketAddr;

use crate::error::SctpError;

/// Convert a `SocketAddr` to a raw `sockaddr_storage` and its valid length.
pub(crate) fn to_raw(addr: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    match addr {
        SocketAddr::V4(v4) => {
            let sin = unsafe { &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in) };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr.s_addr = u32::from_ne_bytes(v4.ip().octets());
            (
                storage,
                std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t,
            )
        }
        SocketAddr::V6(v6) => {
            let sin6 = unsafe { &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in6) };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_addr.s6_addr = v6.ip().octets();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_scope_id = v6.scope_id();
            (
                storage,
                std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t,
            )
        }
    }
}

/// Convert a raw `sockaddr_storage` to a `SocketAddr`.
pub(crate) fn from_raw(storage: &libc::sockaddr_storage) -> Result<SocketAddr, SctpError> {
    match storage.ss_family as libc::c_int {
        libc::AF_INET => {
            let sin = unsafe { &*(storage as *const _ as *const libc::sockaddr_in) };
            let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            Ok(SocketAddr::V4(std::net::SocketAddrV4::new(
                ip,
                u16::from_be(sin.sin_port),
            )))
        }
        libc::AF_INET6 => {
            let sin6 = unsafe { &*(storage as *const _ as *const libc::sockaddr_in6) };
            let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
            Ok(SocketAddr::V6(std::net::SocketAddrV6::new(
                ip,
                u16::from_be(sin6.sin6_port),
                sin6.sin6_flowinfo,
                sin6.sin6_scope_id,
            )))
        }
        family => Err(SctpError::InvalidAddress(format!(
            "unsupported address family: {family}"
        ))),
    }
}

/// Pack a list of socket addresses into the contiguous buffer of raw sockaddrs
/// that `sctp_bindx`/`sctp_connectx` expect (each entry sized by its family, so
/// v4 and v6 may be mixed; the kernel reads `sa_family`).
pub(crate) fn pack(addrs: &[SocketAddr]) -> Vec<u8> {
    let mut buf = Vec::new();
    for addr in addrs {
        let (storage, len) = to_raw(addr);
        let bytes =
            unsafe { std::slice::from_raw_parts(&storage as *const _ as *const u8, len as usize) };
        buf.extend_from_slice(bytes);
    }
    buf
}

/// Walk a contiguous, mixed-family `sockaddr` array of `count` entries (as
/// returned by `sctp_getpaddrs`/`sctp_getladdrs`) into `SocketAddr`s.
///
/// # Safety
/// `ptr` must point to `count` valid, contiguous sockaddrs sized by family.
pub(crate) unsafe fn parse_array(
    ptr: *const libc::sockaddr,
    count: libc::c_int,
) -> Vec<SocketAddr> {
    let mut out = Vec::with_capacity(count.max(0) as usize);
    let mut p = ptr as *const u8;
    for _ in 0..count {
        let family = (*(p as *const libc::sockaddr)).sa_family as libc::c_int;
        let step = match family {
            libc::AF_INET => std::mem::size_of::<libc::sockaddr_in>(),
            libc::AF_INET6 => std::mem::size_of::<libc::sockaddr_in6>(),
            _ => break,
        };
        let mut storage: libc::sockaddr_storage = std::mem::zeroed();
        std::ptr::copy_nonoverlapping(p, &mut storage as *mut _ as *mut u8, step);
        if let Ok(a) = from_raw(&storage) {
            out.push(a);
        }
        p = p.add(step);
    }
    out
}
