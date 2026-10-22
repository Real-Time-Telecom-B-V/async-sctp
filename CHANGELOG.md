# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). See
[VERSIONING.md](VERSIONING.md) for the compatibility policy.

## [1.1.0]

### Added
- `SctpAssociation::connect_from(local, peer, config)`: bind a client socket to
  its local addresses and port (`bind`, then `sctp_bindx` for a multihomed
  endpoint) before `sctp_connectx`. A peer that tells its associations apart
  by remote address and port needs the client to come from the ones it was
  given; an unbound socket offers every address of the host and an ephemeral
  port. Port `0` leaves the port to the kernel. Python: the keyword
  `local_addrs` of `SctpAssociation.connect_multi`.

### Fixed
- A connect that failed could wait forever. A refused association (the far end
  answers the INIT with an ABORT) or an INIT that is given up sets the socket
  error and queues the association-change notification, which the kernel
  reports as error plus readable; the connect waited for writable only, and
  that set of events is neither writable nor write-closed to the runtime. The
  connect now waits for either direction and returns the socket error
  (`ECONNREFUSED`, `ETIMEDOUT`).
- A receive on an association that was lost could wait forever. When the
  association is aborted while another task of the same side is sending, the
  send can be handed the socket error; `recv` then read the notification that
  the association was lost, skipped it, and waited on a socket that never
  becomes readable again. Once the kernel has said the association is over
  (`COMM_LOST`, `SHUTDOWN_COMP`, `CANT_STR_ASSOC`), `recv` and `recv_msg` return
  what is still queued and then `SctpError::NotConnected`.
- A send that waited for room in the send buffer could wait forever when the
  association was lost meanwhile: a lost association frees no room, and the
  error the kernel reports for it is not writability. A waiting send is now
  woken by the socket error and by a receive that read the notification of the
  loss, and returns the error.
- A socket whose registration with the runtime failed during a connect was
  closed twice.
- `recv` and `recv_msg` return a message that the kernel delivered in several
  parts (larger than the receive buffer of one call) whole, and stay
  cancel-safe while they put it together.

### Changed
- The minimum `tokio` is 1.32 (awaiting error readiness), and its `sync`
  feature is enabled.

### Notes
- A client bound to a fixed local port that connects again right after its
  association was closed can wait for one INIT retransmission (the initial
  retransmission timeout of the kernel, 3 seconds by default) while the far
  end finishes the shutdown of the previous association.

## [1.0.0]

First published release — async SCTP over the Linux kernel stack (lksctp), for
Rust and Python from one source tree.

### Added
- **One-to-one** — `SctpListener` (`bind` / `bind_config` / `bind_multi` /
  `accept`) and `SctpAssociation` (`connect` / `connect_with` / `connect_multi` /
  `send` / `send_with` / `recv` / `recv_msg` / `peer_addrs` / `local_addrs` /
  `shutdown` / `abort`), all tokio-native over `AsyncFd`.
- **One-to-many** — `SctpServer` (`SOCK_SEQPACKET`): `recv` tags each message
  with its `assoc_id` + peer address, `send` targets an association by id, and
  `peeloff` branches one into its own one-to-one `SctpAssociation`.
- **Multihoming** — bind/connect across many local/peer addresses (`sctp_bindx`
  / `sctp_connectx`, mixed IPv4/IPv6) + introspection (`peer_addrs` /
  `local_addrs` via `getpaddrs` / `getladdrs`).
- **Config** — `SctpConfig`: `SCTP_INITMSG` stream counts + INIT retransmit
  tuning, `NODELAY`, `SO_RCVBUF`/`SNDBUF`, `AUTOCLOSE`.
- **Send options** — `SendOptions`: unordered, PR-SCTP timed reliability
  (`ttl_ms`), abort, eof.
- **Notifications** — `AssocChange` (COMM_UP/COMM_LOST/…), shutdown, send-failed,
  surfaced via `recv_msg` / `ServerMessage`.
- **PPID registry** — opaque `u32` on the wire plus well-known names (SIGTRAN,
  3GPP RAN NGAP/S1AP/XnAP/F1AP/E1AP, Diameter) in the `ppid` module.
- **Python wheel** (`pip install async-sctp`, feature `python`) — the whole
  surface as asyncio coroutines via PyO3 + `pyo3-async-runtimes`, `gil_used=false`
  (free-threaded), with `register(py, parent)` for embedders.
- **Quality bar** — real-kernel loopback integration tests (one-to-one,
  one-to-many, multihoming), a counting-allocator leak check, a throughput
  driver, async pytest parity, and CI that loads the `sctp` module.

[1.1.0]: https://github.com/Real-Time-Telecom-B-V/async-sctp/releases/tag/v1.1.0
[1.0.0]: https://github.com/Real-Time-Telecom-B-V/async-sctp/releases/tag/v1.0.0
