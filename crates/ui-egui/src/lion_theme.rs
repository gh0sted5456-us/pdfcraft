//! Lion PDF's look on the craft frame: the Lion theme (`themes/lion.json`: the neutral chrome,
//! white panels and one blue accent Lion PDF has always had) and Glass, the dark theme drawn
//! over a see-through window in a tint the user chooses.
//!
//! Glass keeps the Lion theme's dark colours for text, lines and the accent, and clears the
//! window's surfaces (panels, dialogs and menus keep a backing): the window's clear colour paints the tint once (`clear_color`), so nested panels
//! don't stack their opacity. Hover, pressed and selected states are lighter shades of the tint;
//! menus, popups and dialogs sit on a more opaque shade so they stay legible over pages and the
//! desktop.

use std::sync::{Arc, LazyLock};

use craft::theme::{ColorToken, Rgba, ThemeDef};

/// The Lion theme's file, embedded.
const LION_JSON: &str = include_str!("../themes/lion.json");

// A defect in the embedded file is a build defect; `the_lion_theme_parses` catches it.
#[allow(clippy::panic)]
static LION: LazyLock<Arc<ThemeDef>> = LazyLock::new(|| match craft::theme::format::parse(LION_JSON.as_bytes()) {
    Ok(file) => Arc::new(file.theme),
    Err(errors) => panic!("themes/lion.json is invalid: {}", errors.0.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")),
});

/// The Lion theme.
pub fn lion() -> Arc<ThemeDef> {
    Arc::clone(&LION)
}

/// The Glass theme for `settings`, built once per change of settings: the framework resolves a
/// theme's tokens once for each definition it is given.
pub fn glass(settings: GlassSettings) -> Arc<ThemeDef> {
    static LAST: std::sync::Mutex<Option<(GlassSettings, Arc<ThemeDef>)>> = std::sync::Mutex::new(None);
    let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match &*last {
        Some((at, def)) if *at == settings => Arc::clone(def),
        _ => {
            let def = settings.theme();
            *last = Some((settings, Arc::clone(&def)));
            def
        }
    }
}

/// Glass's tint and opacities, as Preferences ▸ Appearance sets them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct GlassSettings {
    /// The tint, RGB.
    pub tint: [u8; 3],
    /// The window's opacity, in percent (35–100).
    pub opacity: u8,
    /// Menus', popups' and dialogs' opacity, in percent (35–100).
    pub popup_opacity: u8,
    /// The shade over the tool panes, in percent (0–100).
    pub tool_opacity: u8,
}

impl Default for GlassSettings {
    fn default() -> Self {
        Self { tint: [48, 112, 156], opacity: 80, popup_opacity: 88, tool_opacity: 38 }
    }
}

impl GlassSettings {
    /// The settings with every opacity in range.
    pub fn normalized(mut self) -> Self {
        self.opacity = self.opacity.clamp(35, 100);
        self.popup_opacity = self.popup_opacity.clamp(35, 100);
        self.tool_opacity = self.tool_opacity.min(100);
        self
    }

    fn alpha(percent: u8) -> u8 {
        u8::try_from(u16::from(percent.min(100)) * 255 / 100).unwrap_or(u8::MAX)
    }

    /// The tint darkened for a backdrop, at `opacity` percent.
    fn backdrop(self, opacity: u8) -> egui::Color32 {
        let rgb = self.tint.map(|channel| 18 + channel / 5);
        egui::Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], Self::alpha(opacity))
    }

    /// The window's backdrop: what `clear_color` paints under everything.
    pub fn surface(self) -> egui::Color32 {
        let s = self.normalized();
        s.backdrop(s.opacity)
    }

    /// The Glass theme for these settings: the Lion theme's dark mode with clear surfaces.
    pub fn theme(self) -> Arc<ThemeDef> {
        let s = self.normalized();
        let mut def = ThemeDef::clone(&LION);
        // A theme id of its own, so the framework doesn't take Glass for the Lion theme.
        if let Ok(id) = craft::theme::ThemeId::parse("lion.glass") {
            def.id = id;
        }
        def.name = "Lion PDF Glass".into();
        let raised = |alpha: u8| {
            let rgb = s.tint.map(|channel| 26 + channel / 5);
            Rgba([rgb[0], rgb[1], rgb[2], alpha])
        };
        let backdrop = s.tint.map(|channel| 18 + channel / 5);
        let popup = Rgba([backdrop[0], backdrop[1], backdrop[2], Self::alpha(s.popup_opacity)]);
        let clear = Rgba([0, 0, 0, 0]);
        let pins = &mut def.pins.dark.colors;
        for token in [ColorToken::SurfaceBackground, ColorToken::SurfaceCanvas, ColorToken::SurfaceChrome, ColorToken::SurfaceSunken] {
            pins.insert(token, clear);
        }
        // The framework draws dialogs, menus and popups on the panel colour, so panels take the
        // popups' backing: a dialog over a page stays legible.
        pins.insert(ColorToken::SurfacePanel, popup);
        pins.insert(ColorToken::SurfaceRaised, popup);
        pins.insert(ColorToken::StateHover, raised(40));
        pins.insert(ColorToken::StatePressed, raised(75));
        pins.insert(ColorToken::StateSelected, raised(60));
        pins.insert(ColorToken::AccentSoft, raised(60));
        def.pins.dark.refs.remove(&ColorToken::StateSelectedHover);
        pins.insert(ColorToken::StateSelectedHover, raised(75));
        Arc::new(def)
    }

    /// The shade drawn over the tool panes: the pages stay clear.
    pub fn tool_shade(self) -> egui::Color32 {
        egui::Color32::from_black_alpha(Self::alpha(self.normalized().tool_opacity))
    }
}

/// egui's own visuals in the Lion theme's colours, for the egui widgets Lion PDF's dialogs draw
/// beside the framework's components (combo boxes, text edits, sliders): dark for the dark mode
/// and Glass, with Glass's clear surfaces and tinted states.
pub fn egui_visuals(dark: bool, glass: Option<GlassSettings>) -> egui::Visuals {
    let def = match glass {
        Some(settings) => self::glass(settings),
        None => lion(),
    };
    let pins = if dark || glass.is_some() { &def.pins.dark } else { &def.pins.light };
    let base = if dark || glass.is_some() { egui::Visuals::dark() } else { egui::Visuals::light() };
    let color = |token: ColorToken, fallback: egui::Color32| {
        pins.get(token).map_or(fallback, |c| egui::Color32::from_rgba_unmultiplied(c.0[0], c.0[1], c.0[2], c.0[3]))
    };
    let mut v = base;
    let text = color(ColorToken::TextNormal, v.text_color());
    let border = color(ColorToken::LineBorder, v.widgets.noninteractive.bg_stroke.color);
    let hover = color(ColorToken::StateHover, v.widgets.hovered.bg_fill);
    let pressed = color(ColorToken::StatePressed, v.widgets.active.bg_fill);
    let selected = color(ColorToken::StateSelected, v.selection.bg_fill);
    let accent = color(ColorToken::AccentSolid, v.hyperlink_color);
    let panel = color(ColorToken::SurfacePanel, v.panel_fill);
    let raised = color(ColorToken::SurfaceRaised, v.window_fill);
    let sunken = color(ColorToken::SurfaceSunken, v.extreme_bg_color);
    v.override_text_color = Some(text);
    v.weak_text_color = Some(color(ColorToken::TextMuted, v.weak_text_color()));
    v.hyperlink_color = accent;
    v.panel_fill = panel;
    v.window_fill = raised;
    v.extreme_bg_color = sunken;
    v.faint_bg_color = hover;
    v.code_bg_color = sunken;
    v.window_stroke = egui::Stroke::new(1.0, border);
    v.selection.bg_fill = selected;
    v.selection.stroke = egui::Stroke::new(1.0, accent);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, panel),
        (&mut v.widgets.inactive, sunken),
        (&mut v.widgets.hovered, hover),
        (&mut v.widgets.active, pressed),
        (&mut v.widgets.open, pressed),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = egui::Stroke::new(1.0, border);
        w.fg_stroke = egui::Stroke::new(1.0, text);
        w.corner_radius = egui::CornerRadius::same(6);
    }
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, accent);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lion_theme_parses_and_keeps_its_colours() {
        let lion = lion();
        assert_eq!(lion.id.as_str(), "lion.pdf");
        assert_eq!(lion.pins.light.get(ColorToken::AccentSolid), Some(Rgba([0x1b, 0x63, 0xe0, 255])));
        assert_eq!(lion.pins.dark.get(ColorToken::SurfacePanel), Some(Rgba([0x26, 0x26, 0x2a, 255])));
    }

    #[test]
    fn glass_clears_the_surfaces_and_keeps_the_text() {
        let glass = GlassSettings::default().theme();
        assert_eq!(glass.id.as_str(), "lion.glass");
        assert_eq!(glass.pins.dark.get(ColorToken::SurfaceChrome).map(|c| c.0[3]), Some(0));
        assert!(glass.pins.dark.get(ColorToken::SurfacePanel).is_some_and(|c| c.0[3] > 200), "dialogs stay legible");
        assert_eq!(glass.pins.dark.get(ColorToken::TextNormal), lion().pins.dark.get(ColorToken::TextNormal));
        assert!(glass.pins.dark.get(ColorToken::SurfaceRaised).is_some_and(|c| c.0[3] > 200));
    }

    #[test]
    fn glass_opacities_stay_in_range() {
        let s = GlassSettings { tint: [0, 0, 0], opacity: 5, popup_opacity: 200, tool_opacity: 250 }.normalized();
        assert_eq!((s.opacity, s.popup_opacity, s.tool_opacity), (35, 100, 100));
    }
}
