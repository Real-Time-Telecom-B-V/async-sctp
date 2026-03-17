use sctp::{RecvResult, SctpAssociation, SctpListener};
use std::net::SocketAddr;

/// Integration test: establish a loopback SCTP association and exchange data.
///
/// Requires kernel SCTP support (`modprobe sctp`) and `libsctp-dev`.
/// Run with: `cargo test -- --ignored`
#[tokio::test]
#[ignore]
async fn loopback_echo() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = SctpListener::bind(addr).unwrap();
    let bound_addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (assoc, peer) = listener.accept().await.unwrap();
        assert!(peer.port() > 0);

        // Receive and echo
        match assoc.recv_msg().await {
            Ok(RecvResult::Data(data, info)) => {
                assoc.send(&data, info.stream, info.ppid).await.unwrap();
            }
            Ok(RecvResult::Notification(_)) => {
                // Skip notification, try again
                match assoc.recv_msg().await {
                    Ok(RecvResult::Data(data, info)) => {
                        assoc.send(&data, info.stream, info.ppid).await.unwrap();
                    }
                    other => panic!("expected data, got: {other:?}"),
                }
            }
            Err(e) => panic!("recv error: {e}"),
        }
    });

    // Give server a moment to start
    tokio::task::yield_now().await;

    let client = SctpAssociation::connect(bound_addr).await.unwrap();

    let message = b"test payload";
    let ppid = 5; // M2PA
    let stream = 0;

    client.send(message, stream, ppid).await.unwrap();

    // May get notification first
    loop {
        match client.recv_msg().await.unwrap() {
            RecvResult::Data(data, info) => {
                assert_eq!(data, message);
                assert_eq!(info.stream, stream);
                assert_eq!(info.ppid, ppid);
                break;
            }
            RecvResult::Notification(_) => continue,
        }
    }

    client.shutdown().await.unwrap();
    server.await.unwrap();
}

/// Test that SctpListener::bind works and reports correct local address.
#[tokio::test]
#[ignore]
async fn bind_and_local_addr() {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = SctpListener::bind(addr).unwrap();
    let local = listener.local_addr().unwrap();
    assert_eq!(local.ip(), addr.ip());
    assert_ne!(local.port(), 0); // OS assigned a port
}
