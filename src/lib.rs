//! Async Rust wrapper around Linux kernel SCTP (lksctp).
//!
//! Provides `SctpListener` and `SctpAssociation` types for one-to-one style
//! SCTP communication, built on top of the Linux kernel SCTP stack via `libc`
//! and `tokio`'s `AsyncFd`.
//!
//! # Example
//!
//! ```rust,no_run
//! use sctp::{SctpListener, SctpAssociation};
//! use std::net::SocketAddr;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), sctp::SctpError> {
//!     // Server side
//!     let addr: SocketAddr = "127.0.0.1:3868".parse().unwrap();
//!     let listener = SctpListener::bind(addr)?;
//!     let (assoc, peer) = listener.accept().await?;
//!     println!("Accepted connection from {peer}");
//!
//!     // Receive on any stream
//!     let (data, info) = assoc.recv().await?;
//!     println!("Received {} bytes on stream {}", data.len(), info.stream);
//!     Ok(())
//! }
//! ```
//!
//! # Requirements
//!
//! - Linux with SCTP kernel module loaded (`modprobe sctp`)
//! - `libsctp-dev` (Debian/Ubuntu) or `lksctp-tools-devel` (RHEL/Fedora)

pub mod association;
pub mod error;
pub mod listener;
pub mod notification;
pub mod sys;
pub mod types;

pub use association::{RecvResult, SctpAssociation};
pub use error::SctpError;
pub use listener::SctpListener;
pub use notification::{AssocChangeEvent, AssocChangeState, Notification};
pub use types::{PayloadProtocolId, RecvInfo};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppid_round_trip() {
        assert_eq!(
            PayloadProtocolId::from_u32(3),
            Some(PayloadProtocolId::M3ua)
        );
        assert_eq!(
            PayloadProtocolId::from_u32(5),
            Some(PayloadProtocolId::M2pa)
        );
        assert_eq!(PayloadProtocolId::from_u32(99), None);
    }

    #[test]
    fn ppid_display() {
        assert_eq!(format!("{}", PayloadProtocolId::M3ua), "M3UA(3)");
        assert_eq!(format!("{}", PayloadProtocolId::M2pa), "M2PA(5)");
    }

    #[test]
    fn recv_info_display() {
        let info = RecvInfo {
            stream: 1,
            ppid: 3,
            assoc_id: 42,
        };
        assert_eq!(
            format!("{info}"),
            "RecvInfo [stream=1, ppid=M3UA(3), assoc_id=42]"
        );
    }

    #[test]
    fn assoc_change_state_display() {
        assert_eq!(format!("{}", AssocChangeState::CommUp), "COMM_UP");
        assert_eq!(format!("{}", AssocChangeState::CommLost), "COMM_LOST");
        assert_eq!(format!("{}", AssocChangeState::Unknown(99)), "UNKNOWN(99)");
    }

    #[test]
    fn notification_parse_too_short() {
        let buf = [0u8; 2];
        let notif = notification::parse_notification(&buf);
        matches!(notif, Notification::Unknown { sn_type: _ });
    }

    #[test]
    fn notification_parse_unknown_type() {
        let mut buf = [0u8; 8];
        // Set type to some unknown value
        buf[0] = 0xFF;
        buf[1] = 0xFF;
        let notif = notification::parse_notification(&buf);
        match notif {
            Notification::Unknown { sn_type } => assert_eq!(sn_type, 0xFFFF),
            _ => panic!("expected Unknown notification"),
        }
    }
}
