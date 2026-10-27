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
