use std::io;
use std::net::SocketAddr;
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Mutex;

use tokio::io::unix::AsyncFd;

use crate::addr;
use crate::config::SctpConfig;
use crate::error::SctpError;
use crate::notification::Notification;
use crate::recv::{Reassembly, Step};
use crate::sys;
use crate::types::RecvInfo;

/// An SCTP association (a one-to-one connection) wrapping a kernel SCTP socket.
///
/// Obtain one from [`SctpListener::accept`](crate::SctpListener::accept),
/// [`connect`](Self::connect), or by peeling one off a
/// [`SctpServer`](crate::SctpServer).
pub struct SctpAssociation {
    inner: AsyncFd<SctpSocket>,
    /// Cross-call reassembly buffer so `recv`/`recv_msg` accumulate a partially
    /// delivered message until `MSG_EOR` and stay cancel-safe (see [`crate::recv`]).
    recv_state: Mutex<Reassembly>,
}

/// Owns the socket fd and closes it on drop.
pub(crate) struct SctpSocket {
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

/// Per-message send options. Default = reliable, ordered delivery.
#[derive(Debug, Clone, Copy, Default)]
pub struct SendOptions {
    /// Deliver this message unordered (`SCTP_UNORDERED`).
    pub unordered: bool,
    /// PR-SCTP timed reliability: discard the message if it can't be delivered
    /// within this many milliseconds (0 = fully reliable, the default).
    pub ttl_ms: u32,
    /// Send an ABORT for the association (the payload becomes the abort cause).
    pub abort: bool,
    /// Initiate a graceful shutdown after this message (`SCTP_EOF`).
    pub eof: bool,
}

impl SendOptions {
    /// Reliable, ordered (the default).
    pub fn new() -> Self {
        Self::default()
    }
    /// Deliver unordered.
    pub fn unordered(mut self, v: bool) -> Self {
        self.unordered = v;
        self
    }
    /// PR-SCTP timed reliability (discard after `ms` milliseconds; 0 = reliable).
    pub fn ttl_ms(mut self, ms: u32) -> Self {
        self.ttl_ms = ms;
        self
    }

    pub(crate) fn sinfo_flags(&self) -> u32 {
        let mut f = 0;
        if self.unordered {
            f |= sys::SCTP_UNORDERED;
        }
        if self.abort {
            f |= sys::SCTP_ABORT;
        }
        if self.eof {
            f |= sys::SCTP_EOF;
        }
        f
    }
}

impl SctpAssociation {
    /// Wrap an already-connected raw fd (from `accept`/`peeloff`), set it
    /// non-blocking, subscribe to events, and register it with tokio.
    pub(crate) fn from_raw_fd(fd: RawFd) -> Result<Self, SctpError> {
        set_nonblocking(fd)?;
        configure_events(fd)?;
        let inner = AsyncFd::new(SctpSocket { fd })?;
        Ok(Self {
            inner,
            recv_state: Mutex::new(Reassembly::default()),
        })
    }

    /// Connect to a remote SCTP endpoint (kernel defaults).
    pub async fn connect(addr: SocketAddr) -> Result<Self, SctpError> {
        Self::connect_impl(&[addr], &SctpConfig::default()).await
    }

    /// Connect with an explicit [`SctpConfig`] (stream counts, sockopts).
    pub async fn connect_with(addr: SocketAddr, config: &SctpConfig) -> Result<Self, SctpError> {
        Self::connect_impl(&[addr], config).await
    }

    /// Connect to a multihomed peer across several addresses (`sctp_connectx`),
    /// which may mix IPv4 and IPv6.
    pub async fn connect_multi(addrs: &[SocketAddr]) -> Result<Self, SctpError> {
        Self::connect_impl(addrs, &SctpConfig::default()).await
    }

    /// Multihomed connect with an explicit [`SctpConfig`].
    pub async fn connect_multi_with(
        addrs: &[SocketAddr],
        config: &SctpConfig,
    ) -> Result<Self, SctpError> {
        Self::connect_impl(addrs, config).await
    }

    async fn connect_impl(addrs: &[SocketAddr], config: &SctpConfig) -> Result<Self, SctpError> {
        if addrs.is_empty() {
            return Err(SctpError::Io(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no connect addresses",
            )));
        }
        let domain = if addrs.iter().any(SocketAddr::is_ipv6) {
            libc::AF_INET6
        } else {
            libc::AF_INET
        };

        let fd = unsafe { libc::socket(domain, libc::SOCK_STREAM, sys::IPPROTO_SCTP) };
        if fd < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        // From here on, close the fd on any early return.
        let guard = FdGuard(fd);

        set_nonblocking(fd)?;
        if domain == libc::AF_INET6 {
            set_v6only(fd, false);
        }
        config.apply(fd)?;
        configure_events(fd)?;

        let inner = AsyncFd::new(SctpSocket { fd })?;
        guard.disarm(); // ownership moved into SctpSocket

        let packed = addr::pack(addrs);
        let mut assoc_id: i32 = 0;
        let ret = unsafe {
            sys::sctp_connectx(
                inner.as_raw_fd(),
                packed.as_ptr() as *const libc::sockaddr,
                addrs.len() as libc::c_int,
                &mut assoc_id,
            )
        };
        if ret < 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(SctpError::Io(err));
            }
        }

        inner.writable().await?.retain_ready();
        check_so_error(inner.as_raw_fd())?;
        Ok(Self {
            inner,
            recv_state: Mutex::new(Reassembly::default()),
        })
    }

    /// Send `data` on `stream` with the given `ppid`, reliable + ordered.
    pub async fn send(&self, data: &[u8], stream: u16, ppid: u32) -> Result<usize, SctpError> {
        self.send_with(data, stream, ppid, &SendOptions::default())
            .await
    }

    /// Send with explicit [`SendOptions`] (unordered / PR-SCTP / abort / eof).
    pub async fn send_with(
        &self,
        data: &[u8],
        stream: u16,
        ppid: u32,
        opts: &SendOptions,
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
                    ppid.to_be(), // PPID travels in network byte order
                    opts.sinfo_flags(),
                    stream,
                    opts.ttl_ms,
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

    /// Receive the next *complete* application message, returning the payload and
    /// its [`RecvInfo`] (stream, ppid, assoc_id). A message fragmented across more
    /// than one `sctp_recvmsg` is reassembled internally (accumulated until
    /// `MSG_EOR`), so the caller always gets whole messages. SCTP notifications
    /// (COMM_UP, COMM_LOST, …) are skipped transparently — use
    /// [`recv_msg`](Self::recv_msg) if you need to observe them.
    ///
    /// Reassembling a large message spans several syscalls across `await`, but the
    /// accumulator lives in the association, so `recv` is cancel-safe: a future
    /// dropped mid-reassembly (e.g. it lost a `tokio::select!` race) leaves the
    /// leading fragment buffered and the next `recv` resumes it.
    pub async fn recv(&self) -> Result<(Vec<u8>, RecvInfo), SctpError> {
        loop {
            match self.recv_msg().await? {
                RecvResult::Data(data, info) => return Ok((data, info)),
                RecvResult::Notification(_) => continue,
            }
        }
    }

    /// Receive either a complete application message or an SCTP notification. Like
    /// [`recv`](Self::recv), a fragmented data message is reassembled until
    /// `MSG_EOR` before it is returned; a notification that interleaves between two
    /// fragments is returned as its own [`RecvResult::Notification`] without losing
    /// the partially accumulated data (the next call continues it).
    pub async fn recv_msg(&self) -> Result<RecvResult, SctpError> {
        loop {
            let mut guard = self.inner.readable().await?;

            // Do one syscall + accumulate under the lock, but never hold the lock
            // across an `await`: the only cancel point is `readable()` above, where
            // committed fragments already live in `recv_state`.
            let step = {
                let mut state = self
                    .recv_state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.recv_step(self.inner.as_raw_fd(), false)
            };

            match step {
                Step::WouldBlock => {
                    guard.clear_ready();
                    continue;
                }
                // Fragment committed; keep the retained readiness and read the next.
                Step::More => continue,
                Step::Notification(n) => return Ok(RecvResult::Notification(n)),
                Step::Complete(data, info, _addr) => return Ok(RecvResult::Data(data, info)),
                Step::Err(e) => return Err(e),
            }
        }
    }

    /// The peer (remote) addresses of this association — more than one if the
    /// peer is multihomed (`sctp_getpaddrs`).
    pub fn peer_addrs(&self) -> Result<Vec<SocketAddr>, SctpError> {
        addrs_via(self.inner.as_raw_fd(), 0, true)
    }

    /// The local addresses bound to this association (`sctp_getladdrs`).
    pub fn local_addrs(&self) -> Result<Vec<SocketAddr>, SctpError> {
        addrs_via(self.inner.as_raw_fd(), 0, false)
    }

    /// Gracefully shut down the association (SCTP SHUTDOWN handshake).
    pub async fn shutdown(&self) -> Result<(), SctpError> {
        let ret = unsafe { libc::shutdown(self.inner.as_raw_fd(), libc::SHUT_WR) };
        if ret < 0 {
            return Err(SctpError::Io(io::Error::last_os_error()));
        }
        Ok(())
    }

    /// Abort the association immediately (SCTP ABORT, no shutdown handshake).
    pub async fn abort(&self) -> Result<(), SctpError> {
        let opts = SendOptions {
            abort: true,
            ..Default::default()
        };
        self.send_with(&[], 0, 0, &opts).await.map(|_| ())
    }

    /// The raw file descriptor of the underlying socket.
    pub fn as_raw_fd(&self) -> RawFd {
        self.inner.as_raw_fd()
    }
}

/// Result of receiving on an association: application data or a notification.
#[derive(Debug)]
pub enum RecvResult {
    /// Application data with receive info.
    Data(Vec<u8>, RecvInfo),
    /// An SCTP notification.
    Notification(Notification),
}

// ── shared fd helpers ───────────────────────────────────────────────────────

/// Closes a raw fd on drop unless disarmed — for cleanup on the error paths
/// between `socket()` and handing the fd to an owner.
struct FdGuard(RawFd);
impl FdGuard {
    fn disarm(mut self) {
        self.0 = -1;
        std::mem::forget(self);
    }
}
impl Drop for FdGuard {
    fn drop(&mut self) {
        if self.0 >= 0 {
            unsafe { libc::close(self.0) };
        }
    }
}

pub(crate) fn set_nonblocking(fd: RawFd) -> Result<(), SctpError> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    Ok(())
}

pub(crate) fn set_v6only(fd: RawFd, on: bool) {
    let v: libc::c_int = on as libc::c_int;
    unsafe {
        libc::setsockopt(
            fd,
            libc::IPPROTO_IPV6,
            libc::IPV6_V6ONLY,
            &v as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
}

/// Subscribe to the SCTP events we surface as notifications.
pub(crate) fn configure_events(fd: RawFd) -> Result<(), SctpError> {
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

pub(crate) fn check_so_error(fd: RawFd) -> Result<(), SctpError> {
    let mut err_val: libc::c_int = 0;
    let mut err_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    let ret = unsafe {
        libc::getsockopt(
            fd,
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
    Ok(())
}

/// Shared getpaddrs/getladdrs → `Vec<SocketAddr>`.
fn addrs_via(fd: RawFd, assoc_id: i32, peer: bool) -> Result<Vec<SocketAddr>, SctpError> {
    let mut ptr: *mut libc::sockaddr = std::ptr::null_mut();
    let n = unsafe {
        if peer {
            sys::sctp_getpaddrs(fd, assoc_id, &mut ptr)
        } else {
            sys::sctp_getladdrs(fd, assoc_id, &mut ptr)
        }
    };
    if n < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    let out = unsafe { addr::parse_array(ptr, n) };
    unsafe {
        if peer {
            sys::sctp_freepaddrs(ptr);
        } else {
            sys::sctp_freeladdrs(ptr);
        }
    }
    Ok(out)
}
