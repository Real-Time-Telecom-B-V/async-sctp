//! SCTP Payload Protocol Identifiers (PPIDs).
//!
//! A PPID is an opaque `u32` carried per message on the wire — `async-sctp`
//! never interprets it, it just passes it through `send`/`recv`. These constants
//! and [`name`] cover the well-known values from the IANA SCTP PPID registry so
//! callers can label traffic (SIGTRAN, 3GPP RAN, Diameter, …) without hard-coding
//! magic numbers. Unknown PPIDs are perfectly valid — they simply have no name.

/// Reserved / unspecified.
pub const UNSPECIFIED: u32 = 0;

// ── SIGTRAN adaptation layers ───────────────────────────────────────────────
/// ISDN Q.921 User Adaptation.
pub const IUA: u32 = 1;
/// MTP2 User Adaptation.
pub const M2UA: u32 = 2;
/// MTP3 User Adaptation.
pub const M3UA: u32 = 3;
/// SCCP User Adaptation.
pub const SUA: u32 = 4;
/// MTP2 Peer-to-Peer Adaptation.
pub const M2PA: u32 = 5;
/// V5.2 User Adaptation.
pub const V5UA: u32 = 6;
/// H.248 / MEGACO.
pub const H248: u32 = 7;
/// BICC / Q.2150.3.
pub const BICC: u32 = 8;
/// TALI.
pub const TALI: u32 = 9;
/// DPNSS/DASS2 User Adaptation.
pub const DUA: u32 = 10;
/// Aggregate Server Access Protocol.
pub const ASAP: u32 = 11;
/// Endpoint Handlespace Redundancy Protocol.
pub const ENRP: u32 = 12;

// ── 3GPP RAN application protocols ──────────────────────────────────────────
/// S1 Application Protocol (LTE S1-MME).
pub const S1AP: u32 = 18;
/// X2 Application Protocol (LTE inter-eNB).
pub const X2AP: u32 = 27;
/// NG Application Protocol (5G N2, gNB↔AMF).
pub const NGAP: u32 = 60;
/// Xn Application Protocol (5G inter-gNB).
pub const XNAP: u32 = 61;
/// F1 Application Protocol (5G CU↔DU).
pub const F1AP: u32 = 62;
/// E1 Application Protocol (5G CU-CP↔CU-UP).
pub const E1AP: u32 = 64;

// ── Diameter ────────────────────────────────────────────────────────────────
/// Diameter in an SCTP DATA chunk.
pub const DIAMETER: u32 = 46;
/// Diameter in a DTLS/SCTP DATA chunk.
pub const DIAMETER_DTLS: u32 = 47;

/// The registered name for a well-known PPID, if any (`None` for unknown values,
/// which are still valid on the wire).
pub fn name(ppid: u32) -> Option<&'static str> {
    Some(match ppid {
        UNSPECIFIED => "UNSPECIFIED",
        IUA => "IUA",
        M2UA => "M2UA",
        M3UA => "M3UA",
        SUA => "SUA",
        M2PA => "M2PA",
        V5UA => "V5UA",
        H248 => "H248",
        BICC => "BICC",
        TALI => "TALI",
        DUA => "DUA",
        ASAP => "ASAP",
        ENRP => "ENRP",
        S1AP => "S1AP",
        X2AP => "X2AP",
        NGAP => "NGAP",
        XNAP => "XNAP",
        F1AP => "F1AP",
        E1AP => "E1AP",
        DIAMETER => "DIAMETER",
        DIAMETER_DTLS => "DIAMETER_DTLS",
        _ => return None,
    })
}

/// Format a PPID as `NAME(n)` for known values, or `PPID(n)` otherwise.
pub fn display(ppid: u32) -> String {
    match name(ppid) {
        Some(n) => format!("{n}({ppid})"),
        None => format!("PPID({ppid})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_known_names() {
        assert_eq!(name(M3UA), Some("M3UA"));
        assert_eq!(name(M2PA), Some("M2PA"));
        assert_eq!(name(NGAP), Some("NGAP"));
        assert_eq!(name(S1AP), Some("S1AP"));
        assert_eq!(name(DIAMETER), Some("DIAMETER"));
        assert_eq!(name(99999), None);
    }

    #[test]
    fn display_format() {
        assert_eq!(display(NGAP), "NGAP(60)");
        assert_eq!(display(99999), "PPID(99999)");
    }
}
