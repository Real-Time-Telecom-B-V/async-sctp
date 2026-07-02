# Changelog

All notable changes are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). See
[VERSIONING.md](VERSIONING.md) for the compatibility policy.

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

[1.0.0]: https://github.com/Real-Time-Telecom-B-V/async-sctp/releases/tag/v1.0.0
