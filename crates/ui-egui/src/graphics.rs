//! Desktop renderer selection, frame pacing and transparent composition.
use serde::{Deserialize, Serialize};

pub const FPS_LIMITS: [u16; 8] = [0, 30, 60, 90, 120, 144, 165, 240];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GraphicsRenderer {
    #[default]
    Auto,
    Wgpu,
    Gl,
}

impl GraphicsRenderer {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto),
            "wgpu" => Some(Self::Wgpu),
            "gl" | "opengl" | "glow" => Some(Self::Gl),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Automatic (recommended)",
            Self::Wgpu if cfg!(windows) => "Direct3D 12",
            Self::Wgpu => "Native GPU",
            Self::Gl => "OpenGL (compatibility)",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphicsSettings {
    pub renderer: GraphicsRenderer,
    pub fps_limit: u16,
}

impl GraphicsSettings {
    pub fn validated(mut self) -> Self {
        if !FPS_LIMITS.contains(&self.fps_limit) {
            self.fps_limit = 0;
        }
        self
    }
}

impl crate::PdfCraftApp {
    pub fn set_fps_limit(&mut self, fps: u16) -> Result<(), String> {
        if !FPS_LIMITS.contains(&fps) {
            return Err("fps-limit must be display, 0, 30, 60, 90, 120, 144, 165 or 240".into());
        }
        self.graphics.fps_limit = fps;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(runtime) = &self.graphics_runtime {
            runtime.set_limit(fps);
        }
        Ok(())
    }
}

pub(crate) fn preferences(ui: &mut egui::Ui, app: &mut crate::PdfCraftApp) {
    ui.label(tl!("Graphics backend"));
    ui.add_enabled_ui(!cfg!(target_arch = "wasm32"), |ui| {
        egui::ComboBox::from_id_salt("graphics-renderer")
            .selected_text(tl!(app.graphics.renderer.label()))
            .width(ui.available_width().min(230.0))
            .show_ui(ui, |ui| {
                for renderer in [GraphicsRenderer::Auto, GraphicsRenderer::Wgpu, GraphicsRenderer::Gl] {
                    ui.selectable_value(&mut app.graphics.renderer, renderer, tl!(renderer.label()));
                }
            });
        ui.label(tl!("Renderer changes apply after restarting Lion PDF."));
        ui.add_space(14.0);
        ui.label(tl!("Frame rate limit"));
        let selected = if app.graphics.fps_limit == 0 { tl!("Match display").to_owned() } else { format!("{} FPS", app.graphics.fps_limit) };
        egui::ComboBox::from_id_salt("graphics-fps-limit").selected_text(selected).width(ui.available_width().min(230.0)).show_ui(ui, |ui| {
            for fps in FPS_LIMITS {
                let label = if fps == 0 { tl!("Match display").to_owned() } else { format!("{fps} FPS") };
                if ui.selectable_label(app.graphics.fps_limit == fps, label).clicked()
                    && let Err(error) = app.set_fps_limit(fps)
                {
                    app.notify(error);
                }
            }
        });
        ui.label(tl!("The FPS limit applies immediately to desktop windows. Idle windows redraw only when needed."));
    });
    #[cfg(target_arch = "wasm32")]
    ui.label(tl!("Graphics and frame pacing are managed by your browser."));
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::{
        collections::HashMap,
        sync::atomic::{AtomicU16, Ordering},
        time::{Duration, Instant},
    };

    /// Shared by native viewports; each window keeps an independent frame clock.
    pub struct GraphicsRuntime {
        limit: AtomicU16,
        frames: egui::mutex::Mutex<HashMap<egui::ViewportId, Instant>>,
        #[cfg(windows)]
        composition: bool,
    }

    impl GraphicsRuntime {
        pub fn new(limit: u16, composition: bool) -> Self {
            #[cfg(not(windows))]
            let _ = composition;
            Self {
                limit: AtomicU16::new(if super::FPS_LIMITS.contains(&limit) { limit } else { 0 }),
                frames: Default::default(),
                #[cfg(windows)]
                composition,
            }
        }

        pub fn viewport(&self, builder: egui::ViewportBuilder) -> egui::ViewportBuilder {
            #[cfg(windows)]
            let builder = builder.with_no_redirection_bitmap(self.composition);
            builder
        }

        pub fn set_limit(&self, limit: u16) {
            self.limit.store(if super::FPS_LIMITS.contains(&limit) { limit } else { 0 }, Ordering::Relaxed);
            self.frames.lock().clear();
        }
    }

    fn frame_wait(elapsed: Duration, fps: u16) -> Duration {
        if fps == 0 || !super::FPS_LIMITS.contains(&fps) {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(1.0 / f64::from(fps)).saturating_sub(elapsed)
    }

    impl eframe::ViewportHooks for GraphicsRuntime {
        fn window_destroyed(&self, viewport: egui::ViewportId) {
            self.frames.lock().remove(&viewport);
        }

        fn raw_input(&self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
            let fps = self.limit.load(Ordering::Relaxed);
            if fps == 0 {
                return;
            }
            let last = self.frames.lock().get(&raw.viewport_id).copied();
            let wait = last.map_or(Duration::ZERO, |last| frame_wait(last.elapsed(), fps));
            if !wait.is_zero() {
                // A selected cap deliberately paces rendering, retaining every input event.
                let before = Instant::now();
                std::thread::sleep(wait);
                if let Some(time) = raw.time.as_mut().filter(|time| time.is_finite()) {
                    *time += before.elapsed().as_secs_f64();
                }
            }
            self.frames.lock().insert(raw.viewport_id, Instant::now());
            raw.predicted_dt = raw.predicted_dt.max(1.0 / f32::from(fps));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn caps_wait_only_the_remaining_frame_and_never_catch_up() {
            assert_eq!(frame_wait(Duration::ZERO, 0), Duration::ZERO);
            assert_eq!(frame_wait(Duration::ZERO, 1), Duration::ZERO);
            assert_eq!(frame_wait(Duration::from_millis(5), 60), Duration::from_secs_f64(1.0 / 60.0) - Duration::from_millis(5));
            assert_eq!(frame_wait(Duration::from_secs(1), 60), Duration::ZERO);
            assert!(frame_wait(Duration::ZERO, 30) > frame_wait(Duration::ZERO, 120));
        }
        #[test]
        fn native_cap_waits_for_the_frame_boundary_without_losing_input() {
            use eframe::ViewportHooks;
            let runtime = GraphicsRuntime::new(60, false);
            let ctx = egui::Context::default();
            let mut raw = egui::RawInput { time: Some(1.0), ..Default::default() };
            raw.events.push(egui::Event::Text("keep input".into()));
            runtime.raw_input(&ctx, &mut raw);
            let previous = *runtime.frames.lock().get(&raw.viewport_id).unwrap();
            runtime.raw_input(&ctx, &mut raw);
            assert!(previous.elapsed() >= Duration::from_secs_f64(1.0 / 60.0));
            assert!(raw.time.unwrap() > 1.0);
            assert_eq!(raw.events.len(), 1);
        }

        #[cfg(windows)]
        #[test]
        fn composition_is_a_creation_attribute_and_compatibility_windows_keep_redirection() {
            let native = GraphicsRuntime::new(0, true);
            let compatibility = GraphicsRuntime::new(0, false);
            let mut builder = native.viewport(egui::ViewportBuilder::default());
            assert_eq!(builder.no_redirection_bitmap, Some(true));
            let (_, recreate) = builder.patch(compatibility.viewport(egui::ViewportBuilder::default()));
            assert!(recreate, "this style cannot be changed after creation");
            assert_eq!(builder.no_redirection_bitmap, Some(false));
        }

        #[test]
        fn viewport_clocks_are_independent_and_closed_windows_are_retired() {
            use eframe::ViewportHooks;
            let runtime = GraphicsRuntime::new(60, false);
            let ctx = egui::Context::default();
            let mut root = egui::RawInput::default();
            root.events.push(egui::Event::Text("keep input".into()));
            runtime.raw_input(&ctx, &mut root);
            let child = egui::ViewportId::from_hash_of("fps-child");
            let mut input = egui::RawInput { viewport_id: child, ..Default::default() };
            runtime.raw_input(&ctx, &mut input);
            assert_eq!(runtime.frames.lock().len(), 2);
            assert_eq!(root.events.len(), 1, "frame caps must not drop input");
            runtime.window_destroyed(child);
            assert_eq!(runtime.frames.lock().len(), 1);
            runtime.set_limit(0);
            assert!(runtime.frames.lock().is_empty());
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::GraphicsRuntime;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_round_trip_and_invalid_input_preserves_the_choice() {
        let mut app = crate::PdfCraftApp::new();
        app.set_option("graphics-renderer", "wgpu").unwrap();
        app.set_option("fps-limit", "120").unwrap();
        assert!(app.set_option("fps-limit", "1").is_err());
        assert!(app.set_option("graphics-renderer", "other").is_err());
        let mut restored = crate::PdfCraftApp::new();
        restored.restore(&app.persist());
        assert_eq!(restored.graphics, GraphicsSettings { renderer: GraphicsRenderer::Wgpu, fps_limit: 120 });
        restored.restore(r#"{"graphics":{"renderer":"gl","fps_limit":65535}}"#);
        assert_eq!(restored.graphics.fps_limit, 0);
        assert_eq!(restored.graphics.renderer, GraphicsRenderer::Gl);
    }
    #[test]
    fn older_preferences_keep_automatic_renderer_and_display_rate() {
        let mut app = crate::PdfCraftApp::new();
        app.restore("{}");
        assert_eq!(app.graphics, GraphicsSettings::default());
        assert_eq!(crate::PreferencesCategory::parse("graphics").unwrap().key(), "graphics");
    }
}
