//! Four small, code-drawn themes; no raster assets or additional runtime resources.
use crate::{editor::highlighting::SyntaxPalette, settings::Appearance, ui::text};
use eframe::egui::{self, Color32, FontId, Stroke, TextStyle};
#[derive(Clone, Copy)]
pub struct Palette {
    pub bar: Color32,
    pub tabs: Color32,
    pub document: Color32,
    pub footer: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub accent: Color32,
    pub active: Color32,
    pub syntax: SyntaxPalette,
}
fn c(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}
pub fn label(theme: Appearance) -> &'static str {
    match theme {
        Appearance::Graphite => text::THEME_GRAPHITE,
        Appearance::Paper => text::THEME_PAPER,
        Appearance::Ocean => text::THEME_OCEAN,
        Appearance::Sand => text::THEME_SAND,
    }
}
pub fn palette(theme: Appearance) -> Palette {
    let (
        bar,
        tabs,
        document,
        footer,
        text,
        muted,
        faint,
        line,
        accent,
        active,
        key,
        string,
        number,
        comment,
    ) = match theme {
        Appearance::Graphite => (
            0x161719, 0x161719, 0x191a1d, 0x161719, 0xd5d7dc, 0x8b9099, 0x656b75, 0x292b30,
            0x96c8b3, 0x1e2122, 0xa9c6dc, 0xb9c9ad, 0xd5b592, 0x858e96,
        ),
        Appearance::Paper => (
            0xf5f6f8, 0xeef0f3, 0xffffff, 0xfafbfc, 0x28313c, 0x6e7a89, 0x969eaa, 0xe5e8ed,
            0x527eae, 0xffffff, 0x547a9e, 0x67846b, 0xa1784e, 0x7c8794,
        ),
        Appearance::Ocean => (
            0x151c2b, 0x151c2b, 0x192233, 0x151c2b, 0xcdd6e6, 0x8a9bb2, 0x63748e, 0x28334a,
            0xb3b2e7, 0x233047, 0x9fb7df, 0x9dc3b5, 0xd7b68c, 0x8598ae,
        ),
        Appearance::Sand => (
            0xf7f5ef, 0xf7f5ef, 0xfbfaf6, 0xfbfaf6, 0x494741, 0x827b6e, 0xa19a8d, 0xe9e5dc,
            0x9b7b52, 0xeeebe3, 0x778a92, 0x7b8966, 0xb08a63, 0x8e887b,
        ),
    };
    Palette {
        bar: c(bar),
        tabs: c(tabs),
        document: c(document),
        footer: c(footer),
        text: c(text),
        muted: c(muted),
        faint: c(faint),
        line: c(line),
        accent: c(accent),
        active: c(active),
        syntax: SyntaxPalette {
            foreground: c(text),
            keyword: c(key),
            string: c(string),
            number: c(number),
            comment: c(comment),
        },
    }
}
fn key() -> egui::Id {
    egui::Id::unique("tabloom-appearance")
}
pub fn current(ctx: &egui::Context) -> Appearance {
    ctx.data(|d| d.get_temp(key()))
        .unwrap_or(Appearance::Graphite)
}
pub fn apply(ctx: &egui::Context, appearance: Appearance) {
    let p = palette(appearance);
    ctx.set_theme(if appearance.dark() {
        egui::Theme::Dark
    } else {
        egui::Theme::Light
    });
    let mut style = egui::Style::default();
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(18.));
    style.spacing.item_spacing = egui::vec2(8., 6.);
    style.spacing.button_padding = egui::vec2(8., 5.);
    style.spacing.interact_size = egui::vec2(24., 26.);
    let v = &mut style.visuals;
    *v = if appearance.dark() {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.override_text_color = Some(p.text);
    v.weak_text_color = Some(p.muted);
    v.panel_fill = p.document;
    v.window_fill = p.bar;
    v.extreme_bg_color = p.document;
    v.text_edit_bg_color = Some(p.document);
    v.faint_bg_color = p.active;
    v.code_bg_color = p.active;
    v.window_stroke = Stroke::new(1., p.line);
    v.window_corner_radius = egui::CornerRadius::same(8);
    v.menu_corner_radius = egui::CornerRadius::same(6);
    v.selection.bg_fill = c(match appearance {
        Appearance::Graphite => 0x35483f,
        Appearance::Paper => 0xdce8f5,
        Appearance::Ocean => 0x3b4061,
        Appearance::Sand => 0xe4d8c1,
    });
    v.selection.stroke = Stroke::new(1., p.accent);
    v.hyperlink_color = p.accent;
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(4);
        widget.expansion = 0.;
        widget.fg_stroke = Stroke::new(1., p.text);
        widget.bg_stroke = Stroke::new(1., p.line);
        widget.bg_fill = p.active;
        widget.weak_bg_fill = p.active;
    }
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1., p.line);
    v.widgets.hovered.bg_stroke = Stroke::new(1., p.accent.gamma_multiply(0.4));
    v.widgets.active.bg_stroke = Stroke::new(1., p.accent);
    ctx.set_style_of(
        if appearance.dark() {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        },
        style,
    );
    ctx.data_mut(|d| d.insert_temp(key(), appearance));
    ctx.request_repaint();
}
pub fn panel(fill: Color32, x: i8, y: i8) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(x, y))
}
