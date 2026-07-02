use std::fmt;

use crate::ppid;

/// Metadata about a received SCTP message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecvInfo {
    /// The stream the message arrived on.
    pub stream: u16,
    /// The Payload Protocol Identifier (opaque; see [`crate::ppid`] for names).
    pub ppid: u32,
    /// The association the message belongs to (meaningful on one-to-many sockets).
    pub assoc_id: i32,
}

impl fmt::Display for RecvInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "RecvInfo [stream={}, ppid={}, assoc_id={}]",
            self.stream,
            ppid::display(self.ppid),
            self.assoc_id
        )
    }
}
