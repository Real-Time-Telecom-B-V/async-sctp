//! Loopback throughput driver. Spins a one-to-one echo server and a client that
//! keeps one message in flight, over real kernel SCTP on 127.0.0.1, and reports
//! sustained messages/second and MB/s. SCTP is syscall-bound, so this measures
//! the send→recv round-trip path, not a codec.
//!
//! Run: `cargo run --release --example perf`
//!      `COUNT=200000 SIZE=256 cargo run --release --example perf`

use std::time::Instant;

use async_sctp::{ppid, SctpAssociation, SctpListener};

#[tokio::main]
async fn main() -> Result<(), async_sctp::SctpError> {
    let count: u64 = std::env::var("COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let size: usize = std::env::var("SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(128);

    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap())?;
    let bound = listener.local_addr()?;

    // Echo server.
    tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        while let Ok((data, info)) = assoc.recv().await {
            if assoc.send(&data, info.stream, info.ppid).await.is_err() {
                break;
            }
        }
    });

    let client = SctpAssociation::connect(bound).await?;
    let payload = vec![0xABu8; size];

    println!("[perf] {count} round-trips of {size} B over loopback SCTP");
    let t = Instant::now();
    for _ in 0..count {
        client.send(&payload, 0, ppid::M3UA).await?;
        let (echo, _) = client.recv().await?;
        std::hint::black_box(&echo);
    }
    let secs = t.elapsed().as_secs_f64();

    let rps = count as f64 / secs;
    let mbps = (count as f64 * size as f64 * 2.0) / secs / 1e6; // ×2: send + echo
    println!(
        "  {count} in {secs:.3}s  =>  {:.0} round-trips/s  ({:.1} MB/s, {:.1} µs/rtt)",
        rps,
        mbps,
        secs * 1e6 / count as f64
    );
    Ok(())
}
