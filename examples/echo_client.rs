//! SCTP echo client example.
//!
//! Connects to 127.0.0.1:9999, sends a message on stream 0 with PPID 3 (M3UA),
//! and prints the echoed response.
//!
//! # Usage
//! ```sh
//! cargo run --example echo_client
//! ```

use sctp::SctpAssociation;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:9999".parse()?;
    println!("Connecting to {addr}...");

    let assoc = SctpAssociation::connect(addr).await?;
    println!("Connected!");

    let message = b"Hello, SCTP!";
    let ppid = 3; // M3UA
    let stream = 0;

    assoc.send(message, stream, ppid).await?;
    println!("Sent {} bytes on stream {stream} (ppid={ppid})", message.len());

    let (data, info) = assoc.recv().await?;
    println!(
        "Received echo: {:?} on stream {} (ppid={})",
        String::from_utf8_lossy(&data),
        info.stream,
        info.ppid
    );

    assoc.shutdown().await?;
    println!("Shutdown complete");

    Ok(())
}
