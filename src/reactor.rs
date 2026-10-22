//! Registration of this crate's sockets with the tokio reactor.

use std::io;
use std::os::unix::io::AsRawFd;

use tokio::io::unix::AsyncFd;

/// A socket wrapper that owns its descriptor: the descriptor is open from the
/// wrapper's construction until its `Drop` closes it, and `as_raw_fd` returns
/// that same descriptor every time.
pub(crate) trait OwnsDescriptor: AsRawFd {}

/// Register `socket` with the reactor of the current runtime.
pub(crate) fn register<T: OwnsDescriptor>(socket: T) -> io::Result<AsyncFd<T>> {
    // SAFETY: an `OwnsDescriptor` keeps one descriptor open until it is
    // dropped, and the `AsyncFd` drops it only after deregistering it, so the
    // descriptor stays valid and the same for the life of the registration.
    unsafe { AsyncFd::register(socket) }.map_err(io::Error::from)
}
