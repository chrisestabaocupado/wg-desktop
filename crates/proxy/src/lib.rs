#![forbid(unsafe_code)]

use std::{
    future::Future,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream},
    net::TcpListener,
    sync::Semaphore,
    task::JoinSet,
};
use wg_tunnel::TunnelHandle;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CLIENTS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Target {
    Address(SocketAddr),
    Domain(String, u16),
}

trait Connector: Clone + Send + Sync + 'static {
    type Stream: AsyncRead + AsyncWrite + Unpin + Send;
    fn connect(&self, target: Target) -> impl Future<Output = io::Result<Self::Stream>> + Send;
}

impl Connector for TunnelHandle {
    type Stream = DuplexStream;

    async fn connect(&self, target: Target) -> io::Result<Self::Stream> {
        let destination = match target {
            Target::Address(address) => address,
            Target::Domain(name, port) => SocketAddr::new(self.resolve(name).await?, port),
        };
        TunnelHandle::connect(self, destination).await
    }
}

/// The only production connector is TunnelHandle. There is no direct TCP fallback.
pub async fn serve(
    listener: TcpListener,
    tunnel: TunnelHandle,
    shutdown: impl Future<Output = ()>,
) -> io::Result<()> {
    if !listener.local_addr()?.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "proxy must listen on loopback",
        ));
    }
    let clients = Arc::new(Semaphore::new(MAX_CLIENTS));
    let mut tasks = JoinSet::new();
    tokio::pin!(shutdown);
    let outcome = loop {
        tokio::select! {
            _ = &mut shutdown => break Ok(()),
            result = tasks.join_next(), if !tasks.is_empty() => {
                if result.is_some_and(|result| result.is_err()) { tracing::warn!("Proxy client task failed"); }
            },
            result = listener.accept() => {
                let (stream, _) = match result { Ok(client) => client, Err(error) => break Err(error) };
                let Ok(permit) = clients.clone().try_acquire_owned() else { drop(stream); continue; };
                let connector = tunnel.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    if let Err(error) = handle_client(stream, connector).await {
                        // Do not log destination names, application data, or profile contents.
                        tracing::debug!(kind = ?error.kind(), "Proxy connection ended");
                    }
                });
            }
        }
    };
    tasks.shutdown().await;
    outcome
}

async fn handle_client<S: AsyncRead + AsyncWrite + Unpin, C: Connector>(
    mut client: S,
    connector: C,
) -> io::Result<()> {
    let target = tokio::time::timeout(HANDSHAKE_TIMEOUT, negotiate(&mut client))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "SOCKS handshake timed out"))??;
    let connected = tokio::time::timeout(CONNECT_TIMEOUT, connector.connect(target)).await;
    let mut remote = match connected {
        Ok(Ok(remote)) => remote,
        Ok(Err(error)) => {
            reply(&mut client, reply_code(&error)).await?;
            return Err(error);
        }
        Err(_) => {
            reply(&mut client, 0x06).await?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "tunnel connection timed out",
            ));
        }
    };
    // A success response is never sent before the remote tunnel TCP handshake completes.
    reply(&mut client, 0).await?;
    tokio::io::copy_bidirectional(&mut client, &mut remote).await?;
    Ok(())
}

async fn negotiate<S: AsyncRead + AsyncWrite + Unpin>(client: &mut S) -> io::Result<Target> {
    if client.read_u8().await? != 5 {
        return Err(invalid("only SOCKS5 is supported"));
    }
    let methods_len = usize::from(client.read_u8().await?);
    let mut methods = vec![0; methods_len];
    client.read_exact(&mut methods).await?;
    if !methods.contains(&0) {
        client.write_all(&[5, 0xff]).await?;
        return Err(invalid("no supported authentication method"));
    }
    client.write_all(&[5, 0]).await?;
    let mut request = [0; 4];
    client.read_exact(&mut request).await?;
    if request[0] != 5 || request[2] != 0 {
        return Err(invalid("invalid SOCKS request"));
    }
    if request[1] != 1 {
        reply(client, 7).await?;
        return Err(invalid("only SOCKS CONNECT is supported"));
    }
    let target = match request[3] {
        1 => {
            let mut bytes = [0; 4];
            client.read_exact(&mut bytes).await?;
            Target::Address(SocketAddr::new(
                IpAddr::V4(Ipv4Addr::from(bytes)),
                client.read_u16().await?,
            ))
        }
        4 => {
            let mut bytes = [0; 16];
            client.read_exact(&mut bytes).await?;
            Target::Address(SocketAddr::new(
                IpAddr::V6(Ipv6Addr::from(bytes)),
                client.read_u16().await?,
            ))
        }
        3 => {
            let length = usize::from(client.read_u8().await?);
            let mut bytes = vec![0; length];
            client.read_exact(&mut bytes).await?;
            if bytes.is_empty() || !bytes.is_ascii() || bytes.contains(&0) {
                reply(client, 8).await?;
                return Err(invalid("invalid SOCKS domain"));
            }
            Target::Domain(
                String::from_utf8(bytes).map_err(|_| invalid("invalid SOCKS domain"))?,
                client.read_u16().await?,
            )
        }
        _ => {
            reply(client, 8).await?;
            return Err(invalid("unsupported SOCKS address type"));
        }
    };
    let port = match &target {
        Target::Address(address) => address.port(),
        Target::Domain(_, port) => *port,
    };
    if port == 0 {
        reply(client, 2).await?;
        return Err(invalid("destination port cannot be zero"));
    }
    Ok(target)
}

async fn reply<S: AsyncWrite + Unpin>(client: &mut S, code: u8) -> io::Result<()> {
    client.write_all(&[5, code, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

fn reply_code(error: &io::Error) -> u8 {
    match error.kind() {
        io::ErrorKind::PermissionDenied => 2,
        io::ErrorKind::NotFound | io::ErrorKind::AddrNotAvailable => 4,
        io::ErrorKind::ConnectionRefused => 5,
        io::ErrorKind::TimedOut => 6,
        _ => 1,
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Clone)]
    struct Mock {
        targets: Arc<Mutex<Vec<Target>>>,
        denied: bool,
    }

    impl Connector for Mock {
        type Stream = DuplexStream;
        async fn connect(&self, target: Target) -> io::Result<DuplexStream> {
            self.targets.lock().unwrap().push(target);
            if self.denied {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
            }
            let (stream, mut echo) = tokio::io::duplex(1024);
            tokio::spawn(async move {
                let (mut read, mut write) = tokio::io::split(&mut echo);
                tokio::io::copy(&mut read, &mut write).await.unwrap();
            });
            Ok(stream)
        }
    }

    async fn greeting(client: &mut DuplexStream) {
        client.write_all(&[5, 1, 0]).await.unwrap();
        let mut reply = [0; 2];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [5, 0]);
    }

    #[tokio::test]
    async fn forwards_domain_without_resolving_it_locally_and_relays_half_close() {
        let targets = Arc::new(Mutex::new(vec![]));
        let connector = Mock {
            targets: targets.clone(),
            denied: false,
        };
        let (mut client, server) = tokio::io::duplex(1024);
        let task = tokio::spawn(handle_client(server, connector));
        greeting(&mut client).await;
        client.write_all(&[5, 1, 0, 3, 11]).await.unwrap();
        client.write_all(b"example.com").await.unwrap();
        client.write_u16(443).await.unwrap();
        let mut response = [0; 10];
        client.read_exact(&mut response).await.unwrap();
        assert_eq!(response[1], 0);
        client.write_all(b"hello through tunnel").await.unwrap();
        client.shutdown().await.unwrap();
        let mut echoed = vec![];
        client.read_to_end(&mut echoed).await.unwrap();
        assert_eq!(echoed, b"hello through tunnel");
        task.await.unwrap().unwrap();
        assert_eq!(
            *targets.lock().unwrap(),
            [Target::Domain("example.com".into(), 443)]
        );
    }

    #[tokio::test]
    async fn denied_destination_never_gets_success_or_direct_fallback() {
        let targets = Arc::new(Mutex::new(vec![]));
        let (mut client, server) = tokio::io::duplex(1024);
        let task = tokio::spawn(handle_client(
            server,
            Mock {
                targets: targets.clone(),
                denied: true,
            },
        ));
        greeting(&mut client).await;
        client
            .write_all(&[5, 1, 0, 1, 1, 1, 1, 1, 0, 80])
            .await
            .unwrap();
        let mut reply = [0; 10];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 2);
        assert!(task.await.unwrap().is_err());
        assert_eq!(targets.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rejects_udp_without_contacting_connector() {
        let targets = Arc::new(Mutex::new(vec![]));
        let (mut client, server) = tokio::io::duplex(1024);
        let task = tokio::spawn(handle_client(
            server,
            Mock {
                targets: targets.clone(),
                denied: false,
            },
        ));
        greeting(&mut client).await;
        client.write_all(&[5, 3, 0, 1]).await.unwrap();
        let mut reply = [0; 10];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply[1], 7);
        assert!(task.await.unwrap().is_err());
        assert!(targets.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn rejects_unsupported_authentication() {
        let (mut client, mut server) = tokio::io::duplex(1024);
        let task = tokio::spawn(async move { negotiate(&mut server).await });
        client.write_all(&[5, 1, 2]).await.unwrap();
        let mut reply = [0; 2];
        client.read_exact(&mut reply).await.unwrap();
        assert_eq!(reply, [5, 255]);
        assert!(task.await.unwrap().is_err());
    }
}
