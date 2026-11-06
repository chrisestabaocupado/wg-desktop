use std::{
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use anyhow::{ensure, Context, Result};
use eframe::egui;
use tokio::sync::{oneshot, watch};
use wg_core::{ConnectionState, Profile, ProfileSummary};
use wg_tunnel::{Snapshot, TunnelSession};

use crate::storage;

mod theme;
mod view;
mod widgets;

#[cfg(test)]
mod preview;

const WINDOW_SIZE: egui::Vec2 = egui::vec2(420.0, 600.0);

fn window_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(WINDOW_SIZE)
            .with_min_inner_size(WINDOW_SIZE)
            .with_max_inner_size(WINDOW_SIZE)
            .with_decorations(false)
            .with_resizable(false)
            .with_maximize_button(false),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    }
}

pub fn launch(smoke_test: bool) -> Result<()> {
    let runtime = tokio::runtime::Handle::current();
    let options = window_options();
    eframe::run_native(
        "WgDesktop",
        options,
        Box::new(move |context| {
            theme::apply(&context.egui_ctx);
            Ok(Box::new(Desktop::new(runtime, smoke_test)))
        }),
    )
    .map_err(|error| anyhow::anyhow!("No se pudo iniciar la ventana: {error}"))
}

#[derive(Clone)]
enum Source {
    File(PathBuf),
    Stored(String),
}

impl Source {
    fn profile(&self) -> Result<Profile> {
        let bytes = match self {
            Self::File(path) => storage::read_config(path)?,
            Self::Stored(name) => storage::load(name)?,
        };
        storage::parse(&bytes)
    }

    fn label(&self) -> String {
        match self {
            Self::File(path) => path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            Self::Stored(name) => name.clone(),
        }
    }
}

enum Event {
    Ready(SocketAddr, watch::Receiver<Snapshot>),
    Finished(std::result::Result<(), String>),
}

struct Desktop {
    runtime: tokio::runtime::Handle,
    events: mpsc::Receiver<Event>,
    sender: mpsc::Sender<Event>,
    source: Option<Source>,
    summary: Option<ProfileSummary>,
    profiles: Vec<String>,
    profile_name: String,
    profile_filter: String,
    tab: view::Tab,
    save_dialog: bool,
    focus_profile_name: bool,
    focus_port: bool,
    copied_address_until: Option<Instant>,
    port: String,
    stop: Option<oneshot::Sender<()>>,
    status: Option<watch::Receiver<Snapshot>>,
    bound: Option<SocketAddr>,
    session_started: Option<Instant>,
    disconnecting: bool,
    error: Option<String>,
    notice: Option<String>,
    smoke_deadline: Option<Instant>,
}

impl Desktop {
    fn new(runtime: tokio::runtime::Handle, smoke_test: bool) -> Self {
        let (sender, events) = mpsc::channel();
        let (profiles, error) = match storage::list() {
            Ok(profiles) => (profiles, None),
            Err(error) => (
                vec![],
                Some(format!("No se pudieron listar los perfiles: {error:#}")),
            ),
        };
        Self {
            runtime,
            events,
            sender,
            source: None,
            summary: None,
            profiles,
            profile_name: String::new(),
            profile_filter: String::new(),
            tab: view::Tab::Connection,
            save_dialog: false,
            focus_profile_name: false,
            focus_port: false,
            copied_address_until: None,
            port: "1080".into(),
            stop: None,
            status: None,
            bound: None,
            session_started: None,
            disconnecting: false,
            error,
            notice: None,
            smoke_deadline: smoke_test.then(|| Instant::now() + Duration::from_millis(800)),
        }
    }

    fn select(&mut self, source: Source) {
        self.notice = None;
        match source.profile() {
            Ok(profile) => {
                self.summary = Some(profile.summary());
                self.source = Some(source);
                self.error = None;
                self.tab = view::Tab::Connection;
                self.profile_name = self.source.as_ref().map_or_else(String::new, |source| {
                    source
                        .label()
                        .trim_end_matches(".conf")
                        .chars()
                        .map(|character| {
                            if character.is_ascii_alphanumeric()
                                || character == '-'
                                || character == '_'
                            {
                                character
                            } else {
                                '_'
                            }
                        })
                        .take(64)
                        .collect()
                });
            }
            Err(error) => {
                self.source = None;
                self.summary = None;
                self.error = Some(format!("No se pudo cargar el perfil: {error:#}"));
            }
        }
    }

    fn save(&mut self) -> Result<()> {
        let Some(Source::File(path)) = &self.source else {
            return Ok(());
        };
        storage::import(&self.profile_name, &storage::read_config(path)?)?;
        self.profiles = storage::list()?;
        self.select(Source::Stored(self.profile_name.clone()));
        self.save_dialog = false;
        self.notice =
            Some("Perfil cifrado con DPAPI. El archivo .conf original no fue modificado.".into());
        Ok(())
    }

    fn connect(&mut self) -> Result<()> {
        ensure!(!self.busy(), "Ya hay una conexión en curso");
        let source = self
            .source
            .clone()
            .context("Selecciona una configuración .conf o un perfil guardado")?;
        let port: u16 = self
            .port
            .trim()
            .parse()
            .context("El puerto debe ser un número entre 1 y 65535")?;
        ensure!(port != 0, "El puerto debe ser mayor que cero");
        let (stop, stopping) = oneshot::channel();
        self.stop = Some(stop);
        self.status = None;
        self.session_started = None;
        self.copied_address_until = None;
        self.error = None;
        self.notice = None;
        self.disconnecting = false;
        let sender = self.sender.clone();
        self.runtime.spawn(async move {
            let result = connect_proxy(source, port, &sender, stopping)
                .await
                .map_err(|error| format!("{error:#}"));
            let _ = sender.send(Event::Finished(result));
        });
        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
            self.disconnecting = true;
        }
    }

    fn process_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Ready(bound, status) => {
                    self.bound = Some(bound);
                    self.status = Some(status);
                    self.session_started = Some(Instant::now());
                }
                Event::Finished(result) => {
                    self.stop = None;
                    self.status = None;
                    self.bound = None;
                    self.session_started = None;
                    self.disconnecting = false;
                    match result {
                        Ok(()) => {
                            self.notice = Some(
                                "Desconectado. No se modificaron las rutas ni el DNS de Windows."
                                    .into(),
                            )
                        }
                        Err(error) => {
                            self.error = Some(format!("No se pudo mantener la conexión: {error}"))
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Desktop {
    fn drop(&mut self) {
        self.disconnect();
    }
}

impl eframe::App for Desktop {
    fn update(&mut self, context: &egui::Context, _: &mut eframe::Frame) {
        self.render(context);
    }
}

async fn connect_proxy(
    source: Source,
    port: u16,
    sender: &mpsc::Sender<Event>,
    mut stop: oneshot::Receiver<()>,
) -> Result<()> {
    let profile = source.profile()?;
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .await
        .context("No se pudo abrir el puerto del proxy; puede estar ocupado")?;
    let bound = listener.local_addr()?;
    let session = tokio::select! {
        biased;
        _ = &mut stop => return Ok(()),
        session = TunnelSession::start(profile) => session?,
    };
    let handshake = tokio::select! {
        _ = &mut stop => { session.shutdown().await?; return Ok(()); }
        result = session.handle.wait_for_handshake(Duration::from_secs(20)) => result,
    };
    if let Err(error) = handshake {
        let _ = session.shutdown().await;
        return Err(error).context(
            "El peer no respondió. Revisa el endpoint, las claves y el acceso UDP al servidor",
        );
    }
    let mut status = session.handle.status();
    if sender.send(Event::Ready(bound, status.clone())).is_err() {
        session.shutdown().await?;
        return Ok(());
    }
    let serving = wg_proxy::serve(listener, session.handle.clone(), async {
        loop {
            tokio::select! {
                _ = &mut stop => return,
                changed = status.changed() => {
                    if changed.is_err() || matches!(status.borrow().state, ConnectionState::Error | ConnectionState::Disconnected) { return; }
                }
            }
        }
    }).await;
    let stopped = session.shutdown().await;
    serving?;
    stopped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Source {
        Source::File(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/example.conf"),
        )
    }

    #[tokio::test]
    async fn window_can_render_without_configuration_or_network_startup() {
        let mut desktop = Desktop::new(tokio::runtime::Handle::current(), false);
        let context = egui::Context::default();
        let output = context.run(egui::RawInput::default(), |context| desktop.render(context));
        assert!(!output.shapes.is_empty());
        assert!(desktop.stop.is_none());
        assert!(desktop.source.is_none());
    }

    #[tokio::test]
    async fn invalid_selection_clears_previous_valid_configuration() {
        let mut desktop = Desktop::new(tokio::runtime::Handle::current(), false);
        desktop.select(fixture());
        assert!(desktop.summary.is_some());
        desktop.select(Source::File(PathBuf::from(
            "missing-wg-desktop-test-profile.conf",
        )));
        assert!(desktop.error.is_some());
        assert!(desktop.source.is_none());
        assert!(desktop.summary.is_none());
        assert!(desktop.notice.is_none());
    }

    #[tokio::test]
    async fn invalid_port_never_starts_connection_task() {
        let mut desktop = Desktop::new(tokio::runtime::Handle::current(), false);
        desktop.select(fixture());
        for port in ["0", "65536", "not-a-port"] {
            desktop.port = port.into();
            assert!(desktop.connect().is_err());
            assert!(desktop.stop.is_none());
        }
    }
}
