"""async-sctp — Rust-backed async SCTP for Python.

A thin asyncio surface over the Linux kernel SCTP stack (lksctp): multihoming,
per-message stream and PPID, one-to-one and one-to-many sockets, and association
notifications. The heavy lifting (syscalls, the tokio reactor) runs in Rust; your
coroutines just ``await`` bind/accept/connect/send/recv.

Requires Linux with the SCTP kernel module loaded (``modprobe sctp``) and
``libsctp`` present.
"""

from __future__ import annotations

from importlib.metadata import PackageNotFoundError, version

from ._async_sctp import (
    RecvInfo,
    SctpAssociation,
    SctpConfig,
    SctpError,
    SctpListener,
    SctpServer,
)
from . import _async_sctp as _ext

try:
    __version__ = version("async-sctp")
except PackageNotFoundError:  # source checkout without an installed dist
    __version__ = "0.0.0+unknown"

__all__ = [
    "SctpListener",
    "SctpAssociation",
    "SctpServer",
    "SctpConfig",
    "RecvInfo",
    "SctpError",
    "__version__",
]

# Re-export the well-known PPID constants (async_sctp.NGAP, async_sctp.M3UA, …).
for _name in dir(_ext):
    if _name.isupper():
        globals()[_name] = getattr(_ext, _name)
        __all__.append(_name)
del _ext, _name
