//! Look and feel: a light/dark palette following Windows, Segoe UI, and the
//! handful of widgets the window is made of.

use crate::history::Entry;
use eframe::egui::{
    self, epaint::Shadow, Align, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Margin, Pos2, Rect,
    Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, Widget,
};
use std::path::{Path, PathBuf};

pub struct Palette {
    pub bg: Color32,
    pub card: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub ok: Color32,
    pub danger: Color32,
    pub hover: Color32,
    pub dark: bool,
}

impl Palette {
    pub fn of(ctx: &egui::Context) -> Palette {
        if ctx.style().visuals.dark_mode {
            Palette {
                bg: Color32::from_rgb(28, 28, 30),
                card: Color32::from_rgb(44, 44, 46),
                border: Color32::from_rgb(58, 58, 60),
                text: Color32::from_rgb(242, 242, 247),
                muted: Color32::from_rgb(152, 152, 159),
                accent: Color32::from_rgb(10, 132, 255),
                accent_soft: Color32::from_rgba_unmultiplied(10, 132, 255, 40),
                ok: Color32::from_rgb(48, 209, 88),
                danger: Color32::from_rgb(255, 69, 58),
                hover: Color32::from_rgb(58, 58, 60),
                dark: true,
            }
        } else {
            Palette {
                bg: Color32::from_rgb(242, 242, 247),
                card: Color32::WHITE,
                border: Color32::from_rgb(226, 226, 232),
                text: Color32::from_rgb(28, 28, 30),
                muted: Color32::from_rgb(110, 110, 118),
                accent: Color32::from_rgb(0, 122, 255),
                accent_soft: Color32::from_rgba_unmultiplied(0, 122, 255, 26),
                ok: Color32::from_rgb(36, 168, 72),
                danger: Color32::from_rgb(215, 50, 40),
                hover: Color32::from_rgb(240, 240, 245),
                dark: false,
            }
        }
    }
}

pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}

/// Segoe UI (and its semibold) first, egui's own fonts after for symbols.
pub fn setup(ctx: &egui::Context) {
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let fonts_dir = Path::new(&windir).join("Fonts");
    let mut fonts = egui::FontDefinitions::default();
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    for (key, file) in [("segoe", "segoeui.ttf"), ("segoe-sb", "seguisb.ttf")] {
        if let Ok(bytes) = std::fs::read(fonts_dir.join(file)) {
            fonts.font_data.insert(key.into(), std::sync::Arc::new(egui::FontData::from_owned(bytes)));
        }
    }
    let mut prop = Vec::new();
    if fonts.font_data.contains_key("segoe") {
        prop.push("segoe".to_string());
    }
    prop.extend(fallback.iter().cloned());
    let mut sb = Vec::new();
    if fonts.font_data.contains_key("segoe-sb") {
        sb.push("segoe-sb".to_string());
    }
    sb.extend(prop.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(semibold(), sb);
    ctx.set_fonts(fonts);
    ctx.options_mut(|o| o.theme_preference = egui::ThemePreference::System);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(12.0, 6.0);
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        for w in [
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.noninteractive,
        ] {
            w.corner_radius = CornerRadius::same(8);
        }
        s.visuals.selection.bg_fill = Color32::from_rgb(0, 122, 255);
    });
}

pub fn window_icon() -> egui::IconData {
    egui::IconData { rgba: crate::icon::icon_rgba(64), width: 64, height: 64 }
}

pub fn app_icon(ui: &mut Ui, size: f32) {
    let tex = ui.ctx().data_mut(|d| d.get_temp::<egui::TextureHandle>(egui::Id::new("app-icon")));
    let tex = tex.unwrap_or_else(|| {
        let img = egui::ColorImage::from_rgba_unmultiplied([128, 128], &crate::icon::icon_rgba(128));
        let t = ui.ctx().load_texture("app-icon", img, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("app-icon"), t.clone()));
        t
    });
    ui.add(egui::Image::new((tex.id(), Vec2::splat(size))));
}

pub fn window_frame(p: &Palette) -> Frame {
    Frame::new().fill(p.bg).inner_margin(Margin::symmetric(18, 16))
}

pub fn card(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(14)
        .inner_margin(Margin::same(16))
        .shadow(Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if p.dark { 60 } else { 14 }) })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

pub fn accent_card(ui: &mut Ui, p: &Palette, add: impl FnOnce(&mut Ui)) {
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.5_f32, p.accent))
        .corner_radius(14)
        .inner_margin(Margin::same(16))
        .shadow(Shadow { offset: [0, 2], blur: 14, spread: 0, color: p.accent_soft })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        });
}

pub fn section_title(ui: &mut Ui, p: &Palette, t: &str) {
    ui.label(RichText::new(t).size(15.0).family(semibold()).color(p.text));
}

pub fn hairline(ui: &mut Ui, p: &Palette) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0_f32, p.border));
}

/// An iOS-style switch.
pub struct Toggle<'a> {
    on: &'a mut bool,
    accent: Color32,
    off: Color32,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool, p: &Palette) -> Self {
        Toggle { on, accent: p.ok, off: if p.dark { Color32::from_rgb(72, 72, 74) } else { Color32::from_rgb(214, 214, 220) } }
    }
}

impl Widget for Toggle<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let size = Vec2::new(52.0, 30.0);
        let (rect, mut r) = ui.allocate_exact_size(size, Sense::click());
        if r.clicked() {
            *self.on = !*self.on;
            r.mark_changed();
        }
        let t = ui.ctx().animate_bool_responsive(r.id, *self.on);
        let mut bg = lerp_color(self.off, self.accent, t);
        if !ui.is_enabled() {
            bg = bg.gamma_multiply(0.55);
        }
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(15), bg);
        let x = egui::lerp((rect.left() + 15.0)..=(rect.right() - 15.0), t);
        painter.circle_filled(Pos2::new(x, rect.center().y + 1.0), 13.0, Color32::from_black_alpha(30));
        painter.circle_filled(Pos2::new(x, rect.center().y), 13.0, Color32::WHITE);
        r.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn segment(ui: &mut Ui, p: &Palette, label: &str, selected: bool) -> Response {
    let (fill, color) = if selected { (p.accent, Color32::WHITE) } else { (p.hover, p.text) };
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).color(color))
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(12))
            .min_size(Vec2::new(0.0, 24.0)),
    )
}

pub fn chip(ui: &mut Ui, p: &Palette, color: Color32, text: &str) {
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), if p.dark { 46 } else { 28 }))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(9, 3))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                ui.painter().circle_filled(r.center(), 3.5, color);
                ui.label(RichText::new(text).size(12.5).color(p.text));
            });
        });
}

pub fn primary_button(ui: &mut Ui, p: &Palette, t: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(t).color(Color32::WHITE).family(semibold()))
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(10))
            .min_size(Vec2::new(96.0, 34.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn secondary_button(ui: &mut Ui, p: &Palette, t: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(t).color(p.text))
            .fill(p.hover)
            .stroke(Stroke::new(1.0_f32, p.border))
            .corner_radius(CornerRadius::same(10))
            .min_size(Vec2::new(0.0, 34.0)),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn link_button(ui: &mut Ui, p: &Palette, t: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(t).color(p.accent).size(13.0)).frame(false))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str) -> Response {
    ui.add(egui::Button::new(RichText::new(glyph).size(18.0)).frame(false).min_size(Vec2::splat(32.0)))
        .on_hover_text(tip)
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().filter(|w| w.len() > 2 || name.split_whitespace().count() == 1).collect();
    let s: String = words.iter().take(2).filter_map(|w| w.chars().next()).collect();
    if s.is_empty() { "?".into() } else { s.to_uppercase() }
}

fn hue_for(name: &str) -> Color32 {
    let colors = [
        Color32::from_rgb(0, 122, 255),
        Color32::from_rgb(88, 86, 214),
        Color32::from_rgb(255, 149, 0),
        Color32::from_rgb(52, 199, 89),
        Color32::from_rgb(255, 45, 85),
        Color32::from_rgb(90, 200, 250),
        Color32::from_rgb(175, 82, 222),
    ];
    let h = name.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    colors[(h as usize) % colors.len()]
}

pub fn avatar(ui: &mut Ui, _p: &Palette, name: &str, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let c = hue_for(name);
    ui.painter().circle_filled(r.center(), size / 2.0, c);
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        initials(name),
        FontId::new(size * 0.38, semibold()),
        Color32::WHITE,
    );
}

/// A phone/laptop glyph drawn with shapes, for unnamed devices.
fn device_glyph(ui: &mut Ui, color: Color32, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().circle_filled(r.center(), size / 2.0, color);
    let body = Rect::from_center_size(r.center(), Vec2::new(size * 0.30, size * 0.50));
    ui.painter().rect_stroke(body, CornerRadius::same(3), Stroke::new(1.8_f32, Color32::WHITE), StrokeKind::Inside);
    ui.painter().hline(
        (body.center().x - 2.5)..=(body.center().x + 2.5),
        body.bottom() - 3.5,
        Stroke::new(1.5_f32, Color32::WHITE),
    );
}

pub fn device_row(ui: &mut Ui, p: &Palette, title: &str, sub: &str, enabled: bool) -> Response {
    let width = ui.available_width();
    let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 58.0), if enabled { Sense::click() } else { Sense::hover() });
    if enabled && r.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(12), p.hover);
    }
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(Vec2::new(10.0, 7.0))).layout(Layout::left_to_right(Align::Center)));
    if enabled {
        avatar(&mut child, p, title, 42.0);
    } else {
        device_glyph(&mut child, p.muted.gamma_multiply(0.7), 42.0);
    }
    child.add_space(6.0);
    child.vertical(|ui| {
        ui.add_space(2.0);
        ui.label(RichText::new(title).size(15.0).family(semibold()).color(if enabled { p.text } else { p.muted }));
        ui.label(RichText::new(sub).size(12.0).color(if enabled { p.ok } else { p.muted }));
    });
    if enabled {
        child.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new("Send ›").color(p.accent).family(semibold()));
        });
    }
    if enabled { r.on_hover_cursor(egui::CursorIcon::PointingHand) } else { r }
}

pub fn drop_zone(ui: &mut Ui, p: &Palette, title: &str, sub: &str) -> Response {
    let width = ui.available_width();
    let (rect, r) = ui.allocate_exact_size(Vec2::new(width, 96.0), Sense::click());
    let hovering_files = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
    let active = r.hovered() || hovering_files;
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(12), if active { p.accent_soft } else { p.hover });
    // Dashed border.
    let stroke = Stroke::new(1.5_f32, if active { p.accent } else { p.border.gamma_multiply(1.6) });
    let rr = rect.shrink(1.0);
    let corners = [rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()];
    for w in corners.windows(2) {
        painter.extend(egui::Shape::dashed_line(&[w[0], w[1]], stroke, 6.0, 4.0));
    }
    // Arrow-into-tray glyph.
    let c = Pos2::new(rect.center().x, rect.top() + 32.0);
    let s = Stroke::new(2.2_f32, p.accent);
    painter.line_segment([c + Vec2::new(0.0, -11.0), c + Vec2::new(0.0, 5.0)], s);
    painter.line_segment([c + Vec2::new(-6.0, -1.0), c + Vec2::new(0.0, 5.0)], s);
    painter.line_segment([c + Vec2::new(6.0, -1.0), c + Vec2::new(0.0, 5.0)], s);
    painter.line_segment([c + Vec2::new(-11.0, 10.0), c + Vec2::new(11.0, 10.0)], s);
    painter.text(rect.center() + Vec2::new(0.0, 16.0), egui::Align2::CENTER_CENTER, title, FontId::new(14.0, semibold()), p.text);
    painter.text(rect.center() + Vec2::new(0.0, 34.0), egui::Align2::CENTER_CENTER, sub, FontId::proportional(12.0), p.muted);
    r.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn ext_badge(ui: &mut Ui, p: &Palette, name: &str) {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
    let ext: String = ext.chars().take(4).collect();
    let color = match ext.as_str() {
        "JPG" | "JPEG" | "PNG" | "HEIC" | "GIF" | "WEBP" => Color32::from_rgb(255, 149, 0),
        "MOV" | "MP4" | "M4V" => Color32::from_rgb(175, 82, 222),
        "PDF" => Color32::from_rgb(255, 59, 48),
        "URL" | "WEBLOC" => Color32::from_rgb(0, 122, 255),
        "ZIP" => Color32::from_rgb(142, 142, 147),
        _ => Color32::from_rgb(90, 200, 250),
    };
    let (r, _) = ui.allocate_exact_size(Vec2::new(38.0, 38.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(9), Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), if p.dark { 60 } else { 34 }));
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        if ext.is_empty() { "FILE".into() } else { ext },
        FontId::new(10.5, semibold()),
        color,
    );
}

pub fn file_row(ui: &mut Ui, p: &Palette, name: &str, meta: &str) {
    ui.horizontal(|ui| {
        ext_badge(ui, p, name);
        ui.vertical(|ui| {
            ui.add(egui::Label::new(RichText::new(name).color(p.text)).truncate());
            ui.label(RichText::new(meta).size(12.0).color(p.muted));
        });
    });
}

pub fn countdown(ui: &mut Ui, p: &Palette, frac: f32, label: &str) {
    let w = 90.0;
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 6.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::same(3), p.border);
    let mut f = r;
    f.set_width(w * frac.clamp(0.0, 1.0));
    ui.painter().rect_filled(f, CornerRadius::same(3), if frac < 0.25 { p.danger } else { p.accent });
    ui.label(RichText::new(label).size(12.0).color(p.muted));
}

pub fn result_banner(ui: &mut Ui, p: &Palette, ok: bool, msg: &str) {
    let c = if ok { p.ok } else { p.danger };
    Frame::new()
        .fill(Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), if p.dark { 44 } else { 26 }))
        .corner_radius(12)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(if ok { "✔" } else { "✖" }).color(c).size(16.0));
                ui.add(egui::Label::new(RichText::new(msg).color(p.text)).wrap());
            });
        });
}

/// Concentric rings that pulse while looking for devices.
pub fn radar(ui: &mut Ui, p: &Palette, size: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let t = ui.ctx().input(|i| i.time) as f32;
    for k in 0..3 {
        let phase = ((t * 0.6 + k as f32 / 3.0) % 1.0) as f32;
        let rad = 6.0 + phase * (size / 2.0 - 6.0);
        let a = ((1.0 - phase) * 160.0) as u8;
        ui.painter().circle_stroke(
            r.center(),
            rad,
            Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(p.accent.r(), p.accent.g(), p.accent.b(), a)),
        );
    }
    ui.painter().circle_filled(r.center(), 6.0, p.accent);
    ui.ctx().request_repaint();
}

pub enum RowAction<'a> {
    Open(&'a PathBuf),
    Reveal(&'a PathBuf),
    Url(&'a String),
}

pub fn history_row<'a>(ui: &mut Ui, p: &Palette, e: &'a Entry, mut act: impl FnMut(RowAction<'a>)) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let name = e.path.as_ref().map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|| e.title.clone());
        if e.ok {
            ext_badge(ui, p, &name);
        } else {
            let (r, _) = ui.allocate_exact_size(Vec2::new(38.0, 38.0), Sense::hover());
            ui.painter().rect_filled(r, CornerRadius::same(9), Color32::from_rgba_unmultiplied(255, 59, 48, 34));
            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "!", FontId::new(18.0, semibold()), p.danger);
        }
        let right_w = 120.0;
        ui.allocate_ui(Vec2::new((ui.available_width() - right_w).max(80.0), 40.0), |ui| {
            ui.vertical(|ui| {
                ui.add(egui::Label::new(RichText::new(&e.title).color(if e.ok { p.text } else { p.danger })).truncate());
                let dir = if e.direction == "in" { "from" } else { "to" };
                let arrow = if e.direction == "in" { "↓" } else { "↑" };
                ui.add(
                    egui::Label::new(RichText::new(format!("{arrow} {} {} · {}", dir, e.peer, e.when())).size(12.0).color(p.muted))
                        .truncate(),
                );
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(path) = &e.path {
                if e.direction == "in" && path.exists() {
                    if link_button(ui, p, "Show").clicked() {
                        act(RowAction::Reveal(path));
                    }
                    if link_button(ui, p, "Open").clicked() {
                        act(RowAction::Open(path));
                    }
                }
            }
            if let Some(u) = &e.url {
                if link_button(ui, p, "Open link").clicked() {
                    act(RowAction::Url(u));
                }
            }
        });
    });
    ui.add_space(4.0);
}
