//! One-to-one loopback integration tests. Require a Linux kernel with SCTP
//! (`modprobe sctp`) and `libsctp` present.

use std::net::SocketAddr;
use std::time::Duration;

use async_sctp::{ppid, RecvResult, SctpAssociation, SctpConfig, SctpListener, SendOptions};

/// Read the next *data* message, skipping any notifications (COMM_UP etc.).
async fn recv_data(assoc: &SctpAssociation) -> (Vec<u8>, async_sctp::RecvInfo) {
    loop {
        match assoc.recv_msg().await.unwrap() {
            RecvResult::Data(d, i) => return (d, i),
            RecvResult::Notification(_) => continue,
        }
    }
}

#[tokio::test]
async fn connect_send_recv_echo() {
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (assoc, peer) = listener.accept().await.unwrap();
        assert!(peer.ip().is_loopback());
        let (data, info) = recv_data(&assoc).await;
        assoc.send(&data, info.stream, info.ppid).await.unwrap();
    });

    let client = SctpAssociation::connect(bound).await.unwrap();
    client.send(b"hello sctp", 3, ppid::M3UA).await.unwrap();
    let (echo, info) = recv_data(&client).await;
    assert_eq!(echo, b"hello sctp");
    assert_eq!(info.stream, 3);
    assert_eq!(info.ppid, ppid::M3UA);
    server.await.unwrap();
}

#[tokio::test]
async fn streams_config_is_honored() {
    // Both sides request 16 streams so stream 10 is valid in each direction
    // (the kernel default is only ~10 outbound streams).
    let cfg = SctpConfig::new().streams(16, 16).nodelay(true);
    let listener = SctpListener::bind_config("127.0.0.1:0".parse().unwrap(), &cfg).unwrap();
    let bound = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        let (data, info) = recv_data(&assoc).await;
        assoc.send(&data, info.stream, info.ppid).await.unwrap();
    });

    let client = SctpAssociation::connect_with(bound, &cfg).await.unwrap();
    client.send(b"s", 10, ppid::NGAP).await.unwrap();
    let (_data, info) = recv_data(&client).await;
    assert_eq!(info.stream, 10);
    server.await.unwrap();
}

#[tokio::test]
async fn unordered_send() {
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        let (data, info) = recv_data(&assoc).await;
        assoc.send(&data, info.stream, info.ppid).await.unwrap();
    });
    let client = SctpAssociation::connect(bound).await.unwrap();
    let opts = SendOptions::new().unordered(true);
    client.send_with(b"u", 0, ppid::S1AP, &opts).await.unwrap();
    let (data, _) = recv_data(&client).await;
    assert_eq!(data, b"u");
    server.await.unwrap();
}

#[tokio::test]
async fn peer_and_local_addrs() {
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        // hold the association open briefly
        tokio::time::sleep(Duration::from_millis(100)).await;
        drop(assoc);
    });
    let client = SctpAssociation::connect(bound).await.unwrap();
    let peers = client.peer_addrs().unwrap();
    assert!(peers.iter().any(|a: &SocketAddr| a.port() == bound.port()));
    let locals = client.local_addrs().unwrap();
    assert!(!locals.is_empty());
    server.await.unwrap();
}
