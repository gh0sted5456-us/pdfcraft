//! Lion PDF: the colours Lion PDF's own egui drawing reads, from egui's visuals, which follow the
//! Lion theme (`lion_theme::egui_visuals`).

use egui::Color32;

/// The colours a Lion PDF dialog paints with.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub panel: Color32,
    pub field: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub accent: Color32,
}

impl Tokens {
    /// The colours of `ctx`'s current visuals.
    pub fn get(ctx: &egui::Context) -> Self {
        let v = ctx.global_style().visuals.clone();
        Self {
            text: v.text_color(),
            text_muted: v.weak_text_color(),
            text_faint: v.weak_text_color(),
            panel: v.panel_fill,
            field: v.extreme_bg_color,
            hover: v.faint_bg_color,
            border: v.widgets.noninteractive.bg_stroke.color,
            accent: v.hyperlink_color,
        }
    }
}
