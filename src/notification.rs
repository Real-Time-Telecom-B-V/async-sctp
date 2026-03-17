use crate::sys;
use std::fmt;

/// SCTP notifications delivered via the notification mechanism.
#[derive(Debug, Clone)]
pub enum Notification {
    /// Association state change.
    AssocChange(AssocChangeEvent),
    /// Peer shutdown.
    Shutdown { assoc_id: i32 },
    /// Send failure.
    SendFailed { assoc_id: i32 },
    /// Unknown notification type.
    Unknown { sn_type: u16 },
}

/// Association change states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssocChangeState {
    CommUp,
    CommLost,
    Restart,
    ShutdownComp,
    CantStrAssoc,
    Unknown(u16),
}

impl From<u16> for AssocChangeState {
    fn from(value: u16) -> Self {
        match value {
            sys::SCTP_COMM_UP => Self::CommUp,
            sys::SCTP_COMM_LOST => Self::CommLost,
            sys::SCTP_RESTART => Self::Restart,
            sys::SCTP_SHUTDOWN_COMP => Self::ShutdownComp,
            sys::SCTP_CANT_STR_ASSOC => Self::CantStrAssoc,
            other => Self::Unknown(other),
        }
    }
}

impl fmt::Display for AssocChangeState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommUp => write!(f, "COMM_UP"),
            Self::CommLost => write!(f, "COMM_LOST"),
            Self::Restart => write!(f, "RESTART"),
            Self::ShutdownComp => write!(f, "SHUTDOWN_COMP"),
            Self::CantStrAssoc => write!(f, "CANT_STR_ASSOC"),
            Self::Unknown(v) => write!(f, "UNKNOWN({v})"),
        }
    }
}

/// Association change event details.
#[derive(Debug, Clone)]
pub struct AssocChangeEvent {
    pub state: AssocChangeState,
    pub error: u16,
    pub outbound_streams: u16,
    pub inbound_streams: u16,
    pub assoc_id: i32,
}

impl fmt::Display for AssocChangeEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AssocChange [state={}, error={}, out_streams={}, in_streams={}, assoc_id={}]",
            self.state, self.error, self.outbound_streams, self.inbound_streams, self.assoc_id
        )
    }
}

impl fmt::Display for Notification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssocChange(evt) => write!(f, "Notification::{evt}"),
            Self::Shutdown { assoc_id } => {
                write!(f, "Notification::Shutdown [assoc_id={assoc_id}]")
            }
            Self::SendFailed { assoc_id } => {
                write!(f, "Notification::SendFailed [assoc_id={assoc_id}]")
            }
            Self::Unknown { sn_type } => {
                write!(f, "Notification::Unknown [type=0x{sn_type:04x}]")
            }
        }
    }
}

/// Parse a notification from raw bytes received with MSG_NOTIFICATION flag.
pub fn parse_notification(buf: &[u8]) -> Notification {
    if buf.len() < std::mem::size_of::<sys::SctpNotificationHeader>() {
        return Notification::Unknown { sn_type: 0 };
    }

    let sn_type = u16::from_ne_bytes([buf[0], buf[1]]);

    match sn_type {
        sys::SCTP_ASSOC_CHANGE => {
            if buf.len() >= std::mem::size_of::<sys::SctpAssocChange>() {
                let sac =
                    unsafe { &*(buf.as_ptr() as *const sys::SctpAssocChange) };
                Notification::AssocChange(AssocChangeEvent {
                    state: AssocChangeState::from(sac.sac_state),
                    error: sac.sac_error,
                    outbound_streams: sac.sac_outbound_streams,
                    inbound_streams: sac.sac_inbound_streams,
                    assoc_id: sac.sac_assoc_id,
                })
            } else {
                Notification::Unknown { sn_type }
            }
        }
        sys::SCTP_SHUTDOWN_EVENT => {
            if buf.len() >= std::mem::size_of::<sys::SctpShutdownEvent>() {
                let sse =
                    unsafe { &*(buf.as_ptr() as *const sys::SctpShutdownEvent) };
                Notification::Shutdown {
                    assoc_id: sse.sse_assoc_id,
                }
            } else {
                Notification::Unknown { sn_type }
            }
        }
        sys::SCTP_SEND_FAILED => {
            // Extract assoc_id from offset 8 (after type + flags + length)
            let assoc_id = if buf.len() >= 12 {
                i32::from_ne_bytes([buf[8], buf[9], buf[10], buf[11]])
            } else {
                0
            };
            Notification::SendFailed { assoc_id }
        }
        _ => Notification::Unknown { sn_type },
    }
}
