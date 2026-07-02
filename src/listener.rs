use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};

use tokio::io::unix::AsyncFd;

use crate::addr;
use crate::association::{self, SctpAssociation};
use crate::config::SctpConfig;
use crate::error::SctpError;
use crate::sys;

/// A one-to-one style SCTP listener: `accept` yields a dedicated
/// [`SctpAssociation`] per peer, like `TcpListener` → `TcpStream`.
///
/// For a single socket that serves many associations at once, see
/// [`SctpServer`](crate::SctpServer).
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
    /// Bind and listen on a single address (kernel defaults).
    pub fn bind(addr: SocketAddr) -> Result<Self, SctpError> {
        Self::bind_impl(&[addr], &SctpConfig::default())
    }

    /// Bind and listen on a single address with an explicit [`SctpConfig`].
    pub fn bind_config(addr: SocketAddr, config: &SctpConfig) -> Result<Self, SctpError> {
        Self::bind_impl(&[addr], config)
    }

    /// Bind a multihomed listener across several local addresses (may mix IPv4
    /// and IPv6); the extra addresses are added with `sctp_bindx`.
    pub fn bind_multi(addrs: &[SocketAddr]) -> Result<Self, SctpError> {
        Self::bind_impl(addrs, &SctpConfig::default())
    }

    /// Multihomed bind with an explicit [`SctpConfig`].
    pub fn bind_multi_with(addrs: &[SocketAddr], config: &SctpConfig) -> Result<Self, SctpError> {
        Self::bind_impl(addrs, config)
    }

    fn bind_impl(addrs: &[SocketAddr], config: &SctpConfig) -> Result<Self, SctpError> {
        let primary = *addrs.first().ok_or_else(|| {
            SctpError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no bind addresses",
            ))
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
        match Self::setup(fd, primary, addrs, domain, config) {
            Ok(inner) => Ok(Self { inner }),
            Err(e) => {
                unsafe { libc::close(fd) };
                Err(e)
            }
        }
    }

    fn setup(
        fd: RawFd,
        primary: SocketAddr,
        addrs: &[SocketAddr],
        domain: libc::c_int,
        config: &SctpConfig,
    ) -> Result<AsyncFd<ListenSocket>, SctpError> {
        set_reuseaddr(fd);
        if domain == libc::AF_INET6 {
            association::set_v6only(fd, false);
        }
        config.apply(fd)?;

        let (sockaddr, socklen) = addr::to_raw(&primary);
        if unsafe { libc::bind(fd, &sockaddr as *const _ as *const libc::sockaddr, socklen) } < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        if addrs.len() > 1 {
            let packed = addr::pack(&addrs[1..]);
            let ret = unsafe {
                sys::sctp_bindx(
                    fd,
                    packed.as_ptr() as *const libc::sockaddr,
                    (addrs.len() - 1) as libc::c_int,
                    sys::SCTP_BINDX_ADD_ADDR,
                )
            };
            if ret < 0 {
                return Err(SctpError::Io(io::Error::last_os_error()));
            }
        }
        if unsafe { libc::listen(fd, 128) } < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        association::set_nonblocking(fd)?;
        Ok(AsyncFd::new(ListenSocket { fd })?)
    }

    /// Accept an incoming association, returning it and the peer's address.
    pub async fn accept(&self) -> Result<(SctpAssociation, SocketAddr), SctpError> {
        loop {
            let mut guard = self.inner.readable().await?;

            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let new_fd = unsafe {
                libc::accept(
                    self.inner.as_raw_fd(),
                    &mut storage as *mut _ as *mut libc::sockaddr,
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
            let peer = addr::from_raw(&storage)?;
            let assoc = SctpAssociation::from_raw_fd(new_fd)?;
            return Ok((assoc, peer));
        }
    }

    /// The local address this listener is bound to (the primary; use
    /// `sctp_getladdrs` on an association for the full multihomed set).
    pub fn local_addr(&self) -> Result<SocketAddr, SctpError> {
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        let ret = unsafe {
            libc::getsockname(
                self.inner.as_raw_fd(),
                &mut storage as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            )
        };
        if ret < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        addr::from_raw(&storage)
    }
}

fn set_reuseaddr(fd: RawFd) {
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
}
