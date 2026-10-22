//! Connecting: a refused association is always reported, and a client can be
//! bound to local addresses and a local port before it connects. Requires
//! Linux kernel SCTP on loopback.

use std::net::SocketAddr;
use std::time::Duration;

use async_sctp::{SctpAssociation, SctpConfig, SctpError, SctpListener};

/// A loopback port nothing listens on: bound once to learn a free one, then
/// released.
fn free_port(ip: &str) -> u16 {
    let probe = SctpListener::bind(format!("{ip}:0").parse().unwrap()).unwrap();
    probe.local_addr().unwrap().port()
}

/// The far end answers the INIT with an ABORT (nothing listens there). The
/// connect has to return that as an error every time, not only when the
/// error reaches the socket before the association-change notification does.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_connect_always_returns() {
    for round in 0..40 {
        let port = free_port("127.0.0.1");
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let outcome =
            tokio::time::timeout(Duration::from_secs(3), SctpAssociation::connect(addr)).await;
        match outcome {
            Ok(Err(SctpError::Io(error))) => {
                assert_eq!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionRefused,
                    "round {round}: {error}"
                );
            }
            Ok(Err(other)) => panic!("round {round}: unexpected error {other}"),
            Ok(Ok(_)) => panic!("round {round}: connected to a port nothing listens on"),
            Err(_) => panic!("round {round}: the connect never returned"),
        }
    }
}

/// What comes after a connect still works: the wait for the outcome must not
/// use up the readiness the first send and the first receive need.
#[tokio::test]
async fn a_connect_that_succeeds_can_send_and_receive_at_once() {
    for _ in 0..20 {
        let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let bound = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (assoc, _) = listener.accept().await.unwrap();
            let (data, info) = assoc.recv().await.unwrap();
            assoc.send(&data, info.stream, info.ppid).await.unwrap();
            // Until the client has read the echo.
            let _ = assoc.recv().await;
        });
        let client = tokio::time::timeout(Duration::from_secs(3), SctpAssociation::connect(bound))
            .await
            .expect("the connect returns")
            .unwrap();
        tokio::time::timeout(Duration::from_secs(3), client.send(b"first", 0, 3))
            .await
            .expect("the send returns")
            .unwrap();
        let (echo, _) = tokio::time::timeout(Duration::from_secs(3), client.recv())
            .await
            .expect("the receive returns")
            .unwrap();
        assert_eq!(echo, b"first");
        drop(client);
        server.await.unwrap();
    }
}

/// A client bound before it connects is seen by the far end from exactly the
/// addresses and the port it was bound to.
#[tokio::test]
async fn a_bound_client_connects_from_its_addresses_and_port() {
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    let port = free_port("127.0.0.2");
    let local: Vec<SocketAddr> = vec![
        format!("127.0.0.2:{port}").parse().unwrap(),
        format!("127.0.0.3:{port}").parse().unwrap(),
    ];

    let from = local.clone();
    let server = tokio::spawn(async move {
        let (assoc, peer) = listener.accept().await.unwrap();
        assert!(from.contains(&peer), "accepted from {peer}");
        let mut peers = assoc.peer_addrs().unwrap();
        peers.sort();
        assert_eq!(peers, from, "every address of the peer, and no other");
        // Tell the client it may close.
        assoc.send(b"seen", 0, 3).await.unwrap();
        let _ = assoc.recv().await;
    });

    let client = SctpAssociation::connect_from(&local, &[bound], &SctpConfig::new())
        .await
        .unwrap();
    let mut mine = client.local_addrs().unwrap();
    mine.sort();
    assert_eq!(mine, local);
    client.recv().await.unwrap();
    drop(client);
    server.await.unwrap();
}

/// Port 0 leaves the local port to the kernel, for every address.
#[tokio::test]
async fn a_bound_client_may_leave_the_port_to_the_kernel() {
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    let local: Vec<SocketAddr> = vec![
        "127.0.0.4:0".parse().unwrap(),
        "127.0.0.5:0".parse().unwrap(),
    ];
    let server = tokio::spawn(async move {
        let (assoc, _) = listener.accept().await.unwrap();
        let peers = assoc.peer_addrs().unwrap();
        assoc.send(b"seen", 0, 3).await.unwrap();
        let _ = assoc.recv().await;
        peers
    });
    let client = SctpAssociation::connect_from(&local, &[bound], &SctpConfig::new())
        .await
        .unwrap();
    let mine = client.local_addrs().unwrap();
    client.recv().await.unwrap();
    drop(client);
    let mut peers = server.await.unwrap();
    peers.sort();
    assert_eq!(peers.len(), 2);
    assert_eq!(peers[0].ip().to_string(), "127.0.0.4");
    assert_eq!(peers[1].ip().to_string(), "127.0.0.5");
    assert_ne!(peers[0].port(), 0);
    assert_eq!(peers[0].port(), peers[1].port());
    assert_eq!(mine.len(), 2);
}

/// Local addresses on different ports are not one endpoint.
#[tokio::test]
async fn local_addresses_on_different_ports_are_refused() {
    let local: Vec<SocketAddr> = vec![
        "127.0.0.2:4000".parse().unwrap(),
        "127.0.0.3:4001".parse().unwrap(),
    ];
    let peer: SocketAddr = "127.0.0.1:9".parse().unwrap();
    let outcome = SctpAssociation::connect_from(&local, &[peer], &SctpConfig::new()).await;
    assert!(matches!(outcome, Err(SctpError::InvalidAddress(_))));
}

/// A refused connect from a bound client is reported like any other, and the
/// local port is free again for the next attempt.
#[tokio::test]
async fn a_refused_bound_connect_returns_and_frees_its_port() {
    let local_port = free_port("127.0.0.6");
    let local: SocketAddr = format!("127.0.0.6:{local_port}").parse().unwrap();
    for round in 0..10 {
        let port = free_port("127.0.0.1");
        let peer: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let outcome = tokio::time::timeout(
            Duration::from_secs(3),
            SctpAssociation::connect_from(&[local], &[peer], &SctpConfig::new()),
        )
        .await
        .unwrap_or_else(|_| panic!("round {round}: the connect never returned"));
        match outcome {
            Err(SctpError::Io(error)) => {
                assert_eq!(error.kind(), std::io::ErrorKind::ConnectionRefused)
            }
            other => panic!("round {round}: {:?}", other.map(|_| "connected")),
        }
    }
}

/// The local port of a client is free again once its association is closed:
/// the next connect from the same addresses and port comes up.
#[tokio::test]
async fn the_same_local_port_connects_again_after_a_close() {
    let local_port = free_port("127.0.0.7");
    let local: SocketAddr = format!("127.0.0.7:{local_port}").parse().unwrap();
    let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..2 {
            let (assoc, peer) = listener.accept().await.unwrap();
            assert_eq!(peer, local);
            // Until the client closes.
            let _ = assoc.recv().await;
        }
    });
    for round in 0..2 {
        let client = tokio::time::timeout(
            Duration::from_secs(5),
            SctpAssociation::connect_from(&[local], &[bound], &SctpConfig::new()),
        )
        .await
        .unwrap_or_else(|_| panic!("round {round}: the connect never returned"))
        .unwrap_or_else(|error| panic!("round {round}: {error}"));
        drop(client);
    }
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("both associations accepted")
        .unwrap();
}
