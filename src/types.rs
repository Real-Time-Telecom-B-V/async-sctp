use std::fmt;

/// Well-known SCTP Payload Protocol Identifiers (PPIDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PayloadProtocolId {
    /// No protocol identifier specified.
    None = 0,
    /// IUA (ISDN Q.921 User Adaptation)
    Iua = 1,
    /// M2UA (MTP2 User Adaptation)
    M2ua = 2,
    /// M3UA (MTP3 User Adaptation)
    M3ua = 3,
    /// SUA (SCCP User Adaptation)
    Sua = 4,
    /// M2PA (MTP2 Peer Adaptation)
    M2pa = 5,
    /// V5UA (V5.2 User Adaptation)
    V5ua = 6,
}

impl PayloadProtocolId {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Iua),
            2 => Some(Self::M2ua),
            3 => Some(Self::M3ua),
            4 => Some(Self::Sua),
            5 => Some(Self::M2pa),
            6 => Some(Self::V5ua),
            _ => Option::None,
        }
    }
}

impl fmt::Display for PayloadProtocolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::None => write!(f, "None(0)"),
            Self::Iua => write!(f, "IUA(1)"),
            Self::M2ua => write!(f, "M2UA(2)"),
            Self::M3ua => write!(f, "M3UA(3)"),
            Self::Sua => write!(f, "SUA(4)"),
            Self::M2pa => write!(f, "M2PA(5)"),
            Self::V5ua => write!(f, "V5UA(6)"),
        }
    }
}

/// Information about a received SCTP message.
#[derive(Debug, Clone)]
pub struct RecvInfo {
    /// The stream number the message was received on.
    pub stream: u16,
    /// The Payload Protocol Identifier.
    pub ppid: u32,
    /// The association ID.
    pub assoc_id: i32,
}

impl fmt::Display for RecvInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ppid_name = PayloadProtocolId::from_u32(self.ppid)
            .map(|p| format!("{p}"))
            .unwrap_or_else(|| format!("Unknown({0})", self.ppid));
        write!(
            f,
            "RecvInfo [stream={}, ppid={}, assoc_id={}]",
            self.stream, ppid_name, self.assoc_id
        )
    }
}
