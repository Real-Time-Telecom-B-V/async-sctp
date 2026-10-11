//! An association that is lost ends every receive on it, whoever read the
//! socket error first. Requires Linux kernel SCTP on loopback.

use std::sync::Arc;
use std::time::Duration;

use async_sctp::{SctpAssociation, SctpError, SctpListener};

/// The peer aborts while another task of this side is sending. The send can
/// be the one that is handed the socket error; the receive is then left with
/// a notification that the association was lost and a socket that never
/// becomes readable again. It has to end all the same.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn a_receive_ends_when_the_peer_aborts_while_this_side_sends() {
    for round in 0..40 {
        let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let bound = listener.local_addr().unwrap();
        let client = SctpAssociation::connect(bound).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let server = Arc::new(server);

        let sending = server.clone();
        let sender = tokio::spawn(async move {
            // Until the association is gone.
            while sending.send(b"traffic", 0, 3).await.is_ok() {
                tokio::task::yield_now().await;
            }
        });
        tokio::time::sleep(Duration::from_millis(2)).await;
        // Closed with what the server sent unread: the kernel aborts.
        drop(client);

        let outcome = tokio::time::timeout(Duration::from_secs(2), server.recv()).await;
        match outcome {
            Ok(Err(_)) => {}
            Ok(Ok(_)) => panic!("round {round}: data from a peer that sent none"),
            Err(_) => panic!("round {round}: the receive never ended"),
        }
        // And it stays ended: the next receive does not wait either.
        let again = tokio::time::timeout(Duration::from_secs(2), server.recv()).await;
        assert!(
            matches!(again, Ok(Err(_))),
            "round {round}: a second receive on a lost association"
        );
        let _ = tokio::time::timeout(Duration::from_secs(2), sender).await;
    }
}

/// The same for a peer that shuts down in good order.
#[tokio::test]
async fn a_receive_ends_when_the_peer_closes() {
    for round in 0..20 {
        let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let bound = listener.local_addr().unwrap();
        let client = SctpAssociation::connect(bound).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        drop(client);
        let outcome = tokio::time::timeout(Duration::from_secs(2), server.recv()).await;
        assert!(
            matches!(outcome, Ok(Err(SctpError::PeerShutdown))),
            "round {round}: {:?}",
            outcome.map(|r| r.map(|_| "data"))
        );
    }
}

/// A send that waits for room in the send buffer (the peer reads nothing)
/// ends when the association is lost: there will never be room.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn a_send_that_waits_for_room_ends_when_the_peer_aborts() {
    for round in 0..10 {
        let listener = SctpListener::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let bound = listener.local_addr().unwrap();
        let client = SctpAssociation::connect(bound).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let server = Arc::new(server);

        let sending = server.clone();
        let sender = tokio::spawn(async move {
            let chunk = vec![0x5a; 8192];
            let mut sent = 0u64;
            loop {
                match sending.send(&chunk, 0, 3).await {
                    Ok(_) => sent += 1,
                    Err(_) => return sent,
                }
            }
        });
        // Long enough for the buffers of both ends to fill: the sender waits.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !sender.is_finished(),
            "round {round}: the send did not wait"
        );
        drop(client);
        let sent = tokio::time::timeout(Duration::from_secs(3), sender)
            .await
            .unwrap_or_else(|_| panic!("round {round}: the send never ended"))
            .unwrap();
        assert!(sent > 0);
    }
}
