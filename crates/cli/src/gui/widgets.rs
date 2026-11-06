use eframe::egui::{self, Align2, Color32, FontId, RichText, Sense, Stroke, Vec2};

use super::theme;

pub fn focus_outline(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(3.0),
            12,
            Stroke::new(2.0_f32, theme::TEXT),
            egui::StrokeKind::Inside,
        );
    }
}

pub fn primary_button(ui: &mut egui::Ui, label: &str, width: f32, enabled: bool) -> egui::Response {
    let response = ui
        .scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.weak_bg_fill = theme::ACCENT;
            widgets.inactive.bg_stroke = Stroke::NONE;
            widgets.hovered.weak_bg_fill = theme::ACCENT_HOVER;
            widgets.hovered.bg_stroke = Stroke::NONE;
            widgets.active.weak_bg_fill = theme::ACCENT_PRESSED;
            widgets.active.bg_stroke = Stroke::NONE;
            ui.add_enabled(
                enabled,
                egui::Button::new(RichText::new(label).strong().color(theme::BACKGROUND))
                    .min_size(egui::vec2(width, 44.0))
                    .corner_radius(10),
            )
        })
        .inner;
    focus_outline(ui, &response);
    response
}

#[derive(Clone, Copy)]
pub(super) enum WindowAction {
    Minimize,
    Close,
}

/// Only requests standard operations on this application's own viewport.
/// No global keyboard or mouse hooks are used.
pub(super) fn window_button(ui: &mut egui::Ui, action: WindowAction) -> egui::Response {
    let (glyph, label, command) = match action {
        WindowAction::Minimize => ("−", "Minimizar", egui::ViewportCommand::Minimized(true)),
        WindowAction::Close => ("×", "Cerrar", egui::ViewportCommand::Close),
    };
    let response = ui
        .scope(|ui| {
            ui.visuals_mut().widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
            if matches!(action, WindowAction::Close) {
                let hovered = &mut ui.visuals_mut().widgets.hovered;
                hovered.bg_fill = theme::CLOSE_HOVER;
                hovered.weak_bg_fill = hovered.bg_fill;
                hovered.fg_stroke.color = egui::Color32::WHITE;
            }
            ui.add(
                egui::Button::new(RichText::new(glyph).size(20.0).color(theme::TEXT))
                    .min_size(egui::vec2(28.0, 28.0))
                    .stroke(Stroke::NONE)
                    .corner_radius(5),
            )
        })
        .inner;
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    focus_outline(ui, &response);
    if response.clicked() {
        ui.ctx().send_viewport_cmd(command);
    }
    response.on_hover_text(label)
}

pub fn eyebrow(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(12.0).color(theme::MUTED));
}

pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_owned(), FontId::proportional(12.0), color);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(galley.size().x + 30.0, 24.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, 7, color.gamma_multiply(0.10));
    ui.painter().rect_stroke(
        rect,
        7,
        Stroke::new(1.0_f32, color.gamma_multiply(0.30)),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .circle_filled(egui::pos2(rect.left() + 10.0, rect.center().y), 2.5, color);
    ui.painter().galley(
        rect.center() - galley.size() * 0.5 + egui::vec2(5.0, 0.0),
        galley,
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text));
}

pub fn network_mark(ui: &mut egui::Ui, size: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let painter = ui.painter_at(rect);
    let center = rect.center();
    let nodes = [
        center + egui::vec2(-size * 0.24, size * 0.17),
        center + egui::vec2(0.0, -size * 0.23),
        center + egui::vec2(size * 0.24, size * 0.17),
    ];
    for pair in [(0, 1), (1, 2), (0, 2)] {
        painter.line_segment([nodes[pair.0], nodes[pair.1]], Stroke::new(1.8_f32, color));
    }
    for node in nodes {
        painter.circle(
            node,
            size * 0.085,
            theme::SIDEBAR,
            Stroke::new(1.8_f32, color),
        );
    }
}

pub fn profile_row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 66.0), Sense::click())
        })
        .inner;
    let fill = if response.is_pointer_button_down_on() {
        theme::HOVER
    } else if selected {
        theme::ACCENT_DARK
    } else if response.hovered() {
        theme::HOVER
    } else {
        theme::SIDEBAR
    };
    ui.painter().rect_filled(rect, 10, fill);
    if selected || response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            10,
            Stroke::new(
                if response.has_focus() {
                    2.0_f32
                } else {
                    1.0_f32
                },
                theme::ACCENT,
            ),
            egui::StrokeKind::Inside,
        );
    }
    let color = if selected {
        theme::ACCENT
    } else {
        theme::MUTED
    };
    ui.painter().circle_stroke(
        rect.left_center() + egui::vec2(18.0, 0.0),
        4.0,
        Stroke::new(1.5_f32, color),
    );
    if selected {
        ui.painter()
            .circle_filled(rect.left_center() + egui::vec2(18.0, 0.0), 2.0, color);
    }
    let text_rect = rect.shrink2(egui::vec2(12.0, 10.0));
    let text_rect = egui::Rect::from_min_max(text_rect.min + egui::vec2(24.0, 0.0), text_rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.add(
            egui::Label::new(RichText::new(title).size(14.0).strong().color(if enabled {
                theme::TEXT
            } else {
                theme::MUTED
            }))
            .truncate(),
        )
        .on_hover_text(title);
        ui.add(egui::Label::new(RichText::new(subtitle).size(12.0).color(theme::MUTED)).truncate());
    });
    ui.advance_cursor_after_rect(rect);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, selected, title)
    });
    if enabled {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

fn power_symbol(
    painter: &egui::Painter,
    center: egui::Pos2,
    radius: f32,
    color: Color32,
    width: f32,
) {
    let arc = (0..=48)
        .map(|index| {
            let angle = std::f32::consts::PI * (-0.25 + index as f32 / 48.0 * 1.5);
            center + egui::vec2(angle.cos(), angle.sin()) * radius
        })
        .collect();
    painter.add(egui::Shape::line(arc, Stroke::new(width, color)));
    painter.line_segment(
        [
            center + egui::vec2(0.0, -radius * 1.25),
            center + egui::vec2(0.0, -radius * 0.1),
        ],
        Stroke::new(width, color),
    );
}

pub fn power_button(
    ui: &mut egui::Ui,
    size: f32,
    label: &str,
    color: Color32,
    active: bool,
    pending: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(egui::vec2(size, size + 28.0), Sense::click())
        })
        .inner;
    let center = rect.center_top() + egui::vec2(0.0, size * 0.5);
    let painter = ui.painter_at(rect.expand(4.0));
    let fill = if !enabled {
        theme::HOVER
    } else if !active {
        if response.hovered() || response.is_pointer_button_down_on() {
            theme::HOVER
        } else {
            theme::SURFACE
        }
    } else if response.is_pointer_button_down_on() {
        theme::CONTROL_PRESSED
    } else if response.hovered() {
        theme::CONTROL_HOVER
    } else {
        theme::CONTROL
    };
    painter.circle_stroke(center, size * 0.49, Stroke::new(1.0_f32, theme::BORDER));
    painter.circle_stroke(center, size * 0.445, Stroke::new(1.0_f32, theme::BORDER));
    painter.circle_filled(center, size * 0.39, fill);
    if response.has_focus() {
        painter.circle_stroke(center, size * 0.49, Stroke::new(2.0_f32, theme::ACCENT));
    }
    if pending {
        // A static progress arc plus an explicit state label also works without motion.
        let points = (0..=24)
            .map(|index| {
                let angle = index as f32 / 24.0 * std::f32::consts::PI * 0.8;
                center + egui::vec2(angle.cos(), angle.sin()) * size * 0.445
            })
            .collect();
        painter.add(egui::Shape::line(points, Stroke::new(2.0_f32, color)));
    }
    power_symbol(
        &painter,
        center,
        size * 0.13,
        if enabled { theme::TEXT } else { theme::MUTED },
        3.2,
    );
    painter.text(
        egui::pos2(center.x, rect.bottom() - 10.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(13.0),
        if enabled { theme::TEXT } else { theme::MUTED },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response.on_hover_text(label)
}

pub fn metric(ui: &mut egui::Ui, title: &str, value: &str) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.label(RichText::new(value).size(22.0).color(if value == "—" {
            theme::MUTED
        } else {
            theme::TEXT
        }));
        ui.label(RichText::new(title).size(12.0).color(theme::MUTED));
    });
}

#[derive(Clone, Copy)]
pub enum NavIcon {
    Connection,
    Profiles,
    Settings,
}

pub fn nav_button(
    ui: &mut egui::Ui,
    label: &str,
    icon: NavIcon,
    selected: bool,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 45.0), Sense::click())
        })
        .inner;
    let color = if selected {
        theme::ACCENT
    } else {
        theme::MUTED
    };
    let painter = ui.painter_at(rect);
    if response.hovered() && enabled {
        painter.rect_filled(rect, 9, theme::SURFACE);
    }
    let center = egui::pos2(rect.center().x, rect.top() + 12.0);
    let stroke = Stroke::new(1.6_f32, color);
    match icon {
        NavIcon::Connection => power_symbol(&painter, center, 7.0, color, 1.6),
        NavIcon::Profiles => {
            painter.rect_stroke(
                egui::Rect::from_center_size(center - egui::vec2(2.0, 2.0), egui::vec2(13.0, 15.0)),
                3,
                stroke,
                egui::StrokeKind::Inside,
            );
            painter.rect_filled(
                egui::Rect::from_center_size(center + egui::vec2(2.0, 2.0), egui::vec2(13.0, 15.0)),
                3,
                theme::BACKGROUND,
            );
            painter.rect_stroke(
                egui::Rect::from_center_size(center + egui::vec2(2.0, 2.0), egui::vec2(13.0, 15.0)),
                3,
                stroke,
                egui::StrokeKind::Inside,
            );
        }
        NavIcon::Settings => {
            for (offset, knob) in [(-6.0, -3.0), (0.0, 4.0), (6.0, -1.0)] {
                painter.line_segment(
                    [
                        center + egui::vec2(-9.0, offset),
                        center + egui::vec2(9.0, offset),
                    ],
                    stroke,
                );
                painter.circle(
                    center + egui::vec2(knob, offset),
                    2.4,
                    theme::BACKGROUND,
                    stroke,
                );
            }
        }
    }
    painter.text(
        egui::pos2(rect.center().x, rect.bottom() - 9.0),
        Align2::CENTER_CENTER,
        label,
        FontId::proportional(12.0),
        color,
    );
    if selected {
        painter.circle_filled(egui::pos2(rect.center().x, rect.top()), 2.0, color);
    }
    focus_outline(ui, &response);
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::SelectableLabel, enabled, selected, label)
    });
    response
}

pub fn profile_picker(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, response) = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), Sense::click())
        })
        .inner;
    let painter = ui.painter_at(rect);
    painter.rect_filled(
        rect,
        12,
        if response.hovered() && enabled {
            theme::HOVER
        } else {
            theme::SURFACE
        },
    );
    let center = rect.left_center() + egui::vec2(24.0, 0.0);
    let stroke = Stroke::new(1.5_f32, theme::ACCENT);
    painter.rect_stroke(
        egui::Rect::from_center_size(center, egui::vec2(15.0, 19.0)),
        3,
        stroke,
        egui::StrokeKind::Inside,
    );
    for offset in [-3.0, 2.0] {
        painter.line_segment(
            [
                center + egui::vec2(-3.0, offset),
                center + egui::vec2(3.0, offset),
            ],
            stroke,
        );
    }
    let arrow = rect.right_center() - egui::vec2(20.0, 0.0);
    painter.add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-3.0, -5.0),
            arrow + egui::vec2(2.0, 0.0),
            arrow + egui::vec2(-3.0, 5.0),
        ],
        Stroke::new(1.5_f32, theme::MUTED),
    ));
    let text_rect = egui::Rect::from_min_max(
        rect.min + egui::vec2(48.0, 10.0),
        rect.max - egui::vec2(36.0, 8.0),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(text_rect), |ui| {
        ui.spacing_mut().item_spacing.y = 3.0;
        ui.add(egui::Label::new(RichText::new(title).size(14.0).color(theme::TEXT)).truncate());
        ui.add(egui::Label::new(RichText::new(subtitle).size(12.0).color(theme::MUTED)).truncate());
    });
    ui.advance_cursor_after_rect(rect);
    focus_outline(ui, &response);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            enabled,
            format!("Cambiar perfil: {title}"),
        )
    });
    response.on_hover_text(format!("{title}\n{subtitle}"))
}

pub fn empty_profiles(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 95.0), Sense::hover());
    ui.painter().text(
        rect.center() - egui::vec2(0.0, 10.0),
        Align2::CENTER_CENTER,
        "Aún no tienes perfiles guardados",
        FontId::proportional(14.0),
        theme::TEXT,
    );
    ui.painter().text(
        rect.center() + egui::vec2(0.0, 13.0),
        Align2::CENTER_CENTER,
        "Importa un .conf y guárdalo cifrado",
        FontId::proportional(12.0),
        theme::MUTED,
    );
}

pub fn bytes(value: usize) -> String {
    let value = value as f64;
    if value < 1024.0 {
        return format!("{} B", value as usize);
    }
    for (divisor, unit) in [
        (1024_f64.powi(3), "GiB"),
        (1024_f64.powi(2), "MiB"),
        (1024.0, "KiB"),
    ] {
        if value >= divisor {
            return format!("{:.1} {unit}", value / divisor).replace('.', ",");
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circular_action_can_be_activated_with_the_keyboard() {
        let context = egui::Context::default();
        theme::apply(&context);
        let mut control_id = None;
        let _ = context.run(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                control_id = Some(
                    power_button(
                        ui,
                        148.0,
                        "Conectar proxy",
                        theme::MUTED,
                        false,
                        false,
                        true,
                    )
                    .id,
                );
            });
        });
        context.memory_mut(|memory| memory.request_focus(control_id.unwrap()));
        let mut clicked = false;
        for pressed in [true, false] {
            let _ = context.run(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Space,
                        physical_key: None,
                        pressed,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        clicked |= power_button(
                            ui,
                            148.0,
                            "Conectar proxy",
                            theme::MUTED,
                            false,
                            false,
                            true,
                        )
                        .clicked();
                    });
                },
            );
        }
        assert!(clicked);
    }

    fn button_frame(
        context: &egui::Context,
        action: WindowAction,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, egui::FullOutput) {
        let mut rect = egui::Rect::NOTHING;
        let output = context.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(420.0, 600.0),
                )),
                events,
                ..Default::default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    rect = window_button(ui, action).rect;
                });
            },
        );
        (rect, output)
    }

    #[test]
    fn minimize_and_close_request_only_the_corresponding_window_operation() {
        for action in [WindowAction::Minimize, WindowAction::Close] {
            let context = egui::Context::default();
            theme::apply(&context);
            let (rect, _) = button_frame(&context, action, vec![]);
            let point = rect.center();
            let _ = button_frame(
                &context,
                action,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
            );
            let (_, output) = button_frame(
                &context,
                action,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }],
            );
            let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
            assert_eq!(commands.len(), 1);
            assert!(match action {
                WindowAction::Minimize =>
                    matches!(commands[0], egui::ViewportCommand::Minimized(true)),
                WindowAction::Close => matches!(commands[0], egui::ViewportCommand::Close),
            });
        }
    }

    #[test]
    fn transfer_sizes_are_readable_and_use_consistent_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1,0 KiB");
        assert_eq!(bytes(1_572_864), "1,5 MiB");
        assert_eq!(bytes(1_073_741_824), "1,0 GiB");
    }
}
