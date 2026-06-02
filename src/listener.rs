use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};

use tokio::io::unix::AsyncFd;

use crate::association::SctpAssociation;
use crate::error::SctpError;
use crate::sys;

/// An SCTP listener that accepts incoming associations.
///
/// Wraps a one-to-one style SCTP socket in listening mode.
pub struct SctpListener {
    inner: AsyncFd<ListenSocket>,
}

struct ListenSocket {
    fd: RawFd,
}

impl AsRawFd for ListenSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for ListenSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl SctpListener {
    /// Bind to the given address and start listening for SCTP associations.
    pub fn bind(addr: SocketAddr) -> Result<Self, SctpError> {
        let domain = match addr {
            SocketAddr::V4(_) => libc::AF_INET,
            SocketAddr::V6(_) => libc::AF_INET6,
        };

        let fd = unsafe {
            libc::socket(domain, libc::SOCK_STREAM, sys::IPPROTO_SCTP)
        };
        if fd < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }

        // Allow address reuse
        let optval: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }

        // Bind
        let (sockaddr, socklen) = socket_addr_to_raw(&addr);
        let ret = unsafe {
            libc::bind(fd, &sockaddr as *const _ as *const libc::sockaddr, socklen)
        };
        if ret < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        // Listen
        let ret = unsafe { libc::listen(fd, 128) };
        if ret < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        // Set non-blocking
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }
        let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
        if ret < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        let socket = ListenSocket { fd };
        let inner = AsyncFd::new(socket)?;
        Ok(Self { inner })
    }

    /// Bind a multihomed SCTP listener across one or more local addresses,
    /// which may mix IPv4 and IPv6. The first address is bound conventionally;
    /// the rest are added with `sctp_bindx`. A v6 socket is opened with
    /// `IPV6_V6ONLY=0` so v4 and v6 paths can share the association.
    pub fn bind_multi(addrs: &[SocketAddr]) -> Result<Self, SctpError> {
        let primary = *addrs.first().ok_or_else(|| {
            SctpError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no bind addresses"))
        })?;
        let domain = if addrs.iter().any(SocketAddr::is_ipv6) {
            libc::AF_INET6
        } else {
            libc::AF_INET
        };

        let fd = unsafe { libc::socket(domain, libc::SOCK_STREAM, sys::IPPROTO_SCTP) };
        if fd < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }

        let optval: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &optval as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
        if domain == libc::AF_INET6 {
            // Allow v4 and v6 addresses on the same association.
            let off: libc::c_int = 0;
            unsafe {
                libc::setsockopt(
                    fd,
                    libc::IPPROTO_IPV6,
                    libc::IPV6_V6ONLY,
                    &off as *const _ as *const libc::c_void,
                    std::mem::size_of::<libc::c_int>() as libc::socklen_t,
                );
            }
        }

        let (sockaddr, socklen) = socket_addr_to_raw(&primary);
        if unsafe { libc::bind(fd, &sockaddr as *const _ as *const libc::sockaddr, socklen) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        if addrs.len() > 1 {
            let packed = pack_addrs(&addrs[1..]);
            let ret = unsafe {
                sys::sctp_bindx(
                    fd,
                    packed.as_ptr() as *const libc::sockaddr,
                    (addrs.len() - 1) as libc::c_int,
                    sys::SCTP_BINDX_ADD_ADDR,
                )
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                unsafe { libc::close(fd) };
                return Err(SctpError::Io(err));
            }
        }

        if unsafe { libc::listen(fd, 128) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            let err = io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(SctpError::Io(err));
        }

        let socket = ListenSocket { fd };
        Ok(Self { inner: AsyncFd::new(socket)? })
    }

    /// Accept an incoming SCTP association.
    ///
    /// Returns the new association and the peer's address.
    pub async fn accept(&self) -> Result<(SctpAssociation, SocketAddr), SctpError> {
        loop {
            let mut guard = self.inner.readable().await?;

            let mut addr_storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut addr_len: libc::socklen_t =
                std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

            let new_fd = unsafe {
                libc::accept(
                    self.inner.as_raw_fd(),
                    &mut addr_storage as *mut _ as *mut libc::sockaddr,
                    &mut addr_len,
                )
            };

            if new_fd < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                }
                return Err(SctpError::Io(err));
            }

            let peer_addr = raw_to_socket_addr(&addr_storage)?;
            let assoc = SctpAssociation::from_raw_fd(new_fd)?;
            return Ok((assoc, peer_addr));
        }
    }

    /// Returns the local address this listener is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, SctpError> {
        let mut addr_storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len: libc::socklen_t =
            std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

        let ret = unsafe {
            libc::getsockname(
                self.inner.as_raw_fd(),
                &mut addr_storage as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            )
        };
        if ret < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }

        raw_to_socket_addr(&addr_storage)
    }
}

/// Convert a `SocketAddr` to raw sockaddr_storage + length.
/// Pack a list of socket addresses into the contiguous buffer of raw
/// sockaddrs that `sctp_bindx`/`sctp_connectx` expect (each entry sized by
/// its family, so v4 and v6 may be mixed; the kernel reads `sa_family`).
pub(crate) fn pack_addrs(addrs: &[SocketAddr]) -> Vec<u8> {
    let mut buf = Vec::new();
    for addr in addrs {
        let (storage, len) = socket_addr_to_raw(addr);
        let bytes =
            unsafe { std::slice::from_raw_parts(&storage as *const _ as *const u8, len as usize) };
        buf.extend_from_slice(bytes);
    }
    buf
}

fn socket_addr_to_raw(addr: &SocketAddr) -> (libc::sockaddr_storage, libc::socklen_t) {
    let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };

    match addr {
        SocketAddr::V4(v4) => {
            let sin = unsafe { &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in) };
            sin.sin_family = libc::AF_INET as libc::sa_family_t;
            sin.sin_port = v4.port().to_be();
            sin.sin_addr.s_addr = u32::from_ne_bytes(v4.ip().octets());
            (storage, std::mem::size_of::<libc::sockaddr_in>() as libc::socklen_t)
        }
        SocketAddr::V6(v6) => {
            let sin6 = unsafe { &mut *(&mut storage as *mut _ as *mut libc::sockaddr_in6) };
            sin6.sin6_family = libc::AF_INET6 as libc::sa_family_t;
            sin6.sin6_port = v6.port().to_be();
            sin6.sin6_addr.s6_addr = v6.ip().octets();
            sin6.sin6_flowinfo = v6.flowinfo();
            sin6.sin6_scope_id = v6.scope_id();
            (storage, std::mem::size_of::<libc::sockaddr_in6>() as libc::socklen_t)
        }
    }
}

/// Convert a raw `sockaddr_storage` to a `SocketAddr`.
fn raw_to_socket_addr(storage: &libc::sockaddr_storage) -> Result<SocketAddr, SctpError> {
    match storage.ss_family as libc::c_int {
        libc::AF_INET => {
            let sin = unsafe { &*(storage as *const _ as *const libc::sockaddr_in) };
            let ip = std::net::Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr));
            let port = u16::from_be(sin.sin_port);
            Ok(SocketAddr::V4(std::net::SocketAddrV4::new(ip, port)))
        }
        libc::AF_INET6 => {
            let sin6 = unsafe { &*(storage as *const _ as *const libc::sockaddr_in6) };
            let ip = std::net::Ipv6Addr::from(sin6.sin6_addr.s6_addr);
            let port = u16::from_be(sin6.sin6_port);
            Ok(SocketAddr::V6(std::net::SocketAddrV6::new(
                ip,
                port,
                sin6.sin6_flowinfo,
                sin6.sin6_scope_id,
            )))
        }
        family => Err(SctpError::InvalidAddress(format!(
            "unsupported address family: {family}"
        ))),
    }
}
