//! # async-sctp
//!
//! Async **SCTP** for Rust — a [`tokio`] wrapper over the Linux kernel SCTP
//! stack (lksctp) that surfaces the things a generic socket can't: per-message
//! **stream** and **PPID**, **multihoming**, association **notifications**, and
//! both **one-to-one** and **one-to-many** socket styles. It carries any
//! protocol that runs over SCTP — SIGTRAN (M2PA/M3UA/SUA), 3GPP RAN (NGAP,
//! S1AP, XnAP, F1AP), Diameter — the PPID is opaque (see [`ppid`] for names).
//!
//! ```no_run
//! use async_sctp::{SctpListener, ppid};
//!
//! # async fn ex() -> Result<(), async_sctp::SctpError> {
//! let listener = SctpListener::bind("0.0.0.0:38412".parse().unwrap())?;
//! let (assoc, peer) = listener.accept().await?;
//! let (data, info) = assoc.recv().await?;               // info.stream, info.ppid
//! assoc.send(&data, info.stream, ppid::NGAP).await?;    // echo on any PPID
//! # Ok(()) }
//! ```
//!
//! ## Two socket styles
//! - [`SctpListener`] + [`SctpAssociation`] — **one-to-one**: `accept` yields a
//!   dedicated association per peer (like `TcpListener` → `TcpStream`).
//! - [`SctpServer`] — **one-to-many**: a single socket serves many associations;
//!   `recv` returns messages tagged with their `assoc_id`, and you can
//!   [`peeloff`](SctpServer::peeloff) a busy one into its own [`SctpAssociation`].
//!
//! ## Requirements
//! Linux with the SCTP kernel module loaded (`modprobe sctp`) and `libsctp`
//! present (`libsctp-dev` on Debian/Ubuntu, `lksctp-tools-devel` on RHEL/Fedora).

mod addr;
pub mod association;
pub mod config;
pub mod error;
pub mod listener;
pub mod notification;
pub mod ppid;
mod recv;
pub mod server;
pub mod sys;
pub mod types;

#[cfg(feature = "python")]
pub mod python;

pub use association::{RecvResult, SctpAssociation, SendOptions};
pub use config::{InitMsg, SctpConfig};
pub use error::SctpError;
pub use listener::SctpListener;
pub use notification::{AssocChangeEvent, AssocChangeState, Notification};
pub use server::{SctpServer, ServerMessage};
pub use types::RecvInfo;
