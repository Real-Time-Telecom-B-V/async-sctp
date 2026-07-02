//! Multihoming integration test (loopback uses 127.0.0.1 + 127.0.0.2, both in
//! 127/8). Requires Linux kernel SCTP.

use std::net::SocketAddr;
use std::time::Duration;

use async_sctp::{ppid, RecvResult, SctpAssociation, SctpListener};

#[tokio::test]
async fn multihomed_bind_and_connect() {
    // Pick a free port on 127.0.0.1 first, then bind that same port multihomed.
    let probe = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);

    let a: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let b: SocketAddr = format!("127.0.0.2:{port}").parse().unwrap();
    let listener = SctpListener::bind_multi(&[a, b]).unwrap();

    let server = tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        loop {
            match assoc.recv_msg().await.unwrap() {
                RecvResult::Data(d, i) => {
                    assoc.send(&d, i.stream, i.ppid).await.unwrap();
                    break;
                }
                RecvResult::Notification(_) => continue,
            }
        }
        // Hold the association open while the client inspects its peer addresses,
        // otherwise a SHUTDOWN races the getpaddrs call.
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    // Connect across both peer addresses.
    let client = SctpAssociation::connect_multi(&[a, b]).await.unwrap();
    client.send(b"multihome", 0, ppid::M3UA).await.unwrap();
    loop {
        match client.recv_msg().await.unwrap() {
            RecvResult::Data(d, _) => {
                assert_eq!(d, b"multihome");
                break;
            }
            RecvResult::Notification(_) => continue,
        }
    }

    // The server advertised BOTH of its bound loopback addresses to the peer.
    let peers = client.peer_addrs().unwrap();
    assert!(peers
        .iter()
        .any(|p| p.ip().to_string() == "127.0.0.1" && p.port() == port));
    assert!(peers
        .iter()
        .any(|p| p.ip().to_string() == "127.0.0.2" && p.port() == port));

    server.await.unwrap();
}
