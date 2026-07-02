//! One-to-many style SCTP (`SOCK_SEQPACKET`): a single socket that serves many
//! associations at once. Instead of `accept`-ing a dedicated fd per peer, you
//! `recv` messages from *any* association (each tagged with its `assoc_id` and
//! peer address) and `send` back by `assoc_id`. New/closed associations surface
//! as `COMM_UP`/`COMM_LOST` notifications.
//!
//! When one association gets busy enough to warrant its own socket, `peeloff`
//! branches it into a one-to-one [`SctpAssociation`].

use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};

use tokio::io::unix::AsyncFd;

use crate::addr;
use crate::association::{self, SctpAssociation, SendOptions};
use crate::config::SctpConfig;
use crate::error::SctpError;
use crate::notification::Notification;
use crate::sys;
use crate::types::RecvInfo;

/// A one-to-many SCTP socket. See the [module docs](self).
pub struct SctpServer {
    inner: AsyncFd<ServerSocket>,
}

struct ServerSocket {
    fd: RawFd,
}
impl AsRawFd for ServerSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}
impl Drop for ServerSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// A message received on a one-to-many socket: data (with its association +
/// peer address) or an SCTP notification.
#[derive(Debug)]
pub enum ServerMessage {
    /// Application data.
    Data {
        data: Vec<u8>,
        info: RecvInfo,
        addr: SocketAddr,
    },
    /// An SCTP notification (e.g. `AssocChange` COMM_UP/COMM_LOST).
    Notification(Notification),
}

impl SctpServer {
    /// Bind a one-to-many socket on a single address (kernel defaults).
    pub fn bind(addr: SocketAddr) -> Result<Self, SctpError> {
        Self::bind_impl(&[addr], &SctpConfig::default())
    }

    /// Bind on a single address with an explicit [`SctpConfig`].
    pub fn bind_config(addr: SocketAddr, config: &SctpConfig) -> Result<Self, SctpError> {
        Self::bind_impl(&[addr], config)
    }

    /// Bind multihomed across several local addresses (may mix IPv4/IPv6).
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
        let fd = unsafe { libc::socket(domain, libc::SOCK_SEQPACKET, sys::IPPROTO_SCTP) };
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
    ) -> Result<AsyncFd<ServerSocket>, SctpError> {
        let one: libc::c_int = 1;
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_REUSEADDR,
                &one as *const _ as *const libc::c_void,
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            );
        }
        if domain == libc::AF_INET6 {
            association::set_v6only(fd, false);
        }
        config.apply(fd)?;
        association::configure_events(fd)?;

        let (sockaddr, socklen) = addr::to_raw(&primary);
        if unsafe { libc::bind(fd, &sockaddr as *const _ as *const libc::sockaddr, socklen) } < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        if addrs.len() > 1 {
            let packed = addr::pack(&addrs[1..]);
            if unsafe {
                sys::sctp_bindx(
                    fd,
                    packed.as_ptr() as *const libc::sockaddr,
                    (addrs.len() - 1) as libc::c_int,
                    sys::SCTP_BINDX_ADD_ADDR,
                )
            } < 0
            {
                return Err(SctpError::Io(io::Error::last_os_error()));
            }
        }
        // A one-to-many socket must listen() to accept new associations, but you
        // never call accept() — associations surface via recv().
        if unsafe { libc::listen(fd, 128) } < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        association::set_nonblocking(fd)?;
        Ok(AsyncFd::new(ServerSocket { fd })?)
    }

    /// Receive the next message from any association (data or notification).
    pub async fn recv(&self) -> Result<ServerMessage, SctpError> {
        let mut buf = vec![0u8; 65536];
        loop {
            let mut guard = self.inner.readable().await?;

            let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
            let mut fromlen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            let mut sinfo = sys::SctpSndRcvInfo::default();
            let mut msg_flags: libc::c_int = 0;

            let ret = unsafe {
                sys::sctp_recvmsg(
                    self.inner.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    &mut storage as *mut _ as *mut libc::sockaddr,
                    &mut fromlen,
                    &mut sinfo,
                    &mut msg_flags,
                )
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                }
                return Err(SctpError::Io(err));
            }
            let len = ret as usize;
            if msg_flags & sys::MSG_NOTIFICATION != 0 {
                return Ok(ServerMessage::Notification(
                    crate::notification::parse_notification(&buf[..len]),
                ));
            }
            buf.truncate(len);
            let info = RecvInfo {
                stream: sinfo.sinfo_stream,
                ppid: u32::from_be(sinfo.sinfo_ppid),
                assoc_id: sinfo.sinfo_assoc_id,
            };
            let addr = addr::from_raw(&storage)?;
            return Ok(ServerMessage::Data {
                data: buf,
                info,
                addr,
            });
        }
    }

    /// Send to a specific association by id, on `stream` with `ppid`.
    pub async fn send(
        &self,
        assoc_id: i32,
        data: &[u8],
        stream: u16,
        ppid: u32,
    ) -> Result<usize, SctpError> {
        self.send_with(assoc_id, data, stream, ppid, &SendOptions::default())
            .await
    }

    /// Send to a specific association with explicit [`SendOptions`].
    pub async fn send_with(
        &self,
        assoc_id: i32,
        data: &[u8],
        stream: u16,
        ppid: u32,
        opts: &SendOptions,
    ) -> Result<usize, SctpError> {
        loop {
            let mut guard = self.inner.writable().await?;
            let sinfo = sys::SctpSndRcvInfo {
                sinfo_stream: stream,
                sinfo_flags: opts.sinfo_flags() as u16,
                sinfo_ppid: ppid.to_be(),
                sinfo_timetolive: opts.ttl_ms,
                sinfo_assoc_id: assoc_id,
                ..Default::default()
            };
            let ret = unsafe {
                sys::sctp_send(
                    self.inner.as_raw_fd(),
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                    &sinfo,
                    0,
                )
            };
            if ret < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::WouldBlock {
                    guard.clear_ready();
                    continue;
                }
                return Err(SctpError::Io(err));
            }
            return Ok(ret as usize);
        }
    }

    /// Branch an association off this socket into its own one-to-one
    /// [`SctpAssociation`] (`sctp_peeloff`).
    pub fn peeloff(&self, assoc_id: i32) -> Result<SctpAssociation, SctpError> {
        let new_fd = unsafe { sys::sctp_peeloff(self.inner.as_raw_fd(), assoc_id) };
        if new_fd < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        SctpAssociation::from_raw_fd(new_fd)
    }

    /// The local (primary) address this socket is bound to.
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
