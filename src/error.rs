use std::io;

/// Errors that can occur during SCTP operations.
#[derive(Debug, thiserror::Error)]
pub enum SctpError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("association shutdown by peer")]
    PeerShutdown,

    #[error("received SCTP notification: {0}")]
    Notification(String),

    #[error("invalid address: {0}")]
    InvalidAddress(String),

    #[error("not connected")]
    NotConnected,
}
