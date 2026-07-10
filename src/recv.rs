//! Message reassembly shared by the one-to-one association and the one-to-many
//! server.
//!
//! SCTP is message-oriented, but a single `sctp_recvmsg` can return a *partial*
//! message: the read buffer is smaller than the message, or
//! `SCTP_PARTIAL_DELIVERY_POINT` is reached under receive-buffer pressure while the
//! message is still being reassembled. The kernel then delivers what it has with
//! `MSG_EOR` clear and keeps the rest for the next call. A message is complete only
//! once a read returns `MSG_EOR` (RFC 6458 §5.1), so the reader must accumulate
//! across calls until then.
//!
//! [`Reassembly`] holds that accumulation state *in the socket object* (not in the
//! `recv` future), which buys two things:
//! - a notification that interleaves between two data fragments (possible with the
//!   Linux-default `SCTP_FRAGMENT_INTERLEAVE`, RFC 6458 §6.2) is returned as its own
//!   record without discarding the half-read data message, and
//! - `recv` is cancel-safe: a future dropped mid-reassembly leaves the leading
//!   fragment buffered, so the next `recv` resumes instead of desyncing the stream.

use std::io;
use std::net::SocketAddr;
use std::os::unix::io::RawFd;

use libc::c_int;

use crate::addr;
use crate::error::SctpError;
use crate::notification::{self, Notification};
use crate::sys;
use crate::types::RecvInfo;

/// Bytes consumed per `sctp_recvmsg`. A message larger than this comes back over
/// several reads (each MSG_EOR-clear until the last), which is exactly the path
/// [`Reassembly`] exists to stitch back together.
pub(crate) const RECV_WINDOW: usize = 64 * 1024;

/// Accumulation state for one socket's in-progress message. Lives in the
/// `SctpAssociation` / `SctpServer` so it outlives any single `recv` call.
#[derive(Default)]
pub(crate) struct Reassembly {
    /// Payload of the message currently being reassembled (empty between messages).
    buf: Vec<u8>,
    /// Reusable read buffer, sized to `RECV_WINDOW` on first use and kept.
    scratch: Vec<u8>,
}

/// Outcome of a single `sctp_recvmsg` folded through [`Reassembly::recv_step`].
pub(crate) enum Step {
    /// The socket had nothing to read (EWOULDBLOCK) — wait for readiness.
    WouldBlock,
    /// A data fragment was committed but the message is not complete yet — read on.
    More,
    /// A notification (its own atomic record); any half-read data is preserved.
    Notification(Notification),
    /// A complete message: payload, its info, and the peer address (one-to-many).
    Complete(Vec<u8>, RecvInfo, Option<SocketAddr>),
    /// A terminal error (I/O error, or peer shutdown on a `ret == 0` read).
    Err(SctpError),
}

impl Reassembly {
    /// Perform one `sctp_recvmsg` and fold the result into the accumulator.
    ///
    /// `want_addr` fills the peer address (needed by the one-to-many server; the
    /// one-to-one association leaves it `None`).
    pub(crate) fn recv_step(&mut self, fd: RawFd, want_addr: bool) -> Step {
        if self.scratch.len() < RECV_WINDOW {
            self.scratch.resize(RECV_WINDOW, 0);
        }

        let mut sinfo = sys::SctpSndRcvInfo::default();
        let mut msg_flags: c_int = 0;
        let mut storage: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
        let mut fromlen = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        let (from_ptr, fromlen_ptr) = if want_addr {
            (
                &mut storage as *mut libc::sockaddr_storage as *mut libc::sockaddr,
                &mut fromlen as *mut libc::socklen_t,
            )
        } else {
            (std::ptr::null_mut(), std::ptr::null_mut())
        };

        let ret = unsafe {
            sys::sctp_recvmsg(
                fd,
                self.scratch.as_mut_ptr() as *mut libc::c_void,
                RECV_WINDOW,
                from_ptr,
                fromlen_ptr,
                &mut sinfo,
                &mut msg_flags,
            )
        };
        if ret < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::WouldBlock {
                return Step::WouldBlock;
            }
            return Step::Err(SctpError::Io(err));
        }
        if ret == 0 {
            return Step::Err(SctpError::PeerShutdown);
        }
        let n = ret as usize;

        // The peer address is only meaningful for one-to-many data; `.ok()` keeps a
        // notification whose `from` the kernel left unset from becoming an error.
        let addr = if want_addr {
            addr::from_raw(&storage).ok()
        } else {
            None
        };

        // Split the borrow so the read window and the accumulator reference distinct
        // fields: `push_chunk` gets `&scratch[..n]` and `&mut buf` at the same time.
        let Reassembly { buf, scratch } = self;
        push_chunk(buf, &scratch[..n], msg_flags, &sinfo, addr)
    }
}

/// Classify and accumulate one received chunk. Pure (no socket), so the reassembly
/// and interleaved-notification behaviour is unit-testable directly.
fn push_chunk(
    buf: &mut Vec<u8>,
    chunk: &[u8],
    msg_flags: c_int,
    sinfo: &sys::SctpSndRcvInfo,
    addr: Option<SocketAddr>,
) -> Step {
    if msg_flags & sys::MSG_NOTIFICATION != 0 {
        // A notification is its own record; leave any half-read data message alone.
        return Step::Notification(notification::parse_notification(chunk));
    }

    buf.extend_from_slice(chunk);

    if msg_flags & sys::MSG_EOR != 0 {
        // End of record: hand off the complete message and reset for the next one.
        let info = RecvInfo {
            stream: sinfo.sinfo_stream,
            ppid: u32::from_be(sinfo.sinfo_ppid),
            assoc_id: sinfo.sinfo_assoc_id,
        };
        Step::Complete(std::mem::take(buf), info, addr)
    } else {
        Step::More
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ppid;

    fn sinfo(stream: u16, ppid: u32, assoc_id: i32) -> sys::SctpSndRcvInfo {
        sys::SctpSndRcvInfo {
            sinfo_stream: stream,
            sinfo_ppid: ppid.to_be(), // travels network-order, decoded by push_chunk
            sinfo_assoc_id: assoc_id,
            ..Default::default()
        }
    }

    // A data message split across two fragments, with a notification delivered
    // between them, must reassemble to the concatenation of the fragments while the
    // notification surfaces as its own record and does not disturb the accumulator.
    #[test]
    fn reassembles_across_interleaved_notification() {
        let mut buf = Vec::new();
        let si = sinfo(7, ppid::S1AP, 42);

        let frag_a = vec![0xAAu8; 100];
        match push_chunk(&mut buf, &frag_a, 0, &si, None) {
            Step::More => {}
            _ => panic!("first fragment (no EOR) should be More"),
        }
        assert_eq!(buf, frag_a, "fragment A is buffered");

        // A notification interleaves; the half-read data message must be untouched.
        let notif = [0u8; 4]; // shorter than a header -> Unknown, but still a notification
        match push_chunk(&mut buf, &notif, sys::MSG_NOTIFICATION, &si, None) {
            Step::Notification(_) => {}
            _ => panic!("a MSG_NOTIFICATION chunk should surface as a Notification"),
        }
        assert_eq!(
            buf, frag_a,
            "data preserved across the interleaved notification"
        );

        let frag_b = vec![0xBBu8; 50];
        match push_chunk(&mut buf, &frag_b, sys::MSG_EOR, &si, None) {
            Step::Complete(data, info, addr) => {
                let mut expected = frag_a.clone();
                expected.extend_from_slice(&frag_b);
                assert_eq!(data, expected, "reassembled payload = A ++ B");
                assert_eq!(info.stream, 7);
                assert_eq!(info.ppid, ppid::S1AP);
                assert_eq!(info.assoc_id, 42);
                assert!(addr.is_none());
            }
            _ => panic!("the EOR fragment should complete the message"),
        }
        assert!(
            buf.is_empty(),
            "accumulator is reset after a complete message"
        );
    }

    // The common case: a single-chunk message (EOR set on the first read) completes
    // in one step with no accumulation.
    #[test]
    fn single_chunk_message_completes_immediately() {
        let mut buf = Vec::new();
        let si = sinfo(0, ppid::M3UA, 1);
        match push_chunk(&mut buf, b"hello", sys::MSG_EOR, &si, None) {
            Step::Complete(data, info, _) => {
                assert_eq!(data, b"hello");
                assert_eq!(info.ppid, ppid::M3UA);
            }
            _ => panic!("a lone EOR chunk should complete immediately"),
        }
        assert!(buf.is_empty());
    }
}
