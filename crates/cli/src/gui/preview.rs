//! CPU-only layout previews. No native window, winit event loop or tunnel is started.
//! All displayed profiles and counters in these snapshots are explicitly test fixtures.

use super::{
    theme,
    view::{Phase, Tab},
    Desktop, Source,
};
use eframe::egui::{
    self,
    epaint::{ImageData, Primitive, Vertex},
    ColorImage, Pos2, TextureId,
};
use image::{Rgba, RgbaImage};
use std::{collections::HashMap, path::PathBuf};
use tokio::sync::{oneshot, watch};
use wg_core::ConnectionState;
use wg_tunnel::Snapshot;

fn scene(phase: Phase) -> Desktop {
    let mut desktop = Desktop::new(tokio::runtime::Handle::current(), false);
    desktop.profiles.clear();
    desktop.error = None;
    if phase != Phase::Empty {
        desktop.select(Source::File(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/example.conf"),
        ));
        desktop.source = Some(Source::Stored("Oficina".into()));
        desktop.profiles = vec!["Oficina".into(), "Laboratorio".into()];
    }
    if matches!(
        phase,
        Phase::Connected | Phase::Connecting | Phase::Stopping
    ) {
        let (stop, _) = oneshot::channel();
        desktop.stop = Some(stop);
    }
    if phase == Phase::Connected {
        let (_, status) = watch::channel(Snapshot {
            state: ConnectionState::Connected,
            seconds_since_handshake: Some(12),
            tx_bytes: 1_503_232,
            rx_bytes: 6_721_536,
            active_connections: 4,
        });
        desktop.status = Some(status);
        desktop.bound = Some("127.0.0.1:1080".parse().unwrap());
        desktop.session_started =
            std::time::Instant::now().checked_sub(std::time::Duration::from_secs(2438));
    }
    desktop.disconnecting = phase == Phase::Stopping;
    if phase == Phase::Error {
        desktop.error =
            Some("El peer no respondió. Revisa el endpoint y el acceso UDP al servidor.".into());
    }
    desktop
}

#[tokio::test]
async fn all_connection_states_render_at_both_window_sizes_without_native_windows() {
    for size in [[420.0, 600.0], [360.0, 560.0]] {
        for phase in [
            Phase::Empty,
            Phase::Ready,
            Phase::Connecting,
            Phase::Connected,
            Phase::Stopping,
            Phase::Error,
        ] {
            let mut desktop = scene(phase);
            assert_eq!(desktop.phase(), phase);
            let context = egui::Context::default();
            theme::apply(&context);
            for tab in [Tab::Connection, Tab::Profiles, Tab::Settings] {
                desktop.tab = tab;
                let output = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(size[0], size[1]),
                        )),
                        system_theme: Some(egui::Theme::Light),
                        ..Default::default()
                    },
                    |context| desktop.render(context),
                );
                assert!(!output.shapes.is_empty());
                assert!(context.style().visuals.dark_mode);
            }
            assert_eq!(desktop.phase(), phase);
        }
    }
}

#[tokio::test]
async fn dialog_and_long_profile_names_render_without_connecting() {
    let mut desktop = scene(Phase::Ready);
    desktop.source = Some(Source::Stored(
        "Un_nombre_de_perfil_extremadamente_largo_para_comprobar_el_layout".into(),
    ));
    desktop.profiles = vec![desktop.source.as_ref().unwrap().label()];
    desktop.save_dialog = true;
    let context = egui::Context::default();
    theme::apply(&context);
    let output = context.run(egui::RawInput::default(), |context| desktop.render(context));
    assert!(!output.shapes.is_empty());
    assert!(desktop.stop.is_none());
}

#[tokio::test]
#[ignore = "Opt-in CPU rendering to WG_DESKTOP_UI_PREVIEW_DIR; no desktop application is launched"]
async fn export_layout_previews() {
    let directory = PathBuf::from(
        std::env::var_os("WG_DESKTOP_UI_PREVIEW_DIR").expect("set WG_DESKTOP_UI_PREVIEW_DIR"),
    );
    std::fs::create_dir_all(&directory).unwrap();
    for (name, phase, width, height) in [
        ("empty", Phase::Empty, 420, 600),
        ("ready", Phase::Ready, 420, 600),
        ("connected", Phase::Connected, 420, 600),
        ("connecting", Phase::Connecting, 420, 600),
        ("error", Phase::Error, 420, 600),
        ("compact", Phase::Ready, 360, 560),
        ("save-profile", Phase::Ready, 420, 600),
        ("profiles", Phase::Ready, 420, 600),
        ("settings", Phase::Ready, 420, 600),
        ("invalid-port", Phase::Ready, 420, 600),
        ("search-empty", Phase::Ready, 420, 600),
        ("save-invalid", Phase::Ready, 420, 600),
        ("connected-compact", Phase::Connected, 360, 560),
        ("long-profile", Phase::Ready, 360, 560),
    ] {
        let mut desktop = scene(phase);
        if matches!(name, "save-profile" | "save-invalid") {
            desktop.source = Some(Source::File(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/example.conf"),
            ));
            desktop.profile_name = if name == "save-invalid" {
                "Oficina"
            } else {
                "Casa"
            }
            .into();
            desktop.save_dialog = true;
        }
        if name == "profiles" {
            desktop.tab = Tab::Profiles;
        }
        if name == "settings" {
            desktop.tab = Tab::Settings;
        }
        if name == "invalid-port" {
            desktop.tab = Tab::Settings;
            desktop.port = "0".into();
        }
        if name == "search-empty" {
            desktop.tab = Tab::Profiles;
            desktop.profile_filter = "Sin coincidencias".into();
        }
        if name == "long-profile" {
            desktop.source = Some(Source::Stored(
                "Un_nombre_de_perfil_muy_largo_para_comprobar_la_ventana".into(),
            ));
        }
        let image = rasterize(&mut desktop, width, height);
        image.save(directory.join(format!("{name}.png"))).unwrap();
    }
}

fn ui_frame(
    desktop: &mut Desktop,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, super::WINDOW_SIZE)),
            events,
            ..Default::default()
        },
        |context| desktop.render(context),
    )
}

fn text_rect(output: &egui::FullOutput, label: &str) -> egui::Rect {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    shape.clip_rect.contains_rect(rect),
                    "{label} must remain fully visible: {rect:?} vs {:?}",
                    shape.clip_rect
                );
                Some(rect)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing visible text: {label}"))
}

fn click_text(desktop: &mut Desktop, context: &egui::Context, label: &str) -> egui::FullOutput {
    // Settle the immediate-mode layout before locating the control.
    let _ = ui_frame(desktop, context, vec![]);
    let output = ui_frame(desktop, context, vec![]);
    let point = text_rect(&output, label).center();
    let _ = ui_frame(
        desktop,
        context,
        vec![
            egui::Event::PointerMoved(point),
            egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    ui_frame(
        desktop,
        context,
        vec![egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    )
}

#[tokio::test]
async fn primary_action_and_transfer_metrics_are_visible_without_scrolling() {
    for phase in [Phase::Ready, Phase::Connected] {
        let mut desktop = scene(phase);
        let context = egui::Context::default();
        theme::apply(&context);
        let _ = ui_frame(&mut desktop, &context, vec![]);
        let output = ui_frame(&mut desktop, &context, vec![]);
        for label in [
            if phase == Phase::Ready {
                "Conectar proxy"
            } else {
                "Desconectar"
            },
            "Enviado",
            "Recibido",
            "—",
        ] {
            if phase == Phase::Connected && label == "—" {
                continue;
            }
            text_rect(&output, label);
        }
        if phase == Phase::Connected {
            text_rect(&output, "1,4 MiB");
            text_rect(&output, "6,4 MiB");
        }
    }
}

#[tokio::test]
async fn correcting_invalid_port_opens_settings_and_focuses_the_field() {
    let mut desktop = scene(Phase::Ready);
    desktop.port = "0".into();
    let context = egui::Context::default();
    theme::apply(&context);
    click_text(&mut desktop, &context, "Corregir puerto");
    assert_eq!(desktop.tab, Tab::Settings);
    let _ = ui_frame(&mut desktop, &context, vec![]);
    assert_eq!(
        context.memory(|memory| memory.focused()),
        Some(egui::Id::new("proxy_port"))
    );
    assert!(desktop.stop.is_none());
}

#[tokio::test]
async fn connection_error_is_visible_before_the_retry_action() {
    let mut desktop = scene(Phase::Error);
    let context = egui::Context::default();
    theme::apply(&context);
    let _ = ui_frame(&mut desktop, &context, vec![]);
    let output = ui_frame(&mut desktop, &context, vec![]);
    let error = text_rect(&output, desktop.error.as_ref().unwrap());
    let retry = text_rect(&output, "Reintentar");
    assert!(error.bottom() < retry.top());
}

#[tokio::test]
async fn copying_endpoint_confirms_and_copies_the_actual_bound_address() {
    let mut desktop = scene(Phase::Connected);
    let context = egui::Context::default();
    theme::apply(&context);
    let output = click_text(&mut desktop, &context, "Copiar");
    assert!(output.platform_output.commands.iter().any(|command| matches!(command, egui::OutputCommand::CopyText(value) if value == "127.0.0.1:1080")));
    let output = ui_frame(&mut desktop, &context, vec![]);
    text_rect(&output, "Copiado");
}

#[tokio::test]
async fn clearing_search_restores_saved_profiles() {
    let mut desktop = scene(Phase::Ready);
    desktop.tab = Tab::Profiles;
    desktop.profile_filter = "inexistente".into();
    let context = egui::Context::default();
    theme::apply(&context);
    click_text(&mut desktop, &context, "Limpiar");
    assert!(desktop.profile_filter.is_empty());
    let output = ui_frame(&mut desktop, &context, vec![]);
    text_rect(&output, "Oficina");
    text_rect(&output, "Laboratorio");
    assert!(desktop.stop.is_none());
}

#[tokio::test]
async fn bottom_navigation_preserves_an_active_connection() {
    let mut desktop = scene(Phase::Connected);
    let context = egui::Context::default();
    theme::apply(&context);
    for (label, tab) in [
        ("Perfiles", Tab::Profiles),
        ("Ajustes", Tab::Settings),
        ("Conexión", Tab::Connection),
    ] {
        click_text(&mut desktop, &context, label);
        assert_eq!(desktop.tab, tab);
        assert_eq!(desktop.phase(), Phase::Connected);
        assert_eq!(desktop.bound.unwrap().port(), 1080);
    }
}

#[tokio::test]
async fn session_clock_starts_only_after_ready_and_clears_when_finished() {
    let mut desktop = scene(Phase::Ready);
    assert!(desktop.session_started.is_none());
    let (_, status) = watch::channel(Snapshot {
        state: ConnectionState::Connected,
        ..Default::default()
    });
    desktop
        .sender
        .send(super::Event::Ready(
            "127.0.0.1:1080".parse().unwrap(),
            status,
        ))
        .unwrap();
    desktop.process_events();
    assert!(desktop.session_started.is_some());
    assert_eq!(desktop.phase(), Phase::Connected);
    desktop.sender.send(super::Event::Finished(Ok(()))).unwrap();
    desktop.process_events();
    assert!(desktop.session_started.is_none());
    assert_eq!(desktop.phase(), Phase::Ready);
}

#[tokio::test]
async fn status_channel_does_not_imply_a_completed_handshake() {
    let mut desktop = scene(Phase::Connecting);
    let (_, status) = watch::channel(Snapshot::default());
    desktop.status = Some(status);
    assert_eq!(desktop.phase(), Phase::Connecting);
}

#[tokio::test]
async fn duplicate_connect_cannot_start_a_second_task() {
    let mut desktop = scene(Phase::Connecting);
    assert!(desktop.connect().is_err());
    assert_eq!(desktop.phase(), Phase::Connecting);
}

#[tokio::test]
async fn light_windows_theme_cannot_reset_text_or_input_contrast() {
    let mut desktop = scene(Phase::Ready);
    desktop.tab = Tab::Profiles;
    let context = egui::Context::default();
    theme::apply(&context);
    // Reproduce a backend/system preference reported after app initialization.
    context.set_theme(egui::ThemePreference::System);
    let _ = context.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, super::WINDOW_SIZE)),
            system_theme: Some(egui::Theme::Light),
            ..Default::default()
        },
        |context| desktop.render(context),
    );
    let style = context.style();
    assert_eq!(context.theme(), egui::Theme::Dark);
    assert!(style.visuals.dark_mode);
    assert_eq!(style.visuals.override_text_color, Some(theme::TEXT));
    assert_eq!(style.visuals.extreme_bg_color, theme::SIDEBAR);
    assert_eq!(style.visuals.widgets.inactive.fg_stroke.color, theme::TEXT);
}

#[test]
fn native_window_is_fixed_and_portrait_only() {
    let options = super::window_options();
    assert_eq!(options.viewport.inner_size, Some(super::WINDOW_SIZE));
    assert_eq!(options.viewport.min_inner_size, options.viewport.inner_size);
    assert_eq!(options.viewport.max_inner_size, options.viewport.inner_size);
    assert_eq!(options.viewport.resizable, Some(false));
    assert_eq!(options.viewport.maximize_button, Some(false));
    assert_eq!(options.viewport.decorations, Some(false));
    let actual_size = options.viewport.inner_size.unwrap();
    assert!(actual_size.x < actual_size.y);
}

#[tokio::test]
async fn selecting_a_profile_returns_to_connection_without_connecting() {
    let mut desktop = scene(Phase::Empty);
    desktop.tab = Tab::Profiles;
    desktop.select(Source::File(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/example.conf"),
    ));
    assert_eq!(desktop.tab, Tab::Connection);
    assert_eq!(desktop.phase(), Phase::Ready);
    assert!(desktop.stop.is_none());
}

#[tokio::test]
async fn dragging_the_dark_header_requests_native_window_drag_not_maximize() {
    let mut desktop = scene(Phase::Ready);
    let context = egui::Context::default();
    theme::apply(&context);
    let mut observed_drag = false;
    for events in [
        vec![],
        vec![
            egui::Event::PointerMoved(egui::pos2(80.0, 30.0)),
            egui::Event::PointerButton {
                pos: egui::pos2(80.0, 30.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
        ],
        vec![egui::Event::PointerMoved(egui::pos2(110.0, 35.0))],
    ] {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, super::WINDOW_SIZE)),
                events,
                ..Default::default()
            },
            |context| desktop.render(context),
        );
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        observed_drag |= commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::StartDrag));
        assert!(!commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::Maximized(_))));
    }
    assert!(observed_drag);
    assert!(desktop.stop.is_none());
}

fn rasterize(desktop: &mut Desktop, width: u32, height: u32) -> RgbaImage {
    let context = egui::Context::default();
    theme::apply(&context);
    let mut textures = HashMap::<TextureId, ColorImage>::new();
    let mut last_output = None;
    for frame in 0..3 {
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(width as f32, height as f32),
                )),
                time: Some(frame as f64 * 0.25),
                system_theme: Some(egui::Theme::Light),
                ..Default::default()
            },
            |context| desktop.render(context),
        );
        for (id, delta) in &output.textures_delta.set {
            let ImageData::Color(image) = &delta.image;
            if let Some([x, y]) = delta.pos {
                let texture = textures
                    .get_mut(id)
                    .expect("texture must exist before update");
                for row in 0..image.size[1] {
                    let start = (y + row) * texture.size[0] + x;
                    texture.pixels[start..start + image.size[0]].copy_from_slice(
                        &image.pixels[row * image.size[0]..(row + 1) * image.size[0]],
                    );
                }
            } else {
                textures.insert(*id, image.as_ref().clone());
            }
        }
        last_output = Some(output);
    }
    let output = last_output.unwrap();
    let mut image = RgbaImage::from_pixel(width, height, Rgba(theme::BACKGROUND.to_array()));
    for primitive in context.tessellate(output.shapes, output.pixels_per_point) {
        if let Primitive::Mesh(mesh) = primitive.primitive {
            let texture = textures.get(&mesh.texture_id).expect("mesh texture");
            let (triangles, remainder) = mesh.indices.as_chunks::<3>();
            assert!(remainder.is_empty());
            for triangle in triangles {
                let vertices = [
                    mesh.vertices[triangle[0] as usize],
                    mesh.vertices[triangle[1] as usize],
                    mesh.vertices[triangle[2] as usize],
                ];
                draw_triangle(&mut image, vertices, primitive.clip_rect, texture);
            }
        }
    }
    image
}

fn edge(a: Pos2, b: Pos2, point: Pos2) -> f32 {
    (point.x - a.x) * (b.y - a.y) - (point.y - a.y) * (b.x - a.x)
}

fn draw_triangle(
    image: &mut RgbaImage,
    vertices: [Vertex; 3],
    clip: egui::Rect,
    texture: &ColorImage,
) {
    let [a, b, c] = vertices;
    let area = edge(b.pos, c.pos, a.pos);
    if area.abs() < 0.0001 {
        return;
    }
    let left = a
        .pos
        .x
        .min(b.pos.x)
        .min(c.pos.x)
        .max(clip.min.x)
        .floor()
        .max(0.0) as u32;
    let top = a
        .pos
        .y
        .min(b.pos.y)
        .min(c.pos.y)
        .max(clip.min.y)
        .floor()
        .max(0.0) as u32;
    let right = a
        .pos
        .x
        .max(b.pos.x)
        .max(c.pos.x)
        .min(clip.max.x)
        .ceil()
        .min(image.width() as f32)
        .max(0.0) as u32;
    let bottom = a
        .pos
        .y
        .max(b.pos.y)
        .max(c.pos.y)
        .min(clip.max.y)
        .ceil()
        .min(image.height() as f32)
        .max(0.0) as u32;
    for y in top..bottom {
        for x in left..right {
            let point = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            if !clip.contains(point) {
                continue;
            }
            let weights = [
                edge(b.pos, c.pos, point) / area,
                edge(c.pos, a.pos, point) / area,
                edge(a.pos, b.pos, point) / area,
            ];
            if weights.iter().any(|weight| *weight < -0.00001) {
                continue;
            }
            let uv = egui::pos2(
                a.uv.x * weights[0] + b.uv.x * weights[1] + c.uv.x * weights[2],
                a.uv.y * weights[0] + b.uv.y * weights[1] + c.uv.y * weights[2],
            );
            let sample = sample_texture(texture, uv);
            let colors = [a.color.to_array(), b.color.to_array(), c.color.to_array()];
            let mut source = [0.0; 4];
            for (channel, value) in source.iter_mut().enumerate() {
                *value = (colors[0][channel] as f32 * weights[0]
                    + colors[1][channel] as f32 * weights[1]
                    + colors[2][channel] as f32 * weights[2])
                    * sample[channel]
                    / 255.0;
            }
            let destination = image.get_pixel_mut(x, y);
            for (channel, value) in destination.0.iter_mut().enumerate() {
                *value = (source[channel] + *value as f32 * (1.0 - source[3] / 255.0))
                    .clamp(0.0, 255.0) as u8;
            }
        }
    }
}

fn sample_texture(texture: &ColorImage, uv: Pos2) -> [f32; 4] {
    let x = (uv.x * texture.size[0] as f32 - 0.5).clamp(0.0, (texture.size[0] - 1) as f32);
    let y = (uv.y * texture.size[1] as f32 - 0.5).clamp(0.0, (texture.size[1] - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = (
        (x0 + 1).min(texture.size[0] - 1),
        (y0 + 1).min(texture.size[1] - 1),
    );
    let pixels = [
        texture.pixels[y0 * texture.size[0] + x0].to_array(),
        texture.pixels[y0 * texture.size[0] + x1].to_array(),
        texture.pixels[y1 * texture.size[0] + x0].to_array(),
        texture.pixels[y1 * texture.size[0] + x1].to_array(),
    ];
    let (tx, ty) = (x - x.floor(), y - y.floor());
    let mut result = [0.0; 4];
    for (channel, value) in result.iter_mut().enumerate() {
        *value = (pixels[0][channel] as f32 * (1.0 - tx) + pixels[1][channel] as f32 * tx)
            * (1.0 - ty)
            + (pixels[2][channel] as f32 * (1.0 - tx) + pixels[3][channel] as f32 * tx) * ty;
    }
    result
}
