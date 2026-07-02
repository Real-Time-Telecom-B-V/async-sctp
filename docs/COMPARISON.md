# Comparison

Where `async-sctp` sits among the ways to speak SCTP from Rust or Python.

| | async-sctp | [rust-sctp](https://github.com/phsym/rust-sctp) | [webrtc-sctp](https://github.com/simmons/webrtc-sctp) | [pysctp](https://github.com/P1sec/pysctp) |
|---|---|---|---|---|
| Language | Rust **+ Python** | Rust | Rust | Python |
| Async | ✅ tokio / asyncio | ❌ blocking | ✅ (userspace) | ❌ blocking |
| SCTP engine | Linux kernel (lksctp) | Linux kernel | **userspace** (WebRTC) | Linux kernel |
| Per-message stream + PPID | ✅ | partial | n/a | ✅ |
| Multihoming (`bindx`/`connectx`) | ✅ | ✅ | — | ✅ |
| Notifications (COMM_UP/LOST) | ✅ | ❌ (TODO) | — | limited |
| One-to-many + `peeloff` | ✅ | one-to-many only | — | ✅ |
| Stream-count / sockopt config | ✅ | limited | — | ✅ |
| Maintained | ✅ | stale | WIP, "not spec-compliant" | inactive (no release in years) |

## The gap it fills

- **Rust:** there is no well-maintained *async* wrapper over kernel SCTP.
  `rust-sctp` is blocking and hasn't landed notifications; `webrtc-sctp` is a
  userspace stack aimed at WebRTC data channels, not SIGTRAN/3GPP signalling over
  lksctp. `async-sctp` is tokio-native over the kernel stack.
- **Python:** there is **no async SCTP at all**. `pysctp` is synchronous sockets
  and unmaintained; asyncio has no SCTP transport. `async-sctp` is the first
  Rust-backed **async** SCTP for Python, sharing one core with the Rust crate.

## When to use it

- You're building an SCTP-carried signalling service or test rig — SIGTRAN
  (M2PA/M3UA/SUA), 3GPP RAN (NGAP/S1AP/XnAP/F1AP), or Diameter-over-SCTP — and
  want per-message stream/PPID, multihoming, and notifications without hand-rolling
  `libc` FFI.
- You want the **same** SCTP surface in a Rust service and in Python tooling.

## When not to

- You need SCTP where there's **no kernel SCTP** (macOS, Windows, or a sandbox
  without the module) — use a userspace stack like `webrtc-sctp`. `async-sctp` is
  Linux + lksctp only.
- You need the SCTP association logic in userspace for portability or WebRTC data
  channels — again, `webrtc-sctp`.
