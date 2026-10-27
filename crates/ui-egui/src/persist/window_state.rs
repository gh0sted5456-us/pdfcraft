//! The window's saved size and place across starts. eframe stores them just before
//! `App::save`, from the window as it is at that moment. A minimized window has neither: Windows
//! reports it as 0 × 0 at (-32000, -32000), and the next start then opened at egui's 64-point
//! floor in the screen's corner. So while the window is minimized the last settings it was shown
//! with are put back, and a window that opens at that floor is given a usable size.

//! On Windows, maximized settings also contain the hidden border offset. Keep normal physical
//! bounds separately so Restore after a restart does not put the integrated header off-screen.

/// eframe's storage key for the window settings.
const WINDOW_KEY: &str = "window";
/// The size egui opens a stored size below (its floor for "ridiculously small" windows), in points.
const EGUI_FLOOR: f32 = 64.0;
/// The size a window that opened at [`EGUI_FLOOR`] is given (apps/pdfcraft opens a first start at
/// this size too), at most nine tenths of the monitor.
const USABLE: egui::Vec2 = egui::vec2(1440.0, 920.0);

#[derive(Debug, Default)]
pub struct WindowState {
    /// The settings eframe last stored for a shown (not minimized) window.
    shown: Option<String>,
    /// The size at start has been checked ([`Self::repair`]).
    checked: bool,
    #[cfg(target_os = "windows")]
    normal: Option<NormalBounds>,
    #[cfg(target_os = "windows")]
    special: bool,
}

impl WindowState {
    /// The settings the window started with: what to put back if it is minimized before eframe
    /// stores a shown one.
    pub fn started_with(storage: Option<&dyn eframe::Storage>) -> Self {
        Self {
            shown: storage.and_then(|s| s.get_string(WINDOW_KEY)),
            #[cfg(target_os = "windows")]
            normal: storage.and_then(|s| eframe::get_value::<NormalBounds>(s, "pdfcraft-normal-window")).filter(NormalBounds::valid),
            ..Self::default()
        }
    }

    /// After eframe has stored the window: remember it while the window is shown, and while it
    /// is minimized put the last shown settings back (or none, so the next start opens at the
    /// default size).
    pub fn keep(&mut self, storage: &mut dyn eframe::Storage, minimized: bool) {
        #[cfg(target_os = "windows")]
        if let Some(normal) = self.normal {
            eframe::set_value(storage, "pdfcraft-normal-window", &normal);
        }
        if !minimized {
            self.shown = storage.get_string(WINDOW_KEY);
        } else if let Some(shown) = &self.shown {
            storage.set_string(WINDOW_KEY, shown.clone());
        } else {
            storage.remove_string(WINDOW_KEY);
        }
    }

    /// Once the window's size is known: a shown window at egui's floor in both directions came
    /// from settings stored while it was minimized (before this fix), so give it a usable size.
    pub fn repair(&mut self, ctx: &egui::Context, _app_frame: bool) {
        #[cfg(target_os = "windows")]
        self.repair_restore(ctx, _app_frame);
        if self.checked {
            return;
        }
        let (rect, minimized, monitor) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().minimized, i.viewport().monitor_size));
        let Some(rect) = rect else { return };
        if minimized == Some(true) {
            return;
        }
        self.checked = true;
        if rect.width() <= EGUI_FLOOR + 0.5 && rect.height() <= EGUI_FLOOR + 0.5 {
            // The stored settings are the broken ones: never put them back.
            self.shown = None;
            let size = monitor.map_or(USABLE, |m| USABLE.min(m * 0.9));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }
}

/// Physical geometry survives changes to monitor DPI and interface zoom.
#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
struct NormalBounds {
    position: egui::Pos2,
    size: egui::Vec2,
}

#[cfg(target_os = "windows")]
impl NormalBounds {
    fn valid(&self) -> bool {
        self.position.is_finite()
            && self.position.x.abs() <= 1_000_000.0
            && self.position.y.abs() <= 1_000_000.0
            && self.size.is_finite()
            && self.size.min_elem() >= EGUI_FLOOR
            && self.size.max_elem() <= 100_000.0
    }

    fn fit(self, work: egui::Rect) -> Self {
        let size = self.size.min(work.size());
        Self { position: self.position.clamp(work.min, work.max - size), size }
    }
}

#[cfg(target_os = "windows")]
impl WindowState {
    fn repair_restore(&mut self, ctx: &egui::Context, app_frame: bool) {
        let (outer, inner, minimized, maximized, fullscreen) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.inner_rect, v.minimized, v.maximized, v.fullscreen)
        });
        if minimized == Some(true) || maximized.is_none() {
            return;
        }
        let special = maximized == Some(true) || fullscreen == Some(true);
        let restored = self.special && !special;
        self.special = special;
        if special || !app_frame {
            return;
        }
        let (Some(outer), Some(inner)) = (outer, inner) else { return };
        let scale = ctx.pixels_per_point();
        if !scale.is_finite() || !(0.01..=100.0).contains(&scale) {
            return;
        }
        let current = NormalBounds { position: outer.min * scale, size: inner.size() * scale };
        if !current.valid() {
            return;
        }
        if !self.checked || restored {
            // Windows may restore a valid size under the pointer when dragging down the title bar.
            let wanted = if restored && self.normal.is_some_and(|normal| (normal.size - current.size).abs().max_elem() > 1.0) {
                self.normal.unwrap_or(current)
            } else {
                current
            };
            // The center selects the right work area even when the old border begins off-screen.
            let center = wanted.position + wanted.size * 0.5;
            let monitor = winsafe::HMONITOR::MonitorFromPoint(
                winsafe::POINT { x: center.x as i32, y: center.y as i32 },
                winsafe::co::MONITOR::DEFAULTTONEAREST,
            );
            if let Ok(info) = monitor.GetMonitorInfo() {
                let r = info.rcWork;
                let work = egui::Rect::from_min_max(egui::pos2(r.left as f32, r.top as f32), egui::pos2(r.right as f32, r.bottom as f32));
                if work.is_positive() && work.is_finite() {
                    let wanted = if restored && self.normal.is_none() {
                        // Older builds saved maximized bounds as the normal window.
                        NormalBounds { position: current.position, size: (USABLE * scale).min(work.size() * 0.9) }
                    } else {
                        wanted
                    };
                    let target = wanted.fit(work);
                    if current.position.distance(target.position) > 1.0 || (current.size - target.size).abs().max_elem() > 1.0 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(target.position / scale));
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(target.size / scale));
                        self.normal = Some(target);
                        return;
                    }
                }
            }
        }
        self.normal = Some(current);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::Storage as _;

    #[derive(Default)]
    struct Store(std::collections::HashMap<String, String>);

    impl eframe::Storage for Store {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_string(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    const SHOWN: &str = "(outer_position_pixels:Some((x:200.0,y:200.0)),inner_size_points:Some((x:1320.0,y:940.0)))";
    const MINIMIZED: &str = "(outer_position_pixels:Some((x:-32000.0,y:-32000.0)),inner_size_points:Some((x:0.0,y:0.0)))";

    /// Closing PdfCraft while it was minimized stored a 0 x 0 window off screen.
    #[test]
    fn a_minimized_window_keeps_the_last_shown_settings() {
        let mut store = Store::default();
        store.set_string(WINDOW_KEY, SHOWN.into());
        let mut state = WindowState::started_with(Some(&store));
        // eframe stores the minimized window, then the app saves.
        store.set_string(WINDOW_KEY, MINIMIZED.into());
        state.keep(&mut store, true);
        assert_eq!(store.get_string(WINDOW_KEY).as_deref(), Some(SHOWN));
        // Shown again and resized: that is what is kept from then on.
        let moved = SHOWN.replace("200.0", "300.0");
        store.set_string(WINDOW_KEY, moved.clone());
        state.keep(&mut store, false);
        store.set_string(WINDOW_KEY, MINIMIZED.into());
        state.keep(&mut store, true);
        assert_eq!(store.get_string(WINDOW_KEY), Some(moved));
    }

    #[test]
    fn with_nothing_shown_yet_a_minimized_save_stores_no_window() {
        let mut store = Store::default();
        let mut state = WindowState::started_with(Some(&store));
        store.set_string(WINDOW_KEY, MINIMIZED.into());
        state.keep(&mut store, true);
        assert_eq!(store.get_string(WINDOW_KEY), None, "the next start opens at the default size");
    }

    fn repaired(size: egui::Vec2, minimized: Option<bool>) -> Vec<egui::ViewportCommand> {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput::default();
        let info = input.viewports.entry(egui::ViewportId::ROOT).or_default();
        info.inner_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size));
        info.minimized = minimized;
        info.monitor_size = Some(egui::vec2(2674.0, 1504.0));
        let mut state = WindowState { shown: Some(MINIMIZED.into()), ..Default::default() };
        let out = ctx.run_logic(&input, |ctx| state.repair(ctx, false));
        let commands = out.viewport_commands.get(&egui::ViewportId::ROOT).cloned().unwrap_or_default();
        // Repaired: the settings it started with are not put back by a minimized close.
        assert_eq!(state.shown.is_none(), !commands.is_empty());
        commands
    }

    #[test]
    fn a_window_at_eguis_floor_gets_a_usable_size() {
        assert_eq!(repaired(egui::vec2(64.0, 64.0), Some(false)), [egui::ViewportCommand::InnerSize(USABLE)]);
        assert!(repaired(egui::vec2(820.0, 520.0), Some(false)).is_empty(), "a small window someone chose stays");
        assert!(repaired(egui::vec2(64.0, 64.0), Some(true)).is_empty(), "not while minimized");
    }
    #[cfg(target_os = "windows")]
    #[test]
    fn normal_geometry_survives_a_maximized_restart_and_restore() {
        fn pass_at(state: &mut WindowState, size: egui::Vec2, maximized: bool, position: egui::Pos2) -> Vec<egui::ViewportCommand> {
            let ctx = egui::Context::default();
            let mut raw = egui::RawInput::default();
            let info = raw.viewports.entry(egui::ViewportId::ROOT).or_default();
            info.outer_rect = Some(egui::Rect::from_min_size(position, size));
            info.inner_rect = info.outer_rect;
            info.maximized = Some(maximized);
            info.minimized = Some(false);
            ctx.run_logic(&raw, |ctx| state.repair(ctx, true)).viewport_commands.remove(&egui::ViewportId::ROOT).unwrap_or_default()
        }
        fn pass(state: &mut WindowState, size: egui::Vec2, maximized: bool) -> Vec<egui::ViewportCommand> {
            pass_at(state, size, maximized, egui::pos2(100.0, 100.0))
        }
        let normal = egui::vec2(820.0, 520.0);
        let mut state = WindowState::default();
        pass(&mut state, normal, false);
        pass(&mut state, egui::vec2(5120.0, 1392.0), true);
        let mut store = Store::default();
        store.set_string(WINDOW_KEY, "maximized bounds".into());
        state.keep(&mut store, false);
        let mut restarted = WindowState::started_with(Some(&store));
        pass(&mut restarted, egui::vec2(5120.0, 1392.0), true);
        let commands = pass(&mut restarted, egui::vec2(5136.0, 1408.0), false);
        assert!(commands.contains(&egui::ViewportCommand::InnerSize(normal)), "restore uses the last normal size: {commands:?}");
        assert!(commands.contains(&egui::ViewportCommand::OuterPosition(egui::pos2(100.0, 100.0))));
        // A minimized save cannot overwrite the normal restore bounds either.
        restarted.keep(&mut store, true);
        assert_eq!(WindowState::started_with(Some(&store)).normal.unwrap().size, normal);
        pass(&mut restarted, egui::vec2(5120.0, 1392.0), true);
        let moved = egui::pos2(200.0, 200.0);
        assert!(pass_at(&mut restarted, normal, false, moved).is_empty(), "a native drag-restore keeps its valid new position");
        assert_eq!(restarted.normal.unwrap().position, moved);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normal_bounds_fit_the_work_area_at_any_origin_and_scale() {
        for scale in [1.0, 1.25, 1.5, 2.0] {
            for origin in [egui::pos2(0.0, 48.0), egui::pos2(-1920.0, -1080.0), egui::pos2(1920.0, 0.0)] {
                let work = egui::Rect::from_min_size(origin, egui::vec2(1920.0, 1032.0) * scale);
                let old = NormalBounds { position: work.min - egui::vec2(8.0, 8.0) * scale, size: work.size() + egui::vec2(16.0, 16.0) * scale };
                let fixed = old.fit(work);
                assert!(work.contains_rect(egui::Rect::from_min_size(fixed.position, fixed.size)));
                assert_eq!(fixed.position, work.min, "header stays below the work area's top edge");
                let visible = NormalBounds { position: work.min + egui::vec2(30.0, 40.0), size: egui::vec2(800.0, 500.0) };
                assert_eq!(visible.fit(work).position, visible.position, "valid negative monitor coordinates stay put");
            }
        }
        for invalid in [
            NormalBounds { position: egui::pos2(f32::NAN, 0.0), size: egui::vec2(800.0, 600.0) },
            NormalBounds { position: egui::Pos2::ZERO, size: egui::vec2(f32::INFINITY, 600.0) },
        ] {
            assert!(!invalid.valid());
        }
        let mut store = Store::default();
        store.set_string("pdfcraft-normal-window", "not a rectangle".into());
        assert!(WindowState::started_with(Some(&store)).normal.is_none());
    }
}
