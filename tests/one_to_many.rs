//! One-to-many (`SctpServer`) integration tests. Require Linux kernel SCTP.

use async_sctp::{ppid, SctpAssociation, SctpServer, ServerMessage};

/// Read the next data message on the server, skipping notifications.
async fn server_data(srv: &SctpServer) -> (Vec<u8>, i32, u16, u32) {
    loop {
        match srv.recv().await.unwrap() {
            ServerMessage::Data { data, info, .. } => {
                return (data, info.assoc_id, info.stream, info.ppid)
            }
            ServerMessage::Notification(_) => continue,
        }
    }
}

#[tokio::test]
async fn server_serves_many_and_echoes_by_assoc() {
    let server = SctpServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = server.local_addr().unwrap();

    // Two independent one-to-one clients connect to the one-to-many server.
    let a = SctpAssociation::connect(bound).await.unwrap();
    let b = SctpAssociation::connect(bound).await.unwrap();

    a.send(b"from-a", 0, ppid::NGAP).await.unwrap();
    b.send(b"from-b", 1, ppid::NGAP).await.unwrap();

    // The server sees both, on distinct association ids, and echoes each back.
    let mut seen = std::collections::HashSet::new();
    for _ in 0..2 {
        let (data, assoc_id, stream, pp) = server_data(&server).await;
        server.send(assoc_id, &data, stream, pp).await.unwrap();
        seen.insert(data);
    }
    assert!(seen.contains(&b"from-a"[..]));
    assert!(seen.contains(&b"from-b"[..]));

    // Each client gets its own echo back.
    let (ea, _) = a.recv().await.unwrap();
    let (eb, _) = b.recv().await.unwrap();
    assert_eq!(ea, b"from-a");
    assert_eq!(eb, b"from-b");
}

#[tokio::test]
async fn peeloff_to_one_to_one() {
    let server = SctpServer::bind("127.0.0.1:0".parse().unwrap()).unwrap();
    let bound = server.local_addr().unwrap();
    let client = SctpAssociation::connect(bound).await.unwrap();
    client.send(b"peel", 0, ppid::M2PA).await.unwrap();

    let (data, assoc_id, _, _) = server_data(&server).await;
    assert_eq!(data, b"peel");

    // Branch that association into its own one-to-one socket and reply from it.
    let peeled = server.peeloff(assoc_id).unwrap();
    peeled.send(b"peeled-reply", 0, ppid::M2PA).await.unwrap();
    let (reply, _) = client.recv().await.unwrap();
    assert_eq!(reply, b"peeled-reply");
}
