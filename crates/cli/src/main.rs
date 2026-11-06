mod gui;
mod storage;

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{ensure, Context, Result};
use clap::{Args, Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use wg_core::{ConnectionState, Profile};
use wg_tunnel::{Snapshot, TunnelSession};
use zeroize::Zeroizing;

#[derive(Parser)]
#[command(
    version,
    about = "Experimental WireGuard desktop engine — Windows-first, Rust",
    long_about = "WireGuard desktop prototype. Run without arguments to open the graphical interface. SOCKS5 TCP proxy is available; system VPN and kill switch are not implemented yet. Proxy mode does not alter Windows routes, DNS or proxy settings."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Open the desktop interface (also the default when double-clicking the executable).
    Gui {
        #[arg(long, hide = true)]
        smoke_test: bool,
    },
    /// Validate a configuration and show a summary without private keys.
    Validate {
        #[command(flatten)]
        source: Source,
    },
    /// Store a validated configuration encrypted for the current Windows user.
    Import {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        name: String,
    },
    /// List names of protected profiles.
    List,
    /// Start an experimental local SOCKS5 TCP proxy over WireGuard.
    Proxy {
        #[command(flatten)]
        source: Source,
        #[arg(long, default_value = "127.0.0.1:1080")]
        listen: SocketAddr,
        #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(1..=120))]
        handshake_timeout: u64,
    },
    /// Show implemented capabilities and important limitations.
    Doctor,
}

#[derive(Args)]
#[group(required = true, multiple = false)]
struct Source {
    /// Read a UTF-8 WireGuard .conf file without importing it.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Use a previously imported DPAPI-protected profile.
    #[arg(long)]
    profile: Option<String>,
}

impl Source {
    fn read(&self) -> Result<Zeroizing<Vec<u8>>> {
        match (&self.config, &self.profile) {
            (Some(path), None) => storage::read_config(path),
            (None, Some(name)) => storage::load(name),
            _ => anyhow::bail!("specify exactly one of --config or --profile"),
        }
    }

    fn parse(&self) -> Result<Profile> {
        storage::parse(&self.read()?)
    }
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let desktop = std::env::args_os().len() == 1;
    let show_error_dialog = (desktop || std::env::args_os().nth(1).is_some_and(|arg| arg == "gui"))
        && !std::env::args_os().any(|arg| arg == "--smoke-test");
    if desktop {
        wg_windows::detach_console();
    }
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("warn,wg_desktop=info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let result = if desktop {
        gui::launch(false)
    } else {
        run(Cli::parse()).await
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            if show_error_dialog {
                rfd::MessageDialog::new()
                    .set_title("WgDesktop — no se pudo abrir")
                    .set_description(format!("{error:#}\n\nPrueba actualizar el controlador gráfico. Los comandos de consola siguen disponibles."))
                    .set_level(rfd::MessageLevel::Error)
                    .show();
            }
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Gui { smoke_test } => gui::launch(smoke_test)?,
        Command::Validate { source } => {
            let profile = source.parse()?;
            println!("{}", serde_json::to_string_pretty(&profile.summary())?);
        }
        Command::Import { config, name } => {
            let bytes = storage::read_config(&config)?;
            storage::import(&name, &bytes)?;
            println!("Profile imported with current-user DPAPI protection. Original .conf was not modified or deleted.");
        }
        Command::List => {
            for name in storage::list()? {
                println!("{name}");
            }
        }
        Command::Proxy {
            source,
            listen,
            handshake_timeout,
        } => {
            ensure!(
                listen.ip().is_loopback(),
                "proxy must listen only on a loopback address"
            );
            let profile = source.parse()?;
            let listener = tokio::net::TcpListener::bind(listen)
                .await
                .context("cannot bind local proxy port")?;
            let bound = listener.local_addr()?;
            tracing::info!("Starting experimental WireGuard proxy; Windows network settings will not be changed");
            let session = TunnelSession::start(profile).await?;
            let ready = tokio::select! {
                result = session.handle.wait_for_handshake(Duration::from_secs(handshake_timeout)) => result,
                result = tokio::signal::ctrl_c() => { result?; session.shutdown().await?; return Ok(()); }
            };
            if let Err(error) = ready {
                let _ = session.shutdown().await;
                return Err(error);
            }
            tracing::info!(%bound, "WireGuard handshake established; SOCKS5 TCP proxy ready");
            tracing::info!("Configure SOCKS5 remote DNS (curl --socks5-hostname). Browser traffic outside the proxy is not protected.");
            let status = session.handle.status();
            let serving =
                wg_proxy::serve(listener, session.handle.clone(), stop_signal(status)).await;
            let stopped = session.shutdown().await;
            serving.context("proxy listener failed")?;
            stopped?;
        }
        Command::Doctor => {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "os": std::env::consts::OS,
                    "architecture": std::env::consts::ARCH,
                    "proxy": "experimental SOCKS5 CONNECT over userspace WireGuard",
                    "destination_dns": "tunnel-only when supplied as a SOCKS5 domain",
                    "endpoint_dns": "system bootstrap DNS if Endpoint is a hostname",
                    "protected_storage": cfg!(windows),
                    "system_vpn": false,
                    "kill_switch": false,
                    "gui": true,
                    "changes_system_network_settings": false
                }))?
            );
        }
    }
    Ok(())
}

async fn stop_signal(mut status: tokio::sync::watch::Receiver<Snapshot>) {
    let interrupt = tokio::signal::ctrl_c();
    tokio::pin!(interrupt);
    loop {
        tokio::select! {
            result = &mut interrupt => {
                if result.is_err() { tracing::error!("Cannot listen for Ctrl+C; stopping proxy safely"); }
                return;
            }
            changed = status.changed() => {
                if changed.is_err() || matches!(status.borrow().state, ConnectionState::Error | ConnectionState::Disconnected) {
                    tracing::warn!("Tunnel stopped; closing proxy without a direct fallback");
                    return;
                }
            }
        }
    }
}
