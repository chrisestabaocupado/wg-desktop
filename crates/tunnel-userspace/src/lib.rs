#![forbid(unsafe_code)]

mod device;

#[cfg(test)]
mod tests;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

use anyhow::{bail, Context as _, Result};
use boringtun::{
    noise::{Tunn, TunnResult},
    x25519::{PublicKey, StaticSecret},
};
use device::MemoryDevice;
use serde::Serialize;
use smoltcp::{
    iface::{Config, Interface, SocketHandle, SocketSet},
    socket::{dns, tcp},
    time::{Duration as StackDuration, Instant as StackInstant},
    wire::{DnsQueryType, HardwareAddress, IpAddress, IpCidr},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, DuplexStream, ReadBuf},
    net::UdpSocket,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};
use wg_core::{ConnectionState, Endpoint, Profile};

const MAX_CONNECTIONS: usize = 128;
const BUFFER_SIZE: usize = 65_536;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const DNS_TIMEOUT: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub state: ConnectionState,
    pub seconds_since_handshake: Option<u64>,
    pub tx_bytes: usize,
    pub rx_bytes: usize,
    pub active_connections: usize,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            state: ConnectionState::Connecting,
            seconds_since_handshake: None,
            tx_bytes: 0,
            rx_bytes: 0,
            active_connections: 0,
        }
    }
}

#[derive(Clone)]
pub struct TunnelHandle {
    commands: mpsc::Sender<Command>,
    status: watch::Receiver<Snapshot>,
}

impl TunnelHandle {
    /// This never uses an OS TCP socket. The returned stream terminates in smoltcp.
    pub async fn connect(&self, destination: SocketAddr) -> io::Result<DuplexStream> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::Connect { destination, reply })
            .await
            .map_err(|_| stopped())?;
        result.await.map_err(|_| stopped())?
    }

    /// Destination names are resolved only by the DNS socket inside the tunnel.
    pub async fn resolve(&self, name: String) -> io::Result<IpAddr> {
        let (reply, result) = oneshot::channel();
        self.commands
            .send(Command::Resolve { name, reply })
            .await
            .map_err(|_| stopped())?;
        result.await.map_err(|_| stopped())?
    }

    pub fn status(&self) -> watch::Receiver<Snapshot> {
        self.status.clone()
    }

    pub async fn wait_for_handshake(&self, limit: Duration) -> Result<()> {
        let mut status = self.status();
        tokio::time::timeout(limit, async {
            loop {
                {
                    let snapshot = status.borrow();
                    if matches!(
                        snapshot.state,
                        ConnectionState::Error | ConnectionState::Disconnected
                    ) {
                        bail!("tunnel stopped before handshake");
                    }
                    if snapshot.seconds_since_handshake.is_some() {
                        return Ok(());
                    }
                }
                status
                    .changed()
                    .await
                    .context("tunnel stopped before handshake")?;
            }
        })
        .await
        .context("WireGuard handshake timed out")?
    }
}

pub struct TunnelSession {
    pub handle: TunnelHandle,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<()>>>,
}

impl TunnelSession {
    pub async fn start(profile: Profile) -> Result<Self> {
        // These addresses belong to the in-memory stack. Reusing one already owned
        // by a host interface can run two clients for the same WireGuard identity.
        // Binding a temporary UDP socket checks local ownership without sending
        // traffic, changing routes, or reading keys from another client.
        for address in &profile.addresses {
            if std::net::UdpSocket::bind(SocketAddr::new(address.addr(), 0)).is_ok() {
                bail!(
                    "La dirección del perfil {} ya está activa en el sistema. Desactiva el túnel de ese perfil en el cliente oficial de WireGuard y vuelve a conectar WgDesktop. No uses el mismo perfil en ambos clientes a la vez.",
                    address.addr()
                );
            }
        }
        let endpoint = match &profile.peer.endpoint {
            Endpoint::Address(address) => *address,
            Endpoint::Host { name, port } => {
                tracing::warn!("Resolving WireGuard endpoint using bootstrap system DNS; destination DNS stays inside the tunnel");
                tokio::time::timeout(
                    Duration::from_secs(10),
                    tokio::net::lookup_host((name.as_str(), *port)),
                )
                .await
                .context("endpoint DNS timed out")??
                .next()
                .context("endpoint DNS returned no addresses")?
            }
        };
        if endpoint.ip().is_unspecified() || endpoint.ip().is_multicast() {
            bail!("WireGuard endpoint resolved to an unusable address");
        }
        let bind = SocketAddr::new(
            if endpoint.is_ipv4() {
                "0.0.0.0".parse()?
            } else {
                "::".parse()?
            },
            profile.listen_port,
        );
        let udp = UdpSocket::bind(bind)
            .await
            .context("cannot bind WireGuard UDP transport")?;
        udp.connect(endpoint)
            .await
            .context("cannot connect WireGuard UDP transport")?;
        let (commands, receiver) = mpsc::channel(64);
        let (status_tx, status) = watch::channel(Snapshot::default());
        let (stop, stopping) = oneshot::channel();
        let engine = Engine::new(profile, udp)?;
        let task = tokio::spawn(async move {
            let result = engine.run(receiver, stopping, &status_tx).await;
            status_tx.send_modify(|snapshot| {
                snapshot.state = if result.is_ok() {
                    ConnectionState::Disconnected
                } else {
                    ConnectionState::Error
                };
                snapshot.active_connections = 0;
            });
            result
        });
        Ok(Self {
            handle: TunnelHandle { commands, status },
            stop: Some(stop),
            task: Some(task),
        })
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.await.context("tunnel task panicked")??;
        }
        Ok(())
    }
}

impl Drop for TunnelSession {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

enum Command {
    Connect {
        destination: SocketAddr,
        reply: oneshot::Sender<io::Result<DuplexStream>>,
    },
    Resolve {
        name: String,
        reply: oneshot::Sender<io::Result<IpAddr>>,
    },
}

struct Connection {
    socket: SocketHandle,
    bridge: DuplexStream,
    pending: Vec<u8>,
    pending_offset: usize,
    ready: Option<(oneshot::Sender<io::Result<DuplexStream>>, DuplexStream)>,
    deadline: Instant,
    local_eof: bool,
    remote_eof: bool,
}

struct Query {
    handle: dns::QueryHandle,
    name: String,
    try_ipv6: bool,
    reply: oneshot::Sender<io::Result<IpAddr>>,
    deadline: Instant,
}

struct Engine {
    profile: Profile,
    udp: UdpSocket,
    tunnel: Tunn,
    device: MemoryDevice,
    iface: Interface,
    sockets: SocketSet<'static>,
    dns: SocketHandle,
    connections: Vec<Connection>,
    queries: Vec<Query>,
    origin: Instant,
    next_port: u16,
}

impl Engine {
    fn new(profile: Profile, udp: UdpSocket) -> Result<Self> {
        let tunnel = Tunn::new(
            StaticSecret::from(*profile.private_key.expose()),
            PublicKey::from(*profile.peer.public_key.expose()),
            profile.peer.preshared_key.as_ref().map(|key| *key.expose()),
            profile.peer.persistent_keepalive,
            rand::random::<u32>() & 0x00ff_ffff,
            None,
        );
        let mut device = MemoryDevice::new(profile.mtu);
        let mut config = Config::new(HardwareAddress::Ip);
        config.random_seed = rand::random();
        let mut iface = Interface::new(config, &mut device, StackInstant::ZERO);
        for address in &profile.addresses {
            iface.update_ip_addrs(|addresses| {
                addresses
                    .push(IpCidr::new(
                        IpAddress::from(address.addr()),
                        address.prefix_len(),
                    ))
                    .expect("profile address limit validated");
            });
            // On an IP medium the next hop is a routing hint, not an ARP destination.
            match address.addr() {
                IpAddr::V4(ip) => {
                    iface
                        .routes_mut()
                        .add_default_ipv4_route(ip)
                        .map_err(|_| anyhow::anyhow!("cannot add memory IPv4 route"))?;
                }
                IpAddr::V6(ip) => {
                    iface
                        .routes_mut()
                        .add_default_ipv6_route(ip)
                        .map_err(|_| anyhow::anyhow!("cannot add memory IPv6 route"))?;
                }
            }
        }
        let mut sockets = SocketSet::new(vec![]);
        let servers: Vec<_> = profile.dns.iter().copied().map(IpAddress::from).collect();
        let dns = sockets.add(dns::Socket::new(&servers, vec![]));
        Ok(Self {
            profile,
            udp,
            tunnel,
            device,
            iface,
            sockets,
            dns,
            connections: vec![],
            queries: vec![],
            origin: Instant::now(),
            next_port: 49152,
        })
    }

    fn now(&self) -> StackInstant {
        StackInstant::from_millis(self.origin.elapsed().as_millis() as i64)
    }

    async fn run(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        mut stop: oneshot::Receiver<()>,
        status: &watch::Sender<Snapshot>,
    ) -> Result<()> {
        let mut output = vec![0; BUFFER_SIZE + 256];
        let first = self.tunnel.format_handshake_initiation(&mut output, false);
        if let TunnResult::WriteToNetwork(packet) = first {
            self.udp.send(packet).await?;
        }
        let mut interval = tokio::time::interval(Duration::from_millis(5));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut input = vec![0; BUFFER_SIZE + 256];
        let mut last_timers = Instant::now();
        let mut last_status = Instant::now() - Duration::from_secs(1);
        loop {
            tokio::select! {
                _ = &mut stop => return Ok(()),
                command = commands.recv() => match command {
                    Some(command) => self.command(command),
                    None => return Ok(()),
                },
                packet = self.udp.recv(&mut input) => {
                    let size = packet.context("WireGuard UDP receive failed")?;
                    self.receive(&input[..size], &mut output).await?;
                },
                _ = interval.tick() => {}
            }
            if last_timers.elapsed() >= Duration::from_millis(100) {
                match self.tunnel.update_timers(&mut output) {
                    TunnResult::WriteToNetwork(packet) => {
                        self.udp.send(packet).await?;
                    }
                    TunnResult::Err(_) => {
                        tracing::debug!("WireGuard timer expired; awaiting handshake retry")
                    }
                    _ => {}
                }
                last_timers = Instant::now();
            }
            self.iface
                .poll(self.now(), &mut self.device, &mut self.sockets);
            self.poll_queries();
            self.poll_connections();
            self.iface
                .poll(self.now(), &mut self.device, &mut self.sockets);
            self.flush(&mut output).await?;
            if last_status.elapsed() >= Duration::from_millis(250) {
                let (handshake, tx_bytes, rx_bytes, _, _) = self.tunnel.stats();
                status.send_replace(Snapshot {
                    state: if handshake.is_some() {
                        ConnectionState::Connected
                    } else {
                        ConnectionState::Connecting
                    },
                    seconds_since_handshake: handshake.map(|age| age.as_secs()),
                    tx_bytes,
                    rx_bytes,
                    active_connections: self.connections.len(),
                });
                last_status = Instant::now();
            }
        }
    }

    async fn receive(&mut self, packet: &[u8], output: &mut [u8]) -> Result<()> {
        let mut input = packet;
        // BoringTun requires draining queued packets after WriteToNetwork.
        for _ in 0..260 {
            match self
                .tunnel
                .decapsulate(Some(self.udp.peer_addr()?.ip()), input, output)
            {
                TunnResult::WriteToNetwork(packet) => {
                    self.udp.send(packet).await?;
                }
                TunnResult::WriteToTunnelV4(packet, source) => {
                    if self.profile.allows(source.into()) {
                        self.device.receive_packet(packet);
                    }
                    break;
                }
                TunnResult::WriteToTunnelV6(packet, source) => {
                    if self.profile.allows(source.into()) {
                        self.device.receive_packet(packet);
                    }
                    break;
                }
                TunnResult::Done => break,
                TunnResult::Err(_) => {
                    tracing::debug!("Discarded invalid WireGuard packet");
                    break;
                }
            }
            input = &[];
        }
        Ok(())
    }

    async fn flush(&mut self, output: &mut [u8]) -> Result<()> {
        while let Some(packet) = self.device.outgoing.pop_front() {
            if !Tunn::dst_address(&packet).is_some_and(|ip| self.profile.allows(ip)) {
                continue;
            }
            match self.tunnel.encapsulate(&packet, output) {
                TunnResult::WriteToNetwork(packet) => {
                    self.udp.send(packet).await?;
                }
                TunnResult::Err(_) => bail!("WireGuard could not encapsulate packet"),
                _ => {}
            }
        }
        Ok(())
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Connect { destination, reply } => {
                if destination.port() == 0 || !self.profile.allows(destination.ip()) {
                    let _ = reply.send(Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "destination is outside AllowedIPs or has no local address family",
                    )));
                    return;
                }
                if self.connections.len() >= MAX_CONNECTIONS {
                    let _ = reply.send(Err(io::Error::other("tunnel connection limit reached")));
                    return;
                }
                let mut socket = tcp::Socket::new(
                    tcp::SocketBuffer::new(vec![0; BUFFER_SIZE]),
                    tcp::SocketBuffer::new(vec![0; BUFFER_SIZE]),
                );
                socket.set_timeout(Some(StackDuration::from_secs(60)));
                socket.set_nagle_enabled(false);
                // Never reuse a port while a socket (including TIME_WAIT) owns it.
                let mut port = self.next_port;
                for _ in 0..16384 {
                    if !self.sockets.iter().any(|(_, socket)| match socket {
                        smoltcp::socket::Socket::Tcp(tcp) => {
                            tcp.local_endpoint().is_some_and(|local| local.port == port)
                        }
                        _ => false,
                    }) {
                        break;
                    }
                    port = if port == u16::MAX { 49152 } else { port + 1 };
                }
                self.next_port = if port == u16::MAX { 49152 } else { port + 1 };
                if socket
                    .connect(
                        self.iface.context(),
                        (IpAddress::from(destination.ip()), destination.port()),
                        port,
                    )
                    .is_err()
                {
                    let _ = reply.send(Err(io::Error::other("cannot create tunnel TCP socket")));
                    return;
                }
                let handle = self.sockets.add(socket);
                let (bridge, client) = tokio::io::duplex(BUFFER_SIZE);
                self.connections.push(Connection {
                    socket: handle,
                    bridge,
                    pending: vec![],
                    pending_offset: 0,
                    ready: Some((reply, client)),
                    deadline: Instant::now() + CONNECT_TIMEOUT,
                    local_eof: false,
                    remote_eof: false,
                });
            }
            Command::Resolve { name, reply } => {
                if self.profile.dns.is_empty() || self.queries.len() >= MAX_CONNECTIONS {
                    let _ = reply.send(Err(io::Error::other(
                        "no tunnel DNS configured or DNS query limit reached",
                    )));
                    return;
                }
                let v4 = self
                    .profile
                    .addresses
                    .iter()
                    .any(|address| address.addr().is_ipv4());
                let v6 = self
                    .profile
                    .addresses
                    .iter()
                    .any(|address| address.addr().is_ipv6());
                let query_type = if v4 {
                    DnsQueryType::A
                } else {
                    DnsQueryType::Aaaa
                };
                match self.sockets.get_mut::<dns::Socket>(self.dns).start_query(
                    self.iface.context(),
                    &name,
                    query_type,
                ) {
                    Ok(handle) => self.queries.push(Query {
                        handle,
                        name,
                        try_ipv6: v4 && v6,
                        reply,
                        deadline: Instant::now() + DNS_TIMEOUT,
                    }),
                    Err(_) => {
                        let _ = reply.send(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "invalid destination DNS name",
                        )));
                    }
                }
            }
        }
    }

    fn poll_queries(&mut self) {
        let mut pending = Vec::new();
        let dns = self.sockets.get_mut::<dns::Socket>(self.dns);
        for mut query in self.queries.drain(..) {
            if query.reply.is_closed() || Instant::now() >= query.deadline {
                dns.cancel_query(query.handle);
                let _ = query.reply.send(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "tunnel DNS timed out",
                )));
                continue;
            }
            let result = dns.get_query_result(query.handle);
            if matches!(result, Err(dns::GetQueryResultError::Pending)) {
                pending.push(query);
                continue;
            }
            if let Ok(addresses) = result {
                if let Some(ip) = addresses
                    .iter()
                    .map(|ip| IpAddr::from(*ip))
                    .find(|ip| self.profile.allows(*ip))
                {
                    let _ = query.reply.send(Ok(ip));
                    continue;
                }
            }
            if query.try_ipv6 {
                query.try_ipv6 = false;
                if let Ok(handle) =
                    dns.start_query(self.iface.context(), &query.name, DnsQueryType::Aaaa)
                {
                    query.handle = handle;
                    pending.push(query);
                    continue;
                }
            }
            let _ = query.reply.send(Err(io::Error::new(
                io::ErrorKind::NotFound,
                "tunnel DNS returned no permitted address",
            )));
        }
        self.queries = pending;
    }

    fn poll_connections(&mut self) {
        let mut cx = Context::from_waker(Waker::noop());
        let mut remaining = Vec::new();
        for mut connection in self.connections.drain(..) {
            let socket = self.sockets.get_mut::<tcp::Socket>(connection.socket);
            let expired = connection.ready.as_ref().is_some_and(|(reply, _)| {
                reply.is_closed() || Instant::now() >= connection.deadline
            });
            if expired {
                if let Some((reply, _)) = connection.ready.take() {
                    let _ = reply.send(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "tunnel TCP connection timed out",
                    )));
                }
                socket.abort();
            }
            if socket.state() == tcp::State::Established {
                if let Some((reply, client)) = connection.ready.take() {
                    if reply.send(Ok(client)).is_err() {
                        socket.abort();
                    }
                }
            }
            if socket.state() == tcp::State::Closed {
                if let Some((reply, _)) = connection.ready.take() {
                    let _ = reply.send(Err(io::Error::new(
                        io::ErrorKind::ConnectionRefused,
                        "tunnel TCP connection closed",
                    )));
                }
                self.sockets.remove(connection.socket);
                continue;
            }
            if connection.ready.is_none() {
                if connection.pending_offset < connection.pending.len() && socket.can_send() {
                    if let Ok(written) =
                        socket.send_slice(&connection.pending[connection.pending_offset..])
                    {
                        connection.pending_offset += written;
                    }
                }
                if connection.pending_offset == connection.pending.len()
                    && !connection.local_eof
                    && socket.may_send()
                {
                    let mut buffer = [0; 16384];
                    let mut read = ReadBuf::new(&mut buffer);
                    match Pin::new(&mut connection.bridge).poll_read(&mut cx, &mut read) {
                        Poll::Ready(Ok(())) if read.filled().is_empty() => {
                            connection.local_eof = true
                        }
                        Poll::Ready(Ok(())) => {
                            connection.pending.clear();
                            connection.pending.extend_from_slice(read.filled());
                            connection.pending_offset = 0;
                        }
                        Poll::Ready(Err(_)) => {
                            socket.abort();
                            connection.local_eof = true;
                        }
                        Poll::Pending => {}
                    }
                }
                if connection.local_eof && connection.pending_offset == connection.pending.len() {
                    socket.close();
                }
                if socket.can_recv() {
                    let result = socket.recv(|data| {
                        match Pin::new(&mut connection.bridge).poll_write(&mut cx, data) {
                            Poll::Ready(Ok(written)) => (written, false),
                            Poll::Ready(Err(_)) => (0, true),
                            Poll::Pending => (0, false),
                        }
                    });
                    if matches!(result, Ok(true)) {
                        socket.abort();
                    }
                }
                if !socket.may_recv()
                    && !connection.remote_eof
                    && Pin::new(&mut connection.bridge)
                        .poll_shutdown(&mut cx)
                        .is_ready()
                {
                    connection.remote_eof = true;
                }
            }
            remaining.push(connection);
        }
        self.connections = remaining;
    }
}

fn stopped() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "tunnel is not running")
}
