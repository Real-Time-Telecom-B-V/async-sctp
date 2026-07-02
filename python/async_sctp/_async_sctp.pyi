"""Type stubs for the Rust-backed ``async_sctp._async_sctp`` extension module."""

from __future__ import annotations

# Well-known PPID constants (opaque u32 values).
IUA: int
M2UA: int
M3UA: int
SUA: int
M2PA: int
V5UA: int
S1AP: int
X2AP: int
NGAP: int
XNAP: int
F1AP: int
E1AP: int
DIAMETER: int

class SctpError(Exception):
    """SCTP error."""

class RecvInfo:
    """Metadata about a received message."""

    stream: int
    ppid: int
    assoc_id: int
    def ppid_name(self) -> str | None:
        """The well-known name of the PPID, if any (e.g. ``"NGAP"``)."""

class SctpConfig:
    """Socket configuration applied before connect/bind."""

    def __init__(
        self,
        *,
        out_streams: int = 0,
        max_in_streams: int = 0,
        nodelay: bool | None = None,
        recv_buf: int | None = None,
        send_buf: int | None = None,
        autoclose: int | None = None,
    ) -> None: ...

class SctpAssociation:
    """A one-to-one SCTP association."""

    @staticmethod
    async def connect(addr: str, config: SctpConfig | None = None) -> SctpAssociation: ...
    @staticmethod
    async def connect_multi(
        addrs: list[str], config: SctpConfig | None = None
    ) -> SctpAssociation: ...
    async def send(
        self, data: bytes, stream: int, ppid: int, *, unordered: bool = False, ttl_ms: int = 0
    ) -> int: ...
    async def recv(self) -> tuple[bytes, RecvInfo]: ...
    def peer_addrs(self) -> list[str]: ...
    def local_addrs(self) -> list[str]: ...
    async def shutdown(self) -> None: ...
    async def abort(self) -> None: ...

class SctpListener:
    """A one-to-one SCTP listener."""

    @staticmethod
    def bind(addr: str, config: SctpConfig | None = None) -> SctpListener: ...
    @staticmethod
    def bind_multi(addrs: list[str], config: SctpConfig | None = None) -> SctpListener: ...
    async def accept(self) -> tuple[SctpAssociation, str]: ...
    def local_addr(self) -> str: ...

class SctpServer:
    """A one-to-many SCTP socket."""

    @staticmethod
    def bind(addr: str, config: SctpConfig | None = None) -> SctpServer: ...
    @staticmethod
    def bind_multi(addrs: list[str], config: SctpConfig | None = None) -> SctpServer: ...
    async def recv(self) -> tuple[bytes, RecvInfo, str]: ...
    async def send(
        self,
        assoc_id: int,
        data: bytes,
        stream: int,
        ppid: int,
        *,
        unordered: bool = False,
        ttl_ms: int = 0,
    ) -> int: ...
    def peeloff(self, assoc_id: int) -> SctpAssociation: ...
    def local_addr(self) -> str: ...
