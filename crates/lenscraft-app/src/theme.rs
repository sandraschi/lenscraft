//! Craft UI theme, high-contrast visuals, vector widgets, and card frames
//! following EGUI_DESKTOP_UI_AND_PARITY.md.
use egui::{
    Align, Color32, Context, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, RichText,
    Stroke, StrokeKind, TextStyle, Ui, Vec2, Visuals,
};
use lenscraft_core::{Basis, Verdict};

pub const ACCENT_GOLD: Color32 = Color32::from_rgb(212, 168, 83);
pub const ACCENT_GOLD_DARK: Color32 = Color32::from_rgb(146, 98, 20);
pub const OK_GREEN: Color32 = Color32::from_rgb(61, 214, 140);
pub const OK_GREEN_DARK: Color32 = Color32::from_rgb(21, 128, 61);
pub const WARN_AMBER: Color32 = Color32::from_rgb(245, 185, 66);
pub const ERR_RED: Color32 = Color32::from_rgb(248, 113, 113);
pub const INFO_BLUE: Color32 = Color32::from_rgb(96, 165, 250);

/// High-contrast slate-950 dark visuals.
pub fn harsh_visuals() -> Visuals {
    let mut v = Visuals::dark();
    v.panel_fill = Color32::from_rgb(10, 13, 18);
    v.window_fill = Color32::from_rgb(14, 19, 28);
    v.extreme_bg_color = Color32::from_rgb(6, 9, 14);
    v.faint_bg_color = Color32::from_rgb(18, 25, 38);
    v.override_text_color = Some(Color32::from_rgb(245, 247, 250));
    v.widgets.noninteractive.bg_fill = Color32::from_rgb(16, 23, 36);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(38, 54, 84));
    v.widgets.inactive.bg_fill = Color32::from_rgb(22, 31, 48);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(48, 66, 102));
    v.widgets.hovered.bg_fill = Color32::from_rgb(30, 42, 64);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, ACCENT_GOLD);
    v.widgets.active.bg_fill = Color32::from_rgb(38, 54, 82);
    v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_GOLD);
    v.selection.bg_fill = Color32::from_rgb(56, 78, 116);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(44, 62, 94));
    v
}

/// High-contrast slate-50 light visuals (never light grey on white).
pub fn crisp_light_visuals() -> Visuals {
    let mut v = Visuals::light();
    v.panel_fill = Color32::from_rgb(248, 250, 252);
    v.window_fill = Color32::from_rgb(255, 255, 255);
    v.extreme_bg_color = Color32::from_rgb(241, 245, 249);
    v.faint_bg_color = Color32::from_rgb(241, 245, 249);
    v.override_text_color = Some(Color32::from_rgb(15, 23, 42));
    v.widgets.noninteractive.bg_fill = Color32::from_rgb(255, 255, 255);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    v.widgets.inactive.bg_fill = Color32::from_rgb(241, 245, 249);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(148, 163, 184));
    v.widgets.hovered.bg_fill = Color32::from_rgb(226, 232, 240);
    v.widgets.hovered.bg_stroke = Stroke::new(1.5, ACCENT_GOLD_DARK);
    v.widgets.active.bg_fill = Color32::from_rgb(203, 213, 225);
    v.widgets.active.bg_stroke = Stroke::new(1.5, ACCENT_GOLD_DARK);
    v.selection.bg_fill = Color32::from_rgb(219, 234, 254);
    v.window_stroke = Stroke::new(1.0, Color32::from_rgb(203, 213, 225));
    v
}

pub fn apply_craft_style(ctx: &Context, dark_mode: bool) {
    let theme = if dark_mode {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    };
    ctx.set_theme(theme);
    let mut style = (*ctx.style_of(theme)).clone();
    style.visuals = if dark_mode {
        harsh_visuals()
    } else {
        crisp_light_visuals()
    };
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(19.0, FontFamily::Proportional),
    );
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.5, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    ctx.set_style_of(theme, style);
}

/// Framed card with 1px hairline border and 3px left domain accent stripe.
pub fn card_frame<R>(ui: &mut Ui, accent: Color32, add_contents: impl FnOnce(&mut Ui) -> R) -> R {
    let dark = ui.visuals().dark_mode;
    let fill = if dark {
        Color32::from_rgb(16, 23, 36)
    } else {
        Color32::from_rgb(255, 255, 255)
    };
    let border = if dark {
        Color32::from_rgb(38, 54, 84)
    } else {
        Color32::from_rgb(203, 213, 225)
    };
    let resp = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add_contents(ui)
        });
    let r = resp.response.rect;
    let stripe = egui::Rect::from_min_max(r.min, egui::pos2(r.left() + 3.5, r.bottom()));
    ui.painter()
        .rect_filled(stripe, CornerRadius::same(2), accent);
    resp.inner
}

/// Primary gold/amber action button.
pub fn primary_button(ui: &mut Ui, label: &str) -> egui::Response {
    let dark = ui.visuals().dark_mode;
    let bg = if dark { ACCENT_GOLD } else { ACCENT_GOLD_DARK };
    let fg = if dark {
        Color32::from_rgb(16, 12, 6)
    } else {
        Color32::WHITE
    };
    ui.add(
        egui::Button::new(RichText::new(label).strong().color(fg))
            .fill(bg)
            .corner_radius(CornerRadius::same(6)),
    )
}

/// Vector-painted status dot (no Unicode symbol / tofu risk).
pub fn status_dot(ui: &mut Ui, ready: Option<bool>, tooltip: &str) {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(12.0, 12.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let color = match ready {
            Some(true) => OK_GREEN,
            Some(false) => ERR_RED,
            None => WARN_AMBER,
        };
        ui.painter().circle_filled(rect.center(), 4.5, color);
    }
    resp.on_hover_text(tooltip);
}

/// Verdict & basis pill badge.
pub fn verdict_badge(ui: &mut Ui, verdict: Verdict, basis: Basis) {
    let dark = ui.visuals().dark_mode;
    let (text, color) = match verdict {
        Verdict::Identified => ("IDENTIFIED", if dark { OK_GREEN } else { OK_GREEN_DARK }),
        Verdict::Likely => ("LIKELY", WARN_AMBER),
        Verdict::Uncertain => ("UNCERTAIN", Color32::from_rgb(148, 163, 184)),
    };
    let basis_txt = match basis {
        Basis::Label => "wall label",
        Basis::Recognition => "visual recognition",
        Basis::None => "none",
    };
    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
        Frame::new()
            .fill(color.linear_multiply(0.18))
            .stroke(Stroke::new(1.0, color))
            .corner_radius(CornerRadius::same(5))
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(text).strong().small().color(color));
            });
        ui.weak(format!("basis: {basis_txt}"));
    });
}

/// Vector-painted Sun / Crescent Moon theme toggle button.
pub fn theme_toggle_button(ui: &mut Ui, dark_mode: &mut bool) -> bool {
    let size = Vec2::new(28.0, 24.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(
            rect,
            CornerRadius::same(6),
            visuals.bg_fill,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
        let c = rect.center();
        if *dark_mode {
            let sun = Color32::from_rgb(251, 191, 36);
            ui.painter().circle_filled(c, 4.2, sun);
            for k in 0..8 {
                let a = (k as f32) * std::f32::consts::TAU / 8.0;
                let dir = Vec2::new(a.cos(), a.sin());
                ui.painter()
                    .line_segment([c + dir * 6.2, c + dir * 8.8], Stroke::new(1.5, sun));
            }
        } else {
            let moon = Color32::from_rgb(51, 65, 85);
            ui.painter().circle_filled(c, 6.0, moon);
            ui.painter()
                .circle_filled(c + Vec2::new(2.8, -2.0), 5.0, visuals.bg_fill);
        }
    }
    let clicked = resp.on_hover_text("Toggle Dark / Light theme").clicked();
    if clicked {
        *dark_mode = !*dark_mode;
    }
    clicked
}
