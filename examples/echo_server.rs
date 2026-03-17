//! SCTP echo server example.
//!
//! Binds to 127.0.0.1:9999, accepts one association, and echoes back
//! any received data on the same stream and PPID.
//!
//! # Usage
//! ```sh
//! cargo run --example echo_server
//! ```
//!
//! Requires: `modprobe sctp` and `libsctp-dev` installed.

use sctp::{RecvResult, SctpListener};
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr: SocketAddr = "127.0.0.1:9999".parse()?;
    let listener = SctpListener::bind(addr)?;
    println!("SCTP echo server listening on {}", listener.local_addr()?);

    let (assoc, peer) = listener.accept().await?;
    println!("Accepted association from {peer}");

    loop {
        match assoc.recv_msg().await {
            Ok(RecvResult::Data(data, info)) => {
                println!(
                    "Received {} bytes on stream {} (ppid={})",
                    data.len(),
                    info.stream,
                    info.ppid
                );
                // Echo back on same stream and ppid
                assoc.send(&data, info.stream, info.ppid).await?;
                println!("Echoed {} bytes back", data.len());
            }
            Ok(RecvResult::Notification(notif)) => {
                println!("Notification: {notif}");
            }
            Err(sctp::SctpError::PeerShutdown) => {
                println!("Peer shut down, exiting");
                break;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                break;
            }
        }
    }

    Ok(())
}
