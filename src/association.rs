use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};

use tokio::io::unix::AsyncFd;

use crate::error::SctpError;
use crate::notification::{self, Notification};
use crate::sys;
use crate::types::RecvInfo;

/// An SCTP association (connection) wrapping a kernel SCTP socket.
///
/// Provides async send/recv over a one-to-one style SCTP socket.
pub struct SctpAssociation {
    inner: AsyncFd<SctpSocket>,
}

/// Wrapper around a raw file descriptor for the SCTP socket.
struct SctpSocket {
    fd: RawFd,
}

impl AsRawFd for SctpSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Drop for SctpSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.fd);
        }
    }
}

impl SctpAssociation {
    /// Create an `SctpAssociation` from an already-connected raw file descriptor.
    ///
    /// The fd is set to non-blocking mode and wrapped in tokio's `AsyncFd`.
    pub(crate) fn from_raw_fd(fd: RawFd) -> Result<Self, SctpError> {
        set_nonblocking(fd)?;
        configure_events(fd)?;
        let socket = SctpSocket { fd };
        let inner = AsyncFd::new(socket)?;
        Ok(Self { inner })
    }

    /// Connect to a remote SCTP endpoint.
    pub async fn connect(addr: SocketAddr) -> Result<Self, SctpError> {
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

        set_nonblocking(fd)?;
        configure_events(fd)?;

        let socket = SctpSocket { fd };
        let inner = AsyncFd::new(socket)?;

        // Initiate non-blocking connect
        let (sockaddr, socklen) = socket_addr_to_raw(&addr);
        let ret = unsafe {
            libc::connect(inner.as_raw_fd(), &sockaddr as *const _ as *const libc::sockaddr, socklen)
        };

        if ret < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(SctpError::Io(err));
            }
        }

        // Wait for connect to complete
        inner.writable().await?.retain_ready();

        // Check for connect error
        let mut err_val: libc::c_int = 0;
        let mut err_len: libc::socklen_t =
            std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        let ret = unsafe {
            libc::getsockopt(
                inner.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                &mut err_val as *mut _ as *mut libc::c_void,
                &mut err_len,
            )
        };
        if ret < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        if err_val != 0 {
            return Err(SctpError::Io(io::Error::from_raw_os_error(err_val)));
        }

        Ok(Self { inner })
    }

    /// Send data on a specific stream with a Payload Protocol Identifier.
    pub async fn send(
        &self,
        data: &[u8],
        stream: u16,
        ppid: u32,
    ) -> Result<usize, SctpError> {
        loop {
            let mut guard = self.inner.writable().await?;

            let ret = unsafe {
                sys::sctp_sendmsg(
                    self.inner.as_raw_fd(),
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                    std::ptr::null(),
                    0,
                    ppid.to_be(), // PPID is in network byte order
                    0,            // flags
                    stream,
                    0, // timetolive
                    0, // context
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

    /// Receive data, returning the payload and receive info (stream, ppid, etc).
    ///
    /// If the received message is an SCTP notification (e.g. association change),
    /// it is returned as `Err(SctpError::Notification(...))`.
    pub async fn recv(&self) -> Result<(Vec<u8>, RecvInfo), SctpError> {
        let mut buf = vec![0u8; 65536];

        loop {
            let mut guard = self.inner.readable().await?;

            let mut sinfo = sys::SctpSndRcvInfo::default();
            let mut msg_flags: libc::c_int = 0;

            let ret = unsafe {
                sys::sctp_recvmsg(
                    self.inner.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
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

            if ret == 0 {
                return Err(SctpError::PeerShutdown);
            }

            let len = ret as usize;

            // Check if this is a notification
            if msg_flags & sys::MSG_NOTIFICATION != 0 {
                let notif = notification::parse_notification(&buf[..len]);
                return Err(SctpError::Notification(format!("{notif}")));
            }

            buf.truncate(len);

            let info = RecvInfo {
                stream: sinfo.sinfo_stream,
                ppid: u32::from_be(sinfo.sinfo_ppid), // PPID from network byte order
                assoc_id: sinfo.sinfo_assoc_id,
            };

            return Ok((buf, info));
        }
    }

    /// Receive data or a notification.
    ///
    /// Unlike `recv()`, this method returns notifications as `Ok(RecvResult::Notification(...))`
    /// instead of as errors, making it easier to handle both data and notifications.
    pub async fn recv_msg(&self) -> Result<RecvResult, SctpError> {
        let mut buf = vec![0u8; 65536];

        loop {
            let mut guard = self.inner.readable().await?;

            let mut sinfo = sys::SctpSndRcvInfo::default();
            let mut msg_flags: libc::c_int = 0;

            let ret = unsafe {
                sys::sctp_recvmsg(
                    self.inner.as_raw_fd(),
                    buf.as_mut_ptr() as *mut libc::c_void,
                    buf.len(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
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

            if ret == 0 {
                return Err(SctpError::PeerShutdown);
            }

            let len = ret as usize;

            if msg_flags & sys::MSG_NOTIFICATION != 0 {
                let notif = notification::parse_notification(&buf[..len]);
                return Ok(RecvResult::Notification(notif));
            }

            buf.truncate(len);

            let info = RecvInfo {
                stream: sinfo.sinfo_stream,
                ppid: u32::from_be(sinfo.sinfo_ppid),
                assoc_id: sinfo.sinfo_assoc_id,
            };

            return Ok(RecvResult::Data(buf, info));
        }
    }

    /// Gracefully shut down the association.
    pub async fn shutdown(&self) -> Result<(), SctpError> {
        let ret = unsafe {
            libc::shutdown(self.inner.as_raw_fd(), libc::SHUT_WR)
        };
        if ret < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Returns the raw file descriptor of the underlying socket.
    pub fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

/// Result of receiving a message, which can be either data or a notification.
#[derive(Debug)]
pub enum RecvResult {
    /// Application data with receive info.
    Data(Vec<u8>, RecvInfo),
    /// An SCTP notification.
    Notification(Notification),
}

/// Set a file descriptor to non-blocking mode.
fn set_nonblocking(fd: RawFd) -> Result<(), SctpError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    let ret = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
    if ret < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    Ok(())
}

/// Configure SCTP event subscriptions on a socket.
fn configure_events(fd: RawFd) -> Result<(), SctpError> {
    let events = sys::SctpEventSubscribe::default();
    let ret = unsafe {
        libc::setsockopt(
            fd,
            sys::SOL_SCTP,
            sys::SCTP_EVENTS,
            &events as *const _ as *const libc::c_void,
            std::mem::size_of::<sys::SctpEventSubscribe>() as libc::socklen_t,
        )
    };
    if ret < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    Ok(())
}

/// Convert a `SocketAddr` to a raw `libc::sockaddr_storage` and length.
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
