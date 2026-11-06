use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};

// Quiet charcoal surfaces and a restrained sage/teal accent, inspired by the reference.
pub const BACKGROUND: Color32 = Color32::from_rgb(23, 25, 24);
pub const SIDEBAR: Color32 = Color32::from_rgb(28, 31, 29);
pub const SURFACE: Color32 = Color32::from_rgb(33, 37, 35);
pub const HOVER: Color32 = Color32::from_rgb(44, 50, 46);
pub const BORDER: Color32 = Color32::from_rgb(59, 66, 61);
pub const TEXT: Color32 = Color32::from_rgb(240, 241, 234);
pub const MUTED: Color32 = Color32::from_rgb(164, 174, 165);
pub const ACCENT: Color32 = Color32::from_rgb(138, 198, 178);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(158, 215, 195);
pub const ACCENT_PRESSED: Color32 = Color32::from_rgb(118, 178, 158);
pub const ACCENT_DARK: Color32 = Color32::from_rgb(35, 57, 48);
pub const CONTROL: Color32 = Color32::from_rgb(48, 113, 101);
pub const CONTROL_HOVER: Color32 = Color32::from_rgb(52, 120, 107);
pub const CONTROL_PRESSED: Color32 = Color32::from_rgb(39, 95, 85);
pub const SUCCESS: Color32 = ACCENT;
pub const WARNING: Color32 = Color32::from_rgb(235, 199, 139);
pub const DANGER: Color32 = Color32::from_rgb(247, 165, 151);
pub const DANGER_DARK: Color32 = Color32::from_rgb(65, 37, 33);
pub const CLOSE_HOVER: Color32 = Color32::from_rgb(144, 58, 45);

pub fn apply(context: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        egui::FontData::from_static(include_bytes!("../../../../assets/fonts/Inter.ttf")).into(),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    context.set_fonts(fonts);
    // Configure the dark style explicitly: the active theme can change when the
    // native backend first reports Windows' light/dark preference after startup.
    context.set_theme(egui::ThemePreference::Dark);
    let mut style = (*context.style_of(egui::Theme::Dark)).clone();
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = SURFACE;
    visuals.extreme_bg_color = SIDEBAR;
    visuals.faint_bg_color = SURFACE;
    visuals.override_text_color = Some(TEXT);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = DANGER;
    visuals.text_cursor.stroke = Stroke::new(2.0_f32, ACCENT);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, TEXT);
    visuals.selection.bg_fill = ACCENT_DARK;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.inactive.bg_fill = SURFACE;
    visuals.widgets.inactive.weak_bg_fill = SURFACE;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    visuals.widgets.hovered.bg_fill = HOVER;
    visuals.widgets.hovered.weak_bg_fill = HOVER;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    visuals.widgets.active.bg_fill = ACCENT_DARK;
    visuals.widgets.active.weak_bg_fill = ACCENT_DARK;
    visuals.widgets.active.bg_stroke = Stroke::new(2.0_f32, ACCENT);
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(9);
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(9);
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(9);
    style.visuals = visuals;
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size.y = 34.0;
    style.spacing.text_edit_width = 160.0;
    style.spacing.scroll = egui::style::ScrollStyle::solid();
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(22.0));
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(14.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Monospace, FontId::monospace(13.0));
    context.set_style_of(egui::Theme::Dark, style);
}

pub fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0_f32, BORDER))
        .corner_radius(12)
        .inner_margin(16)
}
