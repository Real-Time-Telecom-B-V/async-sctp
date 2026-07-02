//! One-to-many SCTP echo server: a single socket serving every peer, echoing
//! each message back on its own association by `assoc_id`. New/closed peers show
//! up as COMM_UP/COMM_LOST notifications.
//!
//! Run: `cargo run --example many_echo -- 0.0.0.0:38412`

use async_sctp::{ppid, SctpServer, ServerMessage};

#[tokio::main]
async fn main() -> Result<(), async_sctp::SctpError> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:38412".to_string());
    let server = SctpServer::bind(addr.parse().expect("valid addr"))?;
    println!("one-to-many server on {}", server.local_addr()?);

    loop {
        match server.recv().await? {
            ServerMessage::Data { data, info, addr } => {
                println!(
                    "  {} bytes from {addr} assoc {} stream {} ({})",
                    data.len(),
                    info.assoc_id,
                    info.stream,
                    ppid::display(info.ppid)
                );
                server
                    .send(info.assoc_id, &data, info.stream, info.ppid)
                    .await?;
            }
            ServerMessage::Notification(n) => println!("  {n}"),
        }
    }
}
