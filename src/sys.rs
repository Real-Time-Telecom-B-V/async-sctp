//! Raw libc bindings for Linux kernel SCTP.
//!
//! These are the low-level FFI bindings to lksctp functions and constants.
//! Users should prefer the higher-level `SctpListener` and `SctpAssociation` types.

use libc::{c_int, c_void, size_t, sockaddr, socklen_t};

// Protocol number
pub const IPPROTO_SCTP: c_int = 132;
pub const SOL_SCTP: c_int = 132;

// Socket option constants
pub const SCTP_INITMSG: c_int = 2;
pub const SCTP_EVENTS: c_int = 11;
pub const SCTP_NODELAY: c_int = 3;
pub const SCTP_RECVRCVINFO: c_int = 32;
pub const SCTP_RECVNXTINFO: c_int = 33;

// SCTP notification types
pub const SCTP_SN_TYPE_BASE: u16 = 1 << 15;
pub const SCTP_ASSOC_CHANGE: u16 = SCTP_SN_TYPE_BASE | 0x0001;
pub const SCTP_PEER_ADDR_CHANGE: u16 = SCTP_SN_TYPE_BASE | 0x0002;
pub const SCTP_REMOTE_ERROR: u16 = SCTP_SN_TYPE_BASE | 0x0003;
pub const SCTP_SEND_FAILED: u16 = SCTP_SN_TYPE_BASE | 0x0004;
pub const SCTP_SHUTDOWN_EVENT: u16 = SCTP_SN_TYPE_BASE | 0x0005;
pub const SCTP_ADAPTATION_INDICATION: u16 = SCTP_SN_TYPE_BASE | 0x0006;
pub const SCTP_PARTIAL_DELIVERY_EVENT: u16 = SCTP_SN_TYPE_BASE | 0x0007;

// SCTP association change states
pub const SCTP_COMM_UP: u16 = 0;
pub const SCTP_COMM_LOST: u16 = 1;
pub const SCTP_RESTART: u16 = 2;
pub const SCTP_SHUTDOWN_COMP: u16 = 3;
pub const SCTP_CANT_STR_ASSOC: u16 = 4;

// MSG_NOTIFICATION flag for recvmsg
pub const MSG_NOTIFICATION: c_int = 0x8000;

/// sctp_initmsg - SCTP initialization parameters.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SctpInitMsg {
    pub sinit_num_ostreams: u16,
    pub sinit_max_instreams: u16,
    pub sinit_max_attempts: u16,
    pub sinit_max_init_timeo: u16,
}

/// sctp_sndrcvinfo - SCTP send/receive ancillary data.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SctpSndRcvInfo {
    pub sinfo_stream: u16,
    pub sinfo_ssn: u16,
    pub sinfo_flags: u16,
    pub sinfo_ppid: u32,
    pub sinfo_context: u32,
    pub sinfo_timetolive: u32,
    pub sinfo_tsn: u32,
    pub sinfo_cumtsn: u32,
    pub sinfo_assoc_id: i32,
}

/// sctp_event_subscribe - Enable/disable SCTP event notifications.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SctpEventSubscribe {
    pub sctp_data_io_event: u8,
    pub sctp_association_event: u8,
    pub sctp_address_event: u8,
    pub sctp_send_failure_event: u8,
    pub sctp_peer_error_event: u8,
    pub sctp_shutdown_event: u8,
    pub sctp_partial_delivery_event: u8,
    pub sctp_adaptation_layer_event: u8,
    pub sctp_authentication_event: u8,
    pub sctp_sender_dry_event: u8,
    pub sctp_stream_reset_event: u8,
    pub sctp_assoc_reset_event: u8,
    pub sctp_stream_change_event: u8,
    pub sctp_send_failure_event_event: u8,
}

impl Default for SctpEventSubscribe {
    fn default() -> Self {
        Self {
            sctp_data_io_event: 1,
            sctp_association_event: 1,
            sctp_address_event: 0,
            sctp_send_failure_event: 1,
            sctp_peer_error_event: 0,
            sctp_shutdown_event: 1,
            sctp_partial_delivery_event: 0,
            sctp_adaptation_layer_event: 0,
            sctp_authentication_event: 0,
            sctp_sender_dry_event: 0,
            sctp_stream_reset_event: 0,
            sctp_assoc_reset_event: 0,
            sctp_stream_change_event: 0,
            sctp_send_failure_event_event: 0,
        }
    }
}

/// sctp_assoc_change notification header.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SctpAssocChange {
    pub sac_type: u16,
    pub sac_flags: u16,
    pub sac_length: u32,
    pub sac_state: u16,
    pub sac_error: u16,
    pub sac_outbound_streams: u16,
    pub sac_inbound_streams: u16,
    pub sac_assoc_id: i32,
    // info[] follows but is variable-length
}

/// sctp_shutdown_event notification.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SctpShutdownEvent {
    pub sse_type: u16,
    pub sse_flags: u16,
    pub sse_length: u32,
    pub sse_assoc_id: i32,
}

/// Notification header (common prefix for all notifications).
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SctpNotificationHeader {
    pub sn_type: u16,
    pub sn_flags: u16,
    pub sn_length: u32,
}

extern "C" {
    pub fn sctp_sendmsg(
        s: c_int,
        msg: *const c_void,
        len: size_t,
        to: *const sockaddr,
        tolen: socklen_t,
        ppid: u32,
        flags: u32,
        stream_no: u16,
        timetolive: u32,
        context: u32,
    ) -> c_int;

    pub fn sctp_recvmsg(
        s: c_int,
        msg: *mut c_void,
        len: size_t,
        from: *mut sockaddr,
        fromlen: *mut socklen_t,
        sinfo: *mut SctpSndRcvInfo,
        msg_flags: *mut c_int,
    ) -> c_int;

    pub fn sctp_bindx(
        sd: c_int,
        addrs: *const sockaddr,
        addrcnt: c_int,
        flags: c_int,
    ) -> c_int;

    pub fn sctp_connectx(
        sd: c_int,
        addrs: *const sockaddr,
        addrcnt: c_int,
        id: *mut i32,
    ) -> c_int;
}

// Link against libsctp
#[link(name = "sctp")]
extern "C" {}
