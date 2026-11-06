use super::{theme, widgets, Desktop, Source};
use eframe::egui::{self, RichText, Stroke};
use std::time::{Duration, Instant};
use wg_core::ConnectionState;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Tab {
    Connection,
    Profiles,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Phase {
    Empty,
    Ready,
    Connecting,
    Connected,
    Stopping,
    Error,
}

impl Phase {
    fn color(self) -> egui::Color32 {
        match self {
            Self::Connected => theme::SUCCESS,
            Self::Ready => theme::MUTED,
            Self::Connecting | Self::Stopping => theme::WARNING,
            Self::Error => theme::DANGER,
            Self::Empty => theme::MUTED,
        }
    }

    fn badge(self) -> &'static str {
        match self {
            Self::Empty => "Primeros pasos",
            Self::Ready => "Sin conexión",
            Self::Connecting => "Conexión en curso",
            Self::Connected => "Conectado al proxy",
            Self::Stopping => "Cerrando sesión",
            Self::Error => "Requiere atención",
        }
    }
}

impl Desktop {
    pub(super) fn busy(&self) -> bool {
        self.stop.is_some() || self.disconnecting
    }

    pub(super) fn phase(&self) -> Phase {
        if self.disconnecting {
            return Phase::Stopping;
        }
        if let Some(status) = &self.status {
            return match status.borrow().state {
                ConnectionState::Connected => Phase::Connected,
                ConnectionState::Connecting | ConnectionState::Reconnecting => Phase::Connecting,
                ConnectionState::Error | ConnectionState::Blocked => Phase::Error,
                ConnectionState::Disconnecting => Phase::Stopping,
                _ => {
                    if self.busy() {
                        Phase::Connecting
                    } else {
                        Phase::Ready
                    }
                }
            };
        }
        if self.stop.is_some() {
            Phase::Connecting
        } else if self.error.is_some() {
            Phase::Error
        } else if self.source.is_some() {
            Phase::Ready
        } else {
            Phase::Empty
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("WireGuard", &["conf"])
            .pick_file()
        {
            self.select(Source::File(path));
        }
    }

    pub(super) fn render(&mut self, context: &egui::Context) {
        // The Windows backend reports its system theme after AppCreator runs.
        // Keep the app on its configured dark style, including native light-mode hosts.
        if context.options(|options| options.theme_preference) != egui::ThemePreference::Dark {
            context.set_theme(egui::ThemePreference::Dark);
        }
        if !context.style().visuals.dark_mode
            || context.style().visuals.override_text_color != Some(theme::TEXT)
        {
            theme::apply(context);
        }
        self.process_events();
        if !self.busy() && !self.save_dialog {
            if let Some(path) = context.input(|input| {
                input
                    .raw
                    .dropped_files
                    .first()
                    .and_then(|file| file.path.clone())
            }) {
                self.select(Source::File(path));
            }
        }
        egui::TopBottomPanel::bottom("bottom_navigation")
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .stroke(Stroke::new(1.0_f32, theme::BORDER))
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(context, |ui| {
                self.tabs(ui);
                ui.add_space(2.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Proxy local · Solo tráfico configurado")
                            .size(11.0)
                            .color(theme::MUTED),
                    );
                });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::BACKGROUND).inner_margin(20))
            .show(context, |ui| {
                self.header(ui);
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Seqrite: revisión pendiente. No omitir la protección.")
                        .size(11.0)
                        .color(theme::WARNING),
                );
                ui.add_space(8.0);
                egui::ScrollArea::vertical()
                    .id_salt(("compact_tab", self.tab))
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.tab {
                        Tab::Connection => self.connection_tab(ui),
                        Tab::Profiles => self.profiles_tab(ui),
                        Tab::Settings => self.settings_tab(ui),
                    });
            });
        self.save_modal(context);
        if self.busy() || self.smoke_deadline.is_some() {
            context.request_repaint_after(Duration::from_millis(100));
        }
        if self
            .smoke_deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn header(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let branding_width = (ui.available_width() - 80.0).max(170.0);
            let branding = ui.allocate_ui_with_layout(
                egui::vec2(branding_width, 38.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    widgets::network_mark(ui, 23.0, theme::ACCENT);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(
                            RichText::new("WgDesktop")
                                .size(20.0)
                                .strong()
                                .color(theme::TEXT),
                        );
                        ui.label(
                            RichText::new("WireGuard · Proxy local")
                                .size(11.0)
                                .color(theme::MUTED),
                        );
                    });
                },
            );
            let drag = ui.interact(
                branding.response.rect,
                egui::Id::new("window_title_drag"),
                egui::Sense::drag(),
            );
            if drag.drag_started_by(egui::PointerButton::Primary) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            drag.on_hover_cursor(egui::CursorIcon::Grab);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                widgets::window_button(ui, widgets::WindowAction::Close);
                widgets::window_button(ui, widgets::WindowAction::Minimize);
            });
        });
    }

    fn tabs(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |columns| {
            for (column, (tab, label, icon)) in columns.iter_mut().zip([
                (Tab::Connection, "Conexión", widgets::NavIcon::Connection),
                (Tab::Profiles, "Perfiles", widgets::NavIcon::Profiles),
                (Tab::Settings, "Ajustes", widgets::NavIcon::Settings),
            ]) {
                if widgets::nav_button(column, label, icon, self.tab == tab, !self.save_dialog)
                    .clicked()
                {
                    self.tab = tab;
                }
            }
        });
    }

    fn connection_tab(&mut self, ui: &mut egui::Ui) {
        let phase = self.phase();
        let port_valid = self.port.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let label = match phase {
            Phase::Connecting => "Cancelar",
            Phase::Connected => "Desconectar",
            Phase::Stopping => "Desconectando…",
            _ if self.busy() => "Cerrar conexión",
            _ if self.source.is_none() => "Importar .conf",
            _ if !port_valid => "Corregir puerto",
            Phase::Error => "Reintentar",
            _ => "Conectar proxy",
        };
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(phase.badge()).size(13.0).color(phase.color()));
            if phase == Phase::Connected {
                ui.label(
                    RichText::new(self.session_duration())
                        .size(42.0)
                        .color(theme::TEXT),
                );
            } else {
                let title = match phase {
                    Phase::Empty => "Tu primera conexión",
                    Phase::Ready => "Desconectado",
                    Phase::Connecting => "Conectando…",
                    Phase::Stopping => "Desconectando…",
                    Phase::Error => "Revisa la conexión",
                    Phase::Connected => unreachable!(),
                };
                ui.label(RichText::new(title).size(32.0).color(theme::TEXT));
            }
            ui.add_space(4.0);
            if phase == Phase::Error {
                self.feedback(ui);
            } else {
                self.metrics(ui);
            }
            ui.add_space(4.0);
            let size = if phase == Phase::Error {
                112.0
            } else if ui.available_width() < 340.0 {
                138.0
            } else {
                148.0
            };
            if widgets::power_button(
                ui,
                size,
                label,
                phase.color(),
                phase == Phase::Connected,
                matches!(phase, Phase::Connecting | Phase::Stopping),
                !self.save_dialog && phase != Phase::Stopping,
            )
            .clicked()
            {
                if self.busy() {
                    self.disconnect();
                } else if self.source.is_none() {
                    self.open_file();
                } else if !port_valid {
                    self.tab = Tab::Settings;
                    self.focus_port = true;
                } else if let Err(error) = self.connect() {
                    self.error = Some(format!("{error:#}"));
                }
            }
        });
        ui.add_space(6.0);
        let title = self
            .source
            .as_ref()
            .map_or("Añadir un perfil".into(), Source::label);
        let subtitle = self
            .summary
            .as_ref()
            .map_or("Importa un archivo .conf".into(), |summary| {
                summary.endpoint.clone()
            });
        if let Some(bound) = self.bound.filter(|_| phase == Phase::Connected) {
            egui::Frame::new()
                .fill(theme::SURFACE)
                .corner_radius(12)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                    ui.horizontal(|ui| {
                        let info_width = (ui.available_width() - 160.0).max(90.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(info_width, 38.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_min_width(info_width);
                                ui.spacing_mut().item_spacing.y = 3.0;
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&title).size(14.0).color(theme::TEXT),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&subtitle);
                                ui.label(
                                    RichText::new(bound.to_string())
                                        .monospace()
                                        .size(12.0)
                                        .color(theme::MUTED),
                                );
                            },
                        );
                        let copied = self
                            .copied_address_until
                            .is_some_and(|until| Instant::now() < until);
                        if ui
                            .add(
                                egui::Button::new(if copied { "Copiado" } else { "Copiar" })
                                    .min_size(egui::vec2(74.0, 32.0)),
                            )
                            .clicked()
                        {
                            ui.ctx().copy_text(bound.to_string());
                            self.copied_address_until =
                                Some(Instant::now() + Duration::from_secs(2));
                        }
                        if ui
                            .button("Ayuda")
                            .on_hover_text("Cómo configurar SOCKS5 en tu navegador")
                            .clicked()
                        {
                            self.tab = Tab::Settings;
                            ui.ctx().data_mut(|data| {
                                data.insert_temp(egui::Id::new("open_proxy_help"), true)
                            });
                        }
                    });
                });
        } else {
            if widgets::profile_picker(ui, &title, &subtitle, !self.busy() && !self.save_dialog)
                .on_disabled_hover_text("Desconecta antes de cambiar el perfil.")
                .clicked()
            {
                self.tab = Tab::Profiles;
            }
            if phase == Phase::Empty {
                ui.label(
                    RichText::new("También puedes arrastrar un .conf a la ventana.")
                        .size(12.0)
                        .color(theme::MUTED),
                );
            }
        }
        if phase != Phase::Error {
            self.feedback(ui);
        }
    }

    fn session_duration(&self) -> String {
        self.session_started.map_or("—".into(), |started| {
            let seconds = started.elapsed().as_secs();
            format!(
                "{:02}:{:02}:{:02}",
                seconds / 3600,
                seconds / 60 % 60,
                seconds % 60
            )
        })
    }

    fn metrics(&self, ui: &mut egui::Ui) {
        let snapshot = self.status.as_ref().map(|status| status.borrow().clone());
        ui.columns(2, |columns| {
            widgets::metric(
                &mut columns[0],
                "Recibido",
                &snapshot
                    .as_ref()
                    .map_or("—".into(), |s| widgets::bytes(s.rx_bytes)),
            );
            widgets::metric(
                &mut columns[1],
                "Enviado",
                &snapshot
                    .as_ref()
                    .map_or("—".into(), |s| widgets::bytes(s.tx_bytes)),
            );
        });
    }

    fn profiles_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Mis perfiles")
                .size(20.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            RichText::new("Elige una conexión o añade una nueva.")
                .size(13.0)
                .color(theme::MUTED),
        );
        let editable = !self.busy() && !self.save_dialog;
        if widgets::primary_button(
            ui,
            "+  Importar archivo .conf",
            ui.available_width(),
            editable,
        )
        .clicked()
        {
            self.open_file();
        }
        let search_label = ui.label(
            RichText::new("Buscar en tus perfiles guardados")
                .size(12.0)
                .color(theme::MUTED),
        );
        ui.horizontal(|ui| {
            let search_width = (ui.available_width() - 90.0).max(100.0);
            ui.add_enabled(
                !self.save_dialog,
                egui::TextEdit::singleline(&mut self.profile_filter)
                    .hint_text("Buscar perfil…")
                    .desired_width(search_width)
                    .text_color(theme::TEXT)
                    .background_color(theme::SIDEBAR)
                    .margin(egui::Margin::symmetric(9, 7)),
            )
            .labelled_by(search_label.id);
            if ui
                .add_enabled(
                    !self.profile_filter.is_empty() && !self.save_dialog,
                    egui::Button::new("Limpiar"),
                )
                .clicked()
            {
                self.profile_filter.clear();
            }
        });
        let filter = self.profile_filter.trim().to_lowercase();
        let profiles: Vec<_> = self
            .profiles
            .iter()
            .filter(|name| name.to_lowercase().contains(&filter))
            .cloned()
            .collect();
        if matches!(self.source, Some(Source::File(_))) {
            widgets::profile_row(
                ui,
                &self.source.as_ref().unwrap().label(),
                "Archivo importado, aún sin guardar",
                true,
                editable,
            );
            if ui
                .add_enabled(
                    editable,
                    egui::Button::new(RichText::new("Guardar perfil cifrado").color(theme::ACCENT))
                        .min_size(egui::vec2(ui.available_width(), 38.0)),
                )
                .clicked()
            {
                self.error = None;
                self.notice = None;
                self.save_dialog = true;
                self.focus_profile_name = true;
            }
        }
        widgets::eyebrow(
            ui,
            &if filter.is_empty() {
                format!("Guardados en este equipo · {}", self.profiles.len())
            } else {
                format!(
                    "Resultados · {} de {} perfiles",
                    profiles.len(),
                    self.profiles.len()
                )
            },
        );
        if profiles.is_empty() && filter.is_empty() {
            widgets::empty_profiles(ui);
        } else if profiles.is_empty() && !filter.is_empty() {
            ui.label(
                RichText::new(
                    "No encontramos perfiles con ese nombre. Prueba otra búsqueda o pulsa Limpiar.",
                )
                .size(13.0)
                .color(theme::MUTED),
            );
        }
        for name in profiles {
            let selected =
                matches!(&self.source, Some(Source::Stored(current)) if *current == name);
            if widgets::profile_row(
                ui,
                &name,
                if selected {
                    "Seleccionado · Guardado cifrado"
                } else {
                    "Guardado cifrado en este equipo"
                },
                selected,
                editable,
            )
            .on_disabled_hover_text("Desconecta antes de cambiar de perfil.")
            .clicked()
            {
                self.select(Source::Stored(name));
            }
        }
        if self.busy() {
            ui.label(
                RichText::new("Desconecta para importar o cambiar el perfil.")
                    .size(12.0)
                    .color(theme::WARNING),
            );
        }
        ui.label(RichText::new("Puedes arrastrar un .conf a la ventana. Para conservarlo cifrado en este equipo, elige Guardar perfil cifrado.").size(12.0).color(theme::MUTED));
        self.feedback(ui);
    }

    fn settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new("Ajustes")
                .size(20.0)
                .strong()
                .color(theme::TEXT),
        );
        ui.label(
            RichText::new("Configura el proxy para tus aplicaciones.")
                .size(13.0)
                .color(theme::MUTED),
        );
        theme::card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            widgets::eyebrow(ui, "Dirección local");
            ui.label(
                RichText::new("Escucha solo en 127.0.0.1")
                    .size(14.0)
                    .color(theme::TEXT),
            );
            ui.add_enabled_ui(!self.busy() && !self.save_dialog, |ui| {
                ui.horizontal(|ui| {
                    let label = ui.label(RichText::new("Puerto").color(theme::TEXT));
                    let input = ui
                        .add(
                            egui::TextEdit::singleline(&mut self.port)
                                .id(egui::Id::new("proxy_port"))
                                .desired_width(80.0)
                                .char_limit(5)
                                .margin(egui::Margin::symmetric(10, 8))
                                .text_color(theme::TEXT)
                                .background_color(theme::SIDEBAR)
                                .hint_text("1080"),
                        )
                        .labelled_by(label.id);
                    if self.focus_port {
                        input.request_focus();
                        self.focus_port = false;
                    }
                });
            });
            if !self.port.trim().parse::<u16>().is_ok_and(|port| port > 0) {
                ui.label(
                    RichText::new("El puerto debe estar entre 1 y 65535.")
                        .size(12.0)
                        .color(theme::DANGER),
                );
            }
            ui.label(
                RichText::new("Se aplica en la próxima conexión.")
                    .size(12.0)
                    .color(theme::MUTED),
            );
        });
        theme::card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width((ui.available_width() - 1.0).max(0.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Proxy SOCKS5").strong().color(theme::TEXT));
                widgets::pill(ui, "Disponible", theme::SUCCESS);
            });
            ui.label(
                RichText::new("Solo las aplicaciones que configures.")
                    .size(12.0)
                    .color(theme::MUTED),
            );
            ui.horizontal(|ui| {
                ui.label(RichText::new("VPN completa").color(theme::MUTED));
                widgets::pill(ui, "Próximamente", theme::MUTED);
            });
            ui.label(
                RichText::new("VPN del sistema y kill switch aún no implementados.")
                    .size(11.0)
                    .color(theme::MUTED),
            );
        });
        if let Some(snapshot) = self.status.as_ref().map(|status| status.borrow().clone()) {
            ui.label(
                RichText::new(format!(
                    "Conexiones TCP: {} · Handshake: {}",
                    snapshot.active_connections,
                    snapshot
                        .seconds_since_handshake
                        .map_or("—".into(), |age| format!("{age} s"))
                ))
                .size(12.0)
                .color(theme::MUTED),
            );
        }
        egui::CollapsingHeader::new("Datos del perfil")
            .id_salt("compact_details")
            .show(ui, |ui| {
                if let Some(summary) = &self.summary {
                    for (label, value) in [
                        ("Peer", summary.endpoint.clone()),
                        ("Direcciones", summary.addresses.join(", ")),
                        (
                            "DNS",
                            summary
                                .dns
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", "),
                        ),
                        ("MTU", summary.mtu.to_string()),
                        ("Redes permitidas", summary.allowed_ips.join(", ")),
                    ] {
                        widgets::eyebrow(ui, label);
                        ui.add(
                            egui::Label::new(
                                RichText::new(if value.is_empty() {
                                    "No configurado".into()
                                } else {
                                    value
                                })
                                .size(12.0)
                                .color(theme::TEXT),
                            )
                            .wrap(),
                        );
                    }
                    if summary.endpoint_needs_bootstrap_dns {
                        ui.label(
                            RichText::new(
                                "El nombre del peer usa DNS de Windows para el arranque.",
                            )
                            .size(11.0)
                            .color(theme::WARNING),
                        );
                    }
                } else {
                    ui.label("Selecciona un perfil para ver sus datos.");
                }
            });
        let open_help = ui
            .ctx()
            .data_mut(|data| data.remove_temp::<bool>(egui::Id::new("open_proxy_help")))
            .unwrap_or(false);
        let help = egui::CollapsingHeader::new("Cómo configurar tu navegador").id_salt("compact_help").open(open_help.then_some(true)).show(ui, |ui| {
            ui.label("1. Selecciona un proxy de tipo SOCKS5 en tu navegador.");
            ui.label(format!("2. Usa 127.0.0.1 y el puerto {}.", self.port));
            ui.label("3. Activa la resolución DNS a través del proxy.");
            ui.label(RichText::new("Las conexiones que eviten el proxy, como algunas de WebRTC, no quedan protegidas. Un handshake no prueba salida a Internet.").size(12.0).color(theme::MUTED));
            let command = format!("curl.exe --socks5-hostname 127.0.0.1:{} https://example.com", self.port);
            ui.add(egui::Label::new(RichText::new(&command).monospace().size(11.0).color(theme::TEXT)).wrap());
            if ui.button("Copiar comando").clicked() {
                ui.ctx().copy_text(command);
                self.notice = Some("Comando de comprobación copiado.".into());
            }
        });
        if open_help {
            help.header_response.scroll_to_me(Some(egui::Align::TOP));
        }
        egui::CollapsingHeader::new("Revisión de Seqrite").id_salt("compact_security").show(ui, |ui| {
            ui.label(RichText::new("La alerta «Potential Keylogger» continúa en investigación. No se ha confirmado un falso positivo. No desactives la protección ni añadas excepciones.").size(12.0).color(theme::WARNING));
        });
        self.feedback(ui);
    }

    fn feedback(&self, ui: &mut egui::Ui) {
        if let Some(error) = &self.error {
            egui::Frame::new()
                .fill(theme::DANGER_DARK)
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_min_width((ui.available_width() - 1.0).max(0.0));
                    ui.label(
                        RichText::new("No se pudo completar la operación")
                            .size(13.0)
                            .strong()
                            .color(theme::DANGER),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(error).size(12.0).color(theme::DANGER))
                            .wrap(),
                    );
                });
        } else if let Some(notice) = &self.notice {
            egui::Frame::new()
                .fill(theme::SIDEBAR)
                .corner_radius(8)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.label(RichText::new(notice).size(12.0).color(theme::MUTED));
                });
        }
    }

    fn save_modal(&mut self, context: &egui::Context) {
        if !self.save_dialog {
            return;
        }
        let response = egui::Modal::new(egui::Id::new("save_profile_modal"))
            .frame(theme::card().inner_margin(16))
            .show(context, |ui| {
                ui.set_width((context.content_rect().width() - 64.0).clamp(240.0, 328.0));
                ui.label(
                    RichText::new("Guardar perfil")
                        .size(20.0)
                        .strong()
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new("Las claves se cifran con DPAPI para tu usuario de Windows.")
                        .size(12.0)
                        .color(theme::MUTED),
                );
                let label = ui.label("Nombre del perfil");
                let input = ui
                    .add(
                        egui::TextEdit::singleline(&mut self.profile_name)
                            .desired_width(f32::INFINITY)
                            .char_limit(64)
                            .text_color(theme::TEXT)
                            .background_color(theme::SIDEBAR)
                            .margin(egui::Margin::symmetric(10, 8))
                            .hint_text("Oficina"),
                    )
                    .labelled_by(label.id);
                if self.focus_profile_name {
                    input.request_focus();
                    self.focus_profile_name = false;
                }
                let name_error = self.profile_name_error();
                if let Some(error) = name_error {
                    ui.label(RichText::new(error).size(12.0).color(theme::DANGER));
                }
                ui.label(
                    RichText::new(
                        "Letras, números, guiones o guiones bajos. No se sobrescriben perfiles.",
                    )
                    .size(11.0)
                    .color(theme::MUTED),
                );
                if let Some(error) = &self.error {
                    ui.colored_label(theme::DANGER, error);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancelar").clicked() {
                        self.save_dialog = false;
                    }
                    if widgets::primary_button(
                        ui,
                        "Guardar cifrado",
                        0.0,
                        !self.profile_name.is_empty() && name_error.is_none(),
                    )
                    .clicked()
                    {
                        if let Err(error) = self.save() {
                            self.error = Some(format!("No se pudo guardar: {error:#}"));
                        }
                    }
                });
            });
        if response.should_close() {
            self.save_dialog = false;
        }
    }

    fn profile_name_error(&self) -> Option<&'static str> {
        if self.profile_name.is_empty() {
            return None;
        }
        if crate::storage::validate_name(&self.profile_name).is_err() {
            return Some("Usa 1–64 letras sin acentos, números, guiones o _. Evita nombres reservados de Windows.");
        }
        self.profiles
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&self.profile_name))
            .then_some("Ya existe un perfil con ese nombre. Elige otro para conservar ambos.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn save_form_rejects_reserved_invalid_and_case_insensitive_duplicate_names() {
        let mut desktop = Desktop::new(tokio::runtime::Handle::current(), false);
        desktop.profiles = vec!["Oficina".into()];
        for invalid in [
            "CON",
            "lpt1",
            "a/b",
            "nombre con espacios",
            "oficina",
            "OFICINA",
        ] {
            desktop.profile_name = invalid.into();
            assert!(desktop.profile_name_error().is_some(), "{invalid}");
        }
        desktop.profile_name = "Casa-02".into();
        assert!(desktop.profile_name_error().is_none());
        assert!(desktop.stop.is_none());
    }
}
