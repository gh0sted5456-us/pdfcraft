use super::*;

impl PdfCraftApp {
    /// Use the integrated Windows header consistently across appearance modes.
    pub fn draws_window_frame(&self) -> bool {
        self.window_frame_layout().is_some()
    }

    /// Follow the desktop title-bar preference and the integrated Windows header.
    pub(super) fn sync_window_frame(&mut self, ctx: &egui::Context) {
        let app_frame = self.draws_window_frame();
        if self.window_controls.is_none() && !app_frame && self.app_frame_shown.is_none() {
            return;
        }
        if self.app_frame_shown != Some(app_frame) {
            // Linux app headers start undecorated; Windows switches from the native frame.
            if self.app_frame_shown.is_some() || !app_frame || self.window_controls.is_none() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!app_frame));
                #[cfg(target_os = "windows")]
                if let Some(size) = ctx.input(|i| {
                    let v = i.viewport();
                    if v.maximized == Some(true) || v.minimized == Some(true) || v.fullscreen == Some(true) {
                        None
                    } else {
                        v.inner_rect.filter(|r| r.is_positive() && r.is_finite()).map(|r| r.size())
                    }
                }) {
                    // Windows otherwise adds/removes frame margins from the client size on every start.
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
                }
            }
            self.app_frame_shown = Some(app_frame);
        }
    }

    /// Enter or leave full-screen reading (Acrobat: View ▸ Full Screen Mode, ⌘L).
    pub fn set_full_screen(&mut self, ctx: &egui::Context, on: bool) {
        self.apply_full_screen(on);
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
    }

    /// The full-screen state without the window: every document shows one page (or spread) at a
    /// time fitted to the screen while it is on (Acrobat's Full Screen Mode, #379, #1459), and
    /// gets its display back when it is off.
    pub fn apply_full_screen(&mut self, on: bool) {
        // A click turns the page only with the Select tool, and full screen hides the tools:
        // select it there, and give the tool back on leaving (review on #2070).
        if on && !self.full_screen {
            self.full_screen_tool = Some(self.quick_tool);
            self.quick_tool = QuickTool::Select;
        } else if !on && let Some(tool) = self.full_screen_tool.take() {
            self.quick_tool = tool;
        }
        self.full_screen = on;
        for view in &mut self.views {
            if on {
                view.enter_presentation();
            } else {
                view.leave_presentation();
            }
        }
    }

    /// Answer the Full Screen mode prompt a document raised on opening: enter it, or leave the
    /// window as it is.
    pub fn resolve_full_screen_prompt(&mut self, ctx: &egui::Context, allow: bool) {
        let Some(id) = self.pending_full_screen.take() else { return };
        if allow && self.session.get(id).is_some() {
            self.set_full_screen(ctx, true);
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::*;

    #[test]
    fn windows_frame_changes_preserve_normal_client_size() {
        for app_frame in [false, true] {
            for (maximized, minimized, fullscreen) in [(false, false, false), (true, false, false), (false, true, false), (false, false, true)] {
                let ctx = egui::Context::default();
                let mut app = PdfCraftApp::new();
                app.system_title_bar = !app_frame;
                app.app_frame_shown = Some(!app_frame);
                let size = egui::vec2(1000.0, 700.0);
                let mut raw = egui::RawInput::default();
                let info = raw.viewports.entry(egui::ViewportId::ROOT).or_default();
                info.inner_rect = Some(egui::Rect::from_min_size(egui::pos2(26.0, 27.0), size));
                info.maximized = Some(maximized);
                info.minimized = Some(minimized);
                info.fullscreen = Some(fullscreen);
                let commands =
                    ctx.run_logic(&raw, |ctx| app.sync_window_frame(ctx)).viewport_commands.remove(&egui::ViewportId::ROOT).unwrap_or_default();
                assert!(commands.contains(&egui::ViewportCommand::Decorations(!app_frame)));
                assert_eq!(commands.contains(&egui::ViewportCommand::InnerSize(size)), !maximized && !minimized && !fullscreen, "{commands:?}");
                assert!(ctx.run_logic(&raw, |ctx| app.sync_window_frame(ctx)).viewport_commands.values().all(Vec::is_empty));
            }
        }
    }
}
