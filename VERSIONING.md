# Versioning

`async-sctp` follows [Semantic Versioning 2.0.0](https://semver.org/). The public
Rust API — everything reachable as `pub` from the crate root (`SctpListener`,
`SctpAssociation`, `SctpServer`, `SctpConfig`, `SendOptions`, `RecvInfo`, the
`ppid` module, `SctpError`, the notification types) — is the contract. The Python
surface tracks it.

## The git tag is the source of truth

`Cargo.toml`'s `version` matches the release tag, and the release workflow's
`verify-version` job refuses to publish if they disagree. To release: bump
`version`, commit, tag `vX.Y.Z`, push the tag.

## The rule

**MAJOR (`X.0.0`)** — remove/rename/change the signature of a `pub` item, or
change documented behaviour in a way that breaks callers. Removals happen one
minor after a deprecation.

**MINOR (`x.Y.0`)** — backward-compatible additions: new `pub` items (sockopts,
send options, socket styles, config knobs), deprecations (kept working), an MSRV
bump (called out in the changelog).

**PATCH (`x.y.Z`)** — backward-compatible fixes: bug fixes, performance,
behaviour-neutral dependency bumps.

## Platform note

`async-sctp` wraps the Linux kernel SCTP stack, so it is Linux-only by nature.
Requiring a newer kernel SCTP feature is treated like an MSRV bump — a **minor**,
documented in the changelog.

## Pre-releases

`X.Y.Z-rc.N` for validation before a stable tag; crates.io's "newest" pointer
advances only on stable releases.
