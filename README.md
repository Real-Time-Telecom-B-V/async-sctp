# async-sctp

[![crates.io](https://img.shields.io/crates/v/async-sctp.svg)](https://crates.io/crates/async-sctp)
[![docs.rs](https://docs.rs/async-sctp/badge.svg)](https://docs.rs/async-sctp)
[![CI](https://github.com/Real-Time-Telecom-B-V/async-sctp/actions/workflows/ci.yml/badge.svg)](https://github.com/Real-Time-Telecom-B-V/async-sctp/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Async SCTP for Rust _and_ Python** — a [tokio](https://tokio.rs) wrapper over
the Linux kernel SCTP stack (lksctp) that surfaces what a generic socket can't:
per-message **stream** and **PPID**, **multihoming**, association
**notifications**, and both **one-to-one** and **one-to-many** socket styles.

It carries anything that runs over SCTP — SIGTRAN (M2PA/M3UA/SUA), 3GPP RAN
(**NGAP**, **S1AP**, XnAP, F1AP, E1AP), **Diameter** — because the PPID is opaque
(the crate never interprets it). Ships as **both** a Rust crate
(`cargo add async-sctp`) and a Rust-backed Python wheel (`pip install async-sctp`)
from one source tree.

> **Why this exists:** there is no maintained async kernel-SCTP crate for Rust,
> and **no async SCTP at all for Python** ([pysctp](https://github.com/P1sec/pysctp)
> is synchronous and unmaintained; asyncio has no SCTP transport). `async-sctp`
> fills both gaps from one core.

```rust
use async_sctp::{ppid, SctpConfig, SctpListener};

# async fn ex() -> Result<(), async_sctp::SctpError> {
let listener = SctpListener::bind_config(
    "0.0.0.0:38412".parse().unwrap(),
    &SctpConfig::new().streams(30, 65535),   // NGAP wants many streams
)?;
let (assoc, peer) = listener.accept().await?;
let (data, info) = assoc.recv().await?;           // info.stream, info.ppid
assoc.send(&data, info.stream, ppid::NGAP).await?;
# Ok(()) }
```

```python
import asyncio, async_sctp as sctp

async def main():
    listener = sctp.SctpListener.bind("0.0.0.0:38412",
                                      sctp.SctpConfig(out_streams=30))
    assoc, peer = await listener.accept()
    data, info = await assoc.recv()               # info.stream, info.ppid_name()
    await assoc.send(data, info.stream, sctp.NGAP)

asyncio.run(main())
```

## Two socket styles

| | Rust | Python |
|---|---|---|
| **One-to-one** (a fd per peer, like TCP) | `SctpListener` → `accept` → `SctpAssociation` | `SctpListener.bind(...).accept()` |
| **One-to-many** (one fd, many peers) | `SctpServer` — `recv` tags each message with its `assoc_id`; `peeloff` a busy one into its own `SctpAssociation` | `SctpServer` |

## Feature matrix

| Feature | Status |
|---|---|
| Per-message **stream** + **PPID** (opaque `u32`; registry of well-known names) | ✅ |
| **Multihoming** — bind/connect across many local/peer addresses (`bindx`/`connectx`, mixed v4/v6) | ✅ |
| Address **introspection** — `peer_addrs()` / `local_addrs()` (`getpaddrs`/`getladdrs`) | ✅ |
| **Stream config** — `SCTP_INITMSG` out/in stream counts + INIT retransmit tuning | ✅ `SctpConfig::streams(..)` |
| Sockopts — `NODELAY`, `SO_RCVBUF/SNDBUF`, `AUTOCLOSE` | ✅ `SctpConfig` |
| **Send options** — unordered, **PR-SCTP** timed reliability (`ttl_ms`), abort, eof | ✅ `SendOptions` |
| **One-to-many** (`SOCK_SEQPACKET`) + `sctp_peeloff` | ✅ `SctpServer` |
| **Notifications** — `AssocChange` (COMM_UP/COMM_LOST), shutdown, send-failed | ✅ `recv_msg()` / `ServerMessage` |
| Graceful `shutdown()` + immediate `abort()` | ✅ |
| tokio-native (non-blocking, `AsyncFd`); asyncio-native via PyO3 (`gil_used=false`) | ✅ |

## Performance

Single core, real kernel SCTP over `127.0.0.1`, one message in flight
([`examples/perf.rs`](examples/perf.rs)):

| Payload | Round-trips/s | p50 latency |
|---|---|---|
| 128 B echo (send→echo→recv) | **~95 k/s** | **~11 µs/rtt** |

SCTP is **syscall-bound** (the work is in the kernel, not this crate), so
throughput scales with concurrency/streams and cores, not with parsing. A
counting-allocator [leak check](examples/leak_check.rs)
(`./scripts/mem_leak_test.sh`) churns echo + connect/close and asserts **live
bytes stay flat** (Δ 0). Both run in CI.

## Requirements

**Linux only** — this wraps the kernel SCTP stack. You need:
- the SCTP kernel module loaded: `sudo modprobe sctp`
- libsctp: `libsctp-dev` (Debian/Ubuntu) or `lksctp-tools-devel` (RHEL/Fedora)

## Install

```bash
cargo add async-sctp     # Rust crate (zero pyo3 in the default build)
pip install async-sctp   # Rust-backed async Python wheel (Linux)
```

## Documentation

- [`docs/OVERVIEW.md`](docs/OVERVIEW.md) — architecture + the full API surface.
- [`docs/COMPARISON.md`](docs/COMPARISON.md) — vs other SCTP options.
- [`src/ppid.rs`](src/ppid.rs) — the PPID registry.

## Development

```bash
sudo modprobe sctp                         # tests open real SCTP sockets
cargo test                                 # one-to-one + one-to-many + multihome
cargo test --features python
cargo clippy --all-targets -- -D warnings
./scripts/mem_leak_test.sh                 # live-bytes leak check
cargo deny check

maturin develop && pytest python/tests -q -o asyncio_mode=auto
```

## License

MIT — see [LICENSE](LICENSE).
