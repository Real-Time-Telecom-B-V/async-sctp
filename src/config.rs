//! Socket configuration applied before an association is set up.
//!
//! SCTP negotiates parameters (notably the number of streams) at INIT time, so
//! these must be set on the socket *before* `connect`/`listen`. Every field is
//! optional — `None` leaves the kernel default — and the setters are fluent:
//!
//! ```no_run
//! use async_sctp::SctpConfig;
//! let cfg = SctpConfig::new().streams(30, 65535).nodelay(true);
//! ```

use std::io;
use std::os::unix::io::RawFd;

use crate::error::SctpError;
use crate::sys;

/// SCTP INIT parameters (`SCTP_INITMSG`), negotiated at association setup.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InitMsg {
    /// Number of outbound streams to request (`sinit_num_ostreams`).
    pub out_streams: u16,
    /// Maximum inbound streams to allow (`sinit_max_instreams`).
    pub max_in_streams: u16,
    /// Maximum INIT (re)transmit attempts (`sinit_max_attempts`, 0 = default).
    pub max_attempts: u16,
    /// Maximum INIT timeout in milliseconds (`sinit_max_init_timeo`, 0 = default).
    pub max_init_timeo: u16,
}

/// Configuration applied to an SCTP socket before `connect`/`listen`.
///
/// All fields are optional; `None` leaves the kernel default in place.
#[derive(Debug, Clone, Default)]
pub struct SctpConfig {
    /// INIT parameters (stream counts, INIT retransmit tuning).
    pub init: Option<InitMsg>,
    /// `SCTP_NODELAY` — disable Nagle-style bundling for lower latency.
    pub nodelay: Option<bool>,
    /// `SO_RCVBUF` in bytes.
    pub recv_buf: Option<usize>,
    /// `SO_SNDBUF` in bytes.
    pub send_buf: Option<usize>,
    /// `SCTP_AUTOCLOSE` — auto-close idle associations after N seconds
    /// (one-to-many sockets only; 0 disables).
    pub autoclose_secs: Option<u32>,
}

impl SctpConfig {
    /// A config that changes nothing (all kernel defaults).
    pub fn new() -> Self {
        Self::default()
    }

    /// Request `out` outbound streams and allow up to `max_in` inbound.
    ///
    /// This is the important one for protocols that fan work across streams
    /// (NGAP/S1AP use stream 0 for non-UE signalling and additional streams
    /// per UE). The kernel default is only a handful of streams.
    pub fn streams(mut self, out: u16, max_in: u16) -> Self {
        let i = self.init.get_or_insert(InitMsg::default());
        i.out_streams = out;
        i.max_in_streams = max_in;
        self
    }

    /// Tune INIT (re)transmission: up to `attempts` attempts, each capped at
    /// `timeo_ms` milliseconds.
    pub fn init_retransmit(mut self, attempts: u16, timeo_ms: u16) -> Self {
        let i = self.init.get_or_insert(InitMsg::default());
        i.max_attempts = attempts;
        i.max_init_timeo = timeo_ms;
        self
    }

    /// Set `SCTP_NODELAY` (disable Nagle-style bundling).
    pub fn nodelay(mut self, on: bool) -> Self {
        self.nodelay = Some(on);
        self
    }

    /// Set the socket receive buffer (`SO_RCVBUF`) in bytes.
    pub fn recv_buf(mut self, bytes: usize) -> Self {
        self.recv_buf = Some(bytes);
        self
    }

    /// Set the socket send buffer (`SO_SNDBUF`) in bytes.
    pub fn send_buf(mut self, bytes: usize) -> Self {
        self.send_buf = Some(bytes);
        self
    }

    /// Auto-close idle associations after `secs` seconds (one-to-many only).
    pub fn autoclose(mut self, secs: u32) -> Self {
        self.autoclose_secs = Some(secs);
        self
    }

    /// Apply the configured options to a freshly created socket fd. Called
    /// internally before `bind`/`connect`.
    pub(crate) fn apply(&self, fd: RawFd) -> Result<(), SctpError> {
        if let Some(init) = self.init {
            let im = sys::SctpInitMsg {
                sinit_num_ostreams: init.out_streams,
                sinit_max_instreams: init.max_in_streams,
                sinit_max_attempts: init.max_attempts,
                sinit_max_init_timeo: init.max_init_timeo,
            };
            setsockopt(fd, sys::SOL_SCTP, sys::SCTP_INITMSG, &im)?;
        }
        if let Some(on) = self.nodelay {
            let v: libc::c_int = on as libc::c_int;
            setsockopt(fd, sys::SOL_SCTP, sys::SCTP_NODELAY, &v)?;
        }
        if let Some(b) = self.recv_buf {
            let v = b as libc::c_int;
            setsockopt(fd, libc::SOL_SOCKET, libc::SO_RCVBUF, &v)?;
        }
        if let Some(b) = self.send_buf {
            let v = b as libc::c_int;
            setsockopt(fd, libc::SOL_SOCKET, libc::SO_SNDBUF, &v)?;
        }
        if let Some(secs) = self.autoclose_secs {
            let v = secs as libc::c_int;
            setsockopt(fd, sys::SOL_SCTP, sys::SCTP_AUTOCLOSE, &v)?;
        }
        Ok(())
    }
}

/// Typed `setsockopt` for a plain `#[repr(C)]`/scalar option value.
fn setsockopt<T>(
    fd: RawFd,
    level: libc::c_int,
    opt: libc::c_int,
    val: &T,
) -> Result<(), SctpError> {
    let ret = unsafe {
        libc::setsockopt(
            fd,
            level,
            opt,
            val as *const T as *const libc::c_void,
            std::mem::size_of::<T>() as libc::socklen_t,
        )
    };
    if ret < 0 {
        return Err(SctpError::Io(io::Error::last_os_error()));
    }
    Ok(())
}
