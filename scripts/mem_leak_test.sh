#!/usr/bin/env bash
#
# async-sctp memory-leak regression test.
#
# Builds and runs the `leak_check` example, which installs a counting global
# allocator and asserts live bytes (allocated − freed) stay flat across repeated
# echo round-trips and connect/close churn. Exits non-zero (and prints FAIL) on a
# leak. Requires the SCTP kernel module (`sudo modprobe sctp`) + libsctp.
#
# Usage: ./scripts/mem_leak_test.sh

set -euo pipefail
cd "$(dirname "$0")/.."

echo "[*] building leak_check (release)..."
cargo build --release --example leak_check --quiet

echo "[*] running..."
./target/release/examples/leak_check
