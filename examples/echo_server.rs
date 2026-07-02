//! One-to-one SCTP echo server.
//!
//! Run: `cargo run --example echo_server -- 0.0.0.0:38412`
//! (needs `modprobe sctp` + libsctp).

use async_sctp::{ppid, RecvResult, SctpListener};

#[tokio::main]
async fn main() -> Result<(), async_sctp::SctpError> {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:38412".to_string());
    let listener = SctpListener::bind(addr.parse().expect("valid addr"))?;
    println!("listening on {}", listener.local_addr()?);

    loop {
        let (assoc, peer) = listener.accept().await?;
        println!("association from {peer}");
        tokio::spawn(async move {
            loop {
                match assoc.recv_msg().await {
                    Ok(RecvResult::Data(data, info)) => {
                        println!(
                            "  {} bytes on stream {} ({})",
                            data.len(),
                            info.stream,
                            ppid::display(info.ppid)
                        );
                        let _ = assoc.send(&data, info.stream, info.ppid).await;
                    }
                    Ok(RecvResult::Notification(n)) => println!("  {n}"),
                    Err(e) => {
                        println!("  closed: {e}");
                        break;
                    }
                }
            }
        });
    }
}
