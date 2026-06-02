//! IPv4 + IPv6 round-trips through the multihoming-capable `bind_multi` /
//! `connect_multi` APIs (over kernel SCTP / lksctp).

use sctp::{RecvResult, SctpAssociation, SctpListener};

async fn recv_data(a: &SctpAssociation) -> Vec<u8> {
    loop {
        if let RecvResult::Data(d, _) = a.recv_msg().await.unwrap() {
            return d;
        }
    }
}

async fn echo_once_then(addr_family_msg: &'static [u8], listener: SctpListener) {
    let (a, _) = listener.accept().await.unwrap();
    let d = recv_data(&a).await;
    assert_eq!(d, addr_family_msg);
    a.send(&d, 0, 3).await.unwrap();
}

#[tokio::test]
async fn ipv4_roundtrip_via_multi() {
    let listener = SctpListener::bind_multi(&["127.0.0.1:0".parse().unwrap()]).expect("bind v4");
    let addr = listener.local_addr().unwrap();
    assert!(addr.is_ipv4());
    tokio::spawn(echo_once_then(b"hello-v4", listener));

    let c = SctpAssociation::connect_multi(&[addr]).await.expect("connect v4");
    c.send(b"hello-v4", 0, 3).await.unwrap();
    assert_eq!(recv_data(&c).await, b"hello-v4");
}

#[tokio::test]
async fn ipv6_roundtrip_via_multi() {
    let listener = SctpListener::bind_multi(&["[::1]:0".parse().unwrap()]).expect("bind v6");
    let addr = listener.local_addr().unwrap();
    assert!(addr.is_ipv6());
    tokio::spawn(echo_once_then(b"hello-v6", listener));

    let c = SctpAssociation::connect_multi(&[addr]).await.expect("connect v6");
    c.send(b"hello-v6", 0, 3).await.unwrap();
    assert_eq!(recv_data(&c).await, b"hello-v6");
}
