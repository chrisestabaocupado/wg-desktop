//! Real BoringTun handshake and smoltcp traffic, transported only over loopback UDP.
//! No TUN adapter, external server, administrator privileges or OS route changes.

use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use smoltcp::socket::udp;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CLIENT_KEY: [u8; 32] = [1; 32];
const SERVER_KEY: [u8; 32] = [2; 32];

fn profile(
    private: [u8; 32],
    peer: [u8; 32],
    address: &str,
    endpoint: SocketAddr,
    dns: bool,
) -> Profile {
    let public = PublicKey::from(&StaticSecret::from(peer));
    let v6 = address.contains(':');
    let prefix = if v6 { 128 } else { 32 };
    let subnet = if v6 { "fd77::/64" } else { "10.77.0.0/24" };
    let dns = if !dns {
        ""
    } else if v6 {
        "DNS = fd77::1"
    } else {
        "DNS = 10.77.0.1"
    };
    Profile::parse(&format!(
        "[Interface]\nPrivateKey = {}\nAddress = {address}/{prefix}\n{dns}\n[Peer]\nPublicKey = {}\nEndpoint = {endpoint}\nAllowedIPs = {subnet}\n",
        STANDARD.encode(private), STANDARD.encode(public.as_bytes())
    )).unwrap()
}

struct PeerLab {
    endpoint: SocketAddr,
    dns_queries: Arc<AtomicUsize>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<()>>,
}

impl PeerLab {
    async fn start() -> Self {
        Self::start_with_ipv6(false).await
    }

    async fn start_with_ipv6(ipv6: bool) -> Self {
        let udp = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let endpoint = udp.local_addr().unwrap();
        let config = profile(
            SERVER_KEY,
            CLIENT_KEY,
            if ipv6 { "fd77::1" } else { "10.77.0.1" },
            "127.0.0.1:9".parse().unwrap(),
            false,
        );
        let mut engine = Engine::new(config, udp).unwrap();
        let mut echo = tcp::Socket::new(
            tcp::SocketBuffer::new(vec![0; BUFFER_SIZE]),
            tcp::SocketBuffer::new(vec![0; BUFFER_SIZE]),
        );
        echo.listen(8080).unwrap();
        let echo_handle = engine.sockets.add(echo);
        let mut dns = udp::Socket::new(
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 8192]),
            udp::PacketBuffer::new(vec![udp::PacketMetadata::EMPTY; 8], vec![0; 8192]),
        );
        dns.bind(53).unwrap();
        let dns_handle = engine.sockets.add(dns);
        let dns_queries = Arc::new(AtomicUsize::new(0));
        let queries = dns_queries.clone();
        let (stop, mut stopping) = oneshot::channel();
        let task = tokio::spawn(async move {
            let mut buffer = vec![0; BUFFER_SIZE + 256];
            let mut output = vec![0; BUFFER_SIZE + 256];
            let mut pending_echo = vec![];
            let mut offset = 0;
            let mut tick = tokio::time::interval(Duration::from_millis(5));
            loop {
                tokio::select! {
                    _ = &mut stopping => return Ok(()),
                    packet = engine.udp.recv_from(&mut buffer) => {
                        let (length, source) = packet?;
                        engine.udp.connect(source).await?;
                        engine.receive(&buffer[..length], &mut output).await?;
                    },
                    _ = tick.tick() => {}
                }
                engine
                    .iface
                    .poll(engine.now(), &mut engine.device, &mut engine.sockets);
                let dns = engine.sockets.get_mut::<udp::Socket>(dns_handle);
                if let Ok((request, metadata)) = dns.recv() {
                    let mut response = request.to_vec();
                    if response.len() >= 12 {
                        queries.fetch_add(1, Ordering::Relaxed);
                        response[2..4].copy_from_slice(&[0x81, 0x80]);
                        response[6..8].copy_from_slice(&[0, 1]);
                        if ipv6 {
                            response
                                .extend_from_slice(&[0xc0, 0x0c, 0, 28, 0, 1, 0, 0, 0, 30, 0, 16]);
                            response.extend_from_slice(
                                &"fd77::1".parse::<std::net::Ipv6Addr>().unwrap().octets(),
                            );
                        } else {
                            response.extend_from_slice(&[
                                0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 30, 0, 4, 10, 77, 0, 1,
                            ]);
                        }
                        dns.send_slice(&response, metadata.endpoint).unwrap();
                    }
                }
                let echo = engine.sockets.get_mut::<tcp::Socket>(echo_handle);
                if pending_echo.len() == offset && echo.can_recv() {
                    pending_echo = echo.recv(|data| (data.len(), data.to_vec())).unwrap();
                    offset = 0;
                }
                if echo.can_send() && offset < pending_echo.len() {
                    offset += echo.send_slice(&pending_echo[offset..]).unwrap();
                }
                if echo.state() == tcp::State::CloseWait
                    && !echo.can_recv()
                    && offset == pending_echo.len()
                {
                    echo.close();
                }
                engine
                    .iface
                    .poll(engine.now(), &mut engine.device, &mut engine.sockets);
                if engine.udp.peer_addr().is_ok() {
                    engine.flush(&mut output).await?;
                }
            }
        });
        Self {
            endpoint,
            dns_queries,
            stop,
            task,
        }
    }

    async fn shutdown(self) {
        let _ = self.stop.send(());
        self.task.await.unwrap().unwrap();
    }
}

#[tokio::test]
async fn host_owned_address_is_rejected_before_endpoint_dns_or_handshake() {
    let mut config = profile(
        CLIENT_KEY,
        SERVER_KEY,
        "127.0.0.1",
        "127.0.0.1:9".parse().unwrap(),
        false,
    );
    config.peer.endpoint = Endpoint::Host {
        name: "must-not-resolve.invalid".into(),
        port: 51820,
    };
    let error = match TunnelSession::start(config).await {
        Ok(_) => panic!("must not start a tunnel using a host-owned address"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(message.contains("127.0.0.1"));
    assert!(message.contains("ya está activa en el sistema"));
    assert!(message.contains("cliente oficial de WireGuard"));
}

#[tokio::test]
async fn real_handshake_tunneled_dns_tcp_and_half_close() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let peer = PeerLab::start().await;
        let session = TunnelSession::start(profile(
            CLIENT_KEY,
            SERVER_KEY,
            "10.77.0.2",
            peer.endpoint,
            true,
        ))
        .await
        .unwrap();
        session
            .handle
            .wait_for_handshake(Duration::from_secs(5))
            .await
            .unwrap();
        let address = session
            .handle
            .resolve("only-in-tunnel.invalid".into())
            .await
            .unwrap();
        assert_eq!(address, "10.77.0.1".parse::<IpAddr>().unwrap());
        assert!(peer.dns_queries.load(Ordering::Relaxed) > 0);
        let stream = session
            .handle
            .connect(SocketAddr::new(address, 8080))
            .await
            .unwrap();
        let (mut read, mut write) = tokio::io::split(stream);
        let payload: Vec<u8> = (0..200_000).map(|index| (index % 251) as u8).collect();
        let expected = payload.clone();
        let sender = tokio::spawn(async move {
            write.write_all(&payload).await.unwrap();
            write.shutdown().await.unwrap();
        });
        let mut response = vec![];
        read.read_to_end(&mut response).await.unwrap();
        sender.await.unwrap();
        assert_eq!(response, expected);
        assert!(session.handle.status().borrow().tx_bytes > 0);
        session.shutdown().await.unwrap();
        peer.shutdown().await;
    })
    .await
    .expect("local tunnel integration test timed out");
}

#[tokio::test]
async fn rejects_disallowed_destination_without_direct_fallback() {
    let peer = PeerLab::start().await;
    let session = TunnelSession::start(profile(
        CLIENT_KEY,
        SERVER_KEY,
        "10.77.0.2",
        peer.endpoint,
        false,
    ))
    .await
    .unwrap();
    let denied = session
        .handle
        .connect("127.0.0.1:8080".parse().unwrap())
        .await
        .unwrap_err();
    assert_eq!(denied.kind(), io::ErrorKind::PermissionDenied);
    let no_dns = session
        .handle
        .resolve("example.com".into())
        .await
        .unwrap_err();
    assert_eq!(no_dns.kind(), io::ErrorKind::Other);
    session.shutdown().await.unwrap();
    peer.shutdown().await;
}

#[tokio::test]
async fn handshake_timeout_is_not_reported_as_connected() {
    // Keep the port open but do not answer, avoiding platform-specific ICMP behavior.
    let blackhole = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let session = TunnelSession::start(profile(
        CLIENT_KEY,
        SERVER_KEY,
        "10.77.0.2",
        blackhole.local_addr().unwrap(),
        false,
    ))
    .await
    .unwrap();
    assert!(session
        .handle
        .wait_for_handshake(Duration::from_millis(150))
        .await
        .is_err());
    assert_eq!(
        session.handle.status().borrow().state,
        ConnectionState::Connecting
    );
    session.shutdown().await.unwrap();
}

#[tokio::test]
async fn stopping_tunnel_invalidates_handles() {
    let peer = PeerLab::start().await;
    let session = TunnelSession::start(profile(
        CLIENT_KEY,
        SERVER_KEY,
        "10.77.0.2",
        peer.endpoint,
        false,
    ))
    .await
    .unwrap();
    let handle = session.handle.clone();
    session.shutdown().await.unwrap();
    assert_eq!(
        handle
            .connect("10.77.0.1:8080".parse().unwrap())
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
    assert!(handle
        .wait_for_handshake(Duration::from_secs(1))
        .await
        .is_err());
    peer.shutdown().await;
}

#[tokio::test]
async fn ipv6_dns_and_tcp_use_the_memory_stack() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let peer = PeerLab::start_with_ipv6(true).await;
        let session = TunnelSession::start(profile(
            CLIENT_KEY,
            SERVER_KEY,
            "fd77::2",
            peer.endpoint,
            true,
        ))
        .await
        .unwrap();
        session
            .handle
            .wait_for_handshake(Duration::from_secs(5))
            .await
            .unwrap();
        let ip = session
            .handle
            .resolve("ipv6-only.invalid".into())
            .await
            .unwrap();
        assert_eq!(ip, "fd77::1".parse::<IpAddr>().unwrap());
        let mut stream = session
            .handle
            .connect(SocketAddr::new(ip, 8080))
            .await
            .unwrap();
        stream.write_all(b"IPv6 through WireGuard").await.unwrap();
        stream.shutdown().await.unwrap();
        let mut echoed = vec![];
        stream.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, b"IPv6 through WireGuard");
        session.shutdown().await.unwrap();
        peer.shutdown().await;
    })
    .await
    .expect("IPv6 tunnel test timed out");
}
