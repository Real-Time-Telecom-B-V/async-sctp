//! One-to-one SCTP echo client — sends a line, prints the echo.
//!
//! Run: `cargo run --example echo_client -- 127.0.0.1:38412 "hello"`

use async_sctp::{ppid, SctpAssociation, SctpConfig};

#[tokio::main]
async fn main() -> Result<(), async_sctp::SctpError> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:38412".to_string());
    let msg = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "hello sctp".to_string());

    // Request a healthy number of streams up front.
    let cfg = SctpConfig::new().streams(16, 16).nodelay(true);
    let assoc = SctpAssociation::connect_with(addr.parse().expect("valid addr"), &cfg).await?;
    println!("connected; peer addrs = {:?}", assoc.peer_addrs()?);

    assoc.send(msg.as_bytes(), 0, ppid::M3UA).await?;
    let (echo, info) = assoc.recv().await?;
    println!(
        "echo on stream {} ({}): {}",
        info.stream,
        ppid::display(info.ppid),
        String::from_utf8_lossy(&echo)
    );
    assoc.shutdown().await?;
    Ok(())
}
