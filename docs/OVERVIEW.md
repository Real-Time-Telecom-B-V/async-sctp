# async-sctp — architecture overview

A map of the crate's internals and public surface. For usage see the
[README](../README.md); for how it compares to the alternatives see
[COMPARISON.md](COMPARISON.md).

## What it is

`async-sctp` is a thin async layer over the **Linux kernel SCTP stack (lksctp)**.
It opens real kernel SCTP sockets via `libc`, drives them non-blocking through
tokio's [`AsyncFd`](https://docs.rs/tokio/latest/tokio/io/unix/struct.AsyncFd.html),
and exposes SCTP's distinguishing features — per-message stream + PPID,
multihoming, and association notifications — that a `TcpStream` cannot express.
The Python wheel is the same core behind PyO3 + `pyo3-async-runtimes`, so the
coroutines bridge straight onto asyncio.

It is **not** a userspace SCTP implementation (like `webrtc-sctp`): the protocol
engine is the kernel's. That means it is Linux-only and needs the `sctp` module
loaded, but it inherits the kernel's mature, RFC-compliant SCTP.

## Module map

| Path | Responsibility |
|---|---|
| `src/lib.rs` | Crate root: re-exports + crate docs. |
| `src/sys.rs` | Raw `libc`/lksctp FFI: `sctp_sendmsg`/`recvmsg`/`bindx`/`connectx`/`send`/`peeloff`/`getpaddrs`/`getladdrs`, the `#[repr(C)]` structs, socket-option + notification constants. |
| `src/addr.rs` | `SocketAddr` ↔ raw `sockaddr` conversions + the packed mixed-family arrays that `bindx`/`connectx`/`getpaddrs` use. |
| `src/config.rs` | `SctpConfig` (INIT stream counts, NODELAY, buffers, autoclose) applied before connect/listen. |
| `src/listener.rs` | `SctpListener` — one-to-one `bind`/`accept`. |
| `src/association.rs` | `SctpAssociation` — one-to-one connect/send/recv, `SendOptions`, introspection, shutdown/abort. |
| `src/server.rs` | `SctpServer` — one-to-many (`SOCK_SEQPACKET`) recv/send-by-assoc + `peeloff`. |
| `src/notification.rs` | SCTP notification parsing (`AssocChange`, shutdown, send-failed). |
| `src/ppid.rs` | The PPID registry (opaque `u32` + well-known names). |
| `src/types.rs` | `RecvInfo` (stream / ppid / assoc_id). |
| `src/error.rs` | `SctpError`. |
| `src/python.rs` | PyO3 bindings (`--features python`). |

## Public API surface (the SemVer contract)

- **One-to-one:** `SctpListener::{bind, bind_config, bind_multi, bind_multi_with,
  accept, local_addr}`; `SctpAssociation::{connect, connect_with, connect_multi,
  connect_multi_with, send, send_with, recv, recv_msg, peer_addrs, local_addrs,
  shutdown, abort}`.
- **One-to-many:** `SctpServer::{bind, bind_config, bind_multi, bind_multi_with,
  recv, send, send_with, peeloff, local_addr}`; `ServerMessage`.
- **Values:** `SctpConfig` (+ `InitMsg`), `SendOptions`, `RecvInfo`, `RecvResult`,
  `Notification` / `AssocChangeEvent` / `AssocChangeState`, `SctpError`.
- **Constants:** the `ppid` module (`ppid::NGAP`, `ppid::M3UA`, …, `ppid::name`,
  `ppid::display`).

## Async model

Every socket is non-blocking and registered with tokio's reactor via `AsyncFd`.
`send`/`recv`/`accept`/`connect` await readiness and retry on `WouldBlock`, so
they compose with any tokio task. The synchronous constructors (`bind`,
`peeloff`) create an `AsyncFd` and therefore must run inside a tokio runtime; on
the Python side the wheel enters its bundled runtime for those calls
automatically.

`recv()` returns application data and **skips notifications** transparently; use
`recv_msg()` (one-to-one) or `SctpServer::recv` (one-to-many, returns
`ServerMessage`) when you need to observe COMM_UP/COMM_LOST and friends.

## What lives in the kernel vs. here

| Concern | Where |
|---|---|
| SCTP association state machine, retransmission, congestion, heartbeats, chunk bundling | Linux kernel (lksctp) |
| Socket lifecycle, non-blocking I/O, stream/PPID plumbing, multihoming setup, notification parsing, config | this crate |
| Your protocol (M3UA, NGAP, Diameter, …) | your code, on top |
