//! Safe Windows WebView2 child: isolated provider profile, HTTPS navigation, no document bridge.
use pdfcraft_platform::assistant::{self as platform, Provider};
use std::sync::mpsc::{self, Receiver};

pub(crate) struct Host {
    webview: wry::WebView,
    _context: wry::WebContext,
    provider: Provider,
    origin: String,
    navigation: Receiver<String>,
}
fn bounds(rect: egui::Rect, scale: f32) -> Result<wry::Rect, String> {
    let values = [rect.min.x, rect.min.y, rect.width(), rect.height(), scale];
    if values.iter().any(|v| !v.is_finite())
        || rect.min.x < 0.0
        || rect.min.y < 0.0
        || rect.width() < 1.0
        || rect.height() < 1.0
        || !(0.1..=16.0).contains(&scale)
        || values[..4].iter().any(|v| v * scale > 32768.0)
    {
        return Err("The embedded browser area is unavailable at this window size".into());
    }
    Ok(wry::Rect {
        position: wry::dpi::PhysicalPosition::new((rect.min.x * scale).round() as i32, (rect.min.y * scale).round() as i32).into(),
        size: wry::dpi::PhysicalSize::new((rect.width() * scale).round() as u32, (rect.height() * scale).round() as u32).into(),
    })
}
impl Host {
    pub(crate) fn new(window: &winit::window::Window, provider: Provider, rect: egui::Rect, scale: f32, ctx: &egui::Context) -> Result<Self, String> {
        let url = provider.browser_url().ok_or("This provider has no browser chat")?;
        let mut context = wry::WebContext::new(Some(platform::browser_data_dir(provider)?));
        let (tx, navigation) = mpsc::sync_channel(16);
        let repaint = ctx.clone();
        let popup_repaint = ctx.clone();
        let popup = tx.clone();
        let webview = wry::WebViewBuilder::new_with_web_context(&mut context)
            .with_url(url)
            .with_bounds(bounds(rect, scale)?)
            .with_focused(false)
            .with_devtools(false)
            .with_clipboard(false)
            .with_permission_handler(|_| wry::PermissionResponse::Deny)
            .with_download_started_handler(|_, _| false)
            .with_navigation_handler(move |address| {
                if let Some(origin) = platform::browser_origin(provider, &address) {
                    let _ = tx.try_send(origin);
                    repaint.request_repaint();
                    true
                } else {
                    let _ = tx.try_send("Navigation blocked. Use the external browser for other sites or organization sign-in.".into());
                    repaint.request_repaint();
                    false
                }
            })
            .with_new_window_req_handler(move |_, _| {
                let _ = popup.try_send("Popup blocked. Use the external browser if this sign-in requires another window.".into());
                popup_repaint.request_repaint();
                wry::NewWindowResponse::Deny
            })
            .build_as_child(window)
            .map_err(|_| "Windows WebView2 could not start. Check that the WebView2 Runtime is installed.")?;
        Ok(Self { webview, _context: context, provider, origin: url.into(), navigation })
    }
    pub(crate) fn provider(&self) -> Provider {
        self.provider
    }
    pub(crate) fn origin(&self) -> &str {
        &self.origin
    }
    pub(crate) fn show(&mut self, rect: egui::Rect, scale: f32) -> Result<(), String> {
        for origin in self.navigation.try_iter() {
            self.origin = origin;
        }
        self.webview.set_bounds(bounds(rect, scale)?).map_err(|_| "Couldn't resize the embedded browser")?;
        self.webview.set_visible(true).map_err(|_| "Couldn't show the embedded browser")?;
        Ok(())
    }
    pub(crate) fn hide(&self) {
        let _ = self.webview.set_visible(false);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_bounds_reject_invalid_or_unbounded_native_sizes() {
        let rect = egui::Rect::from_min_size(egui::pos2(20.0, 30.0), egui::vec2(500.0, 300.0));
        assert!(bounds(rect, 1.5).is_ok());
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY, 40.0] {
            assert!(bounds(rect, scale).is_err());
        }
        assert!(bounds(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1e9, 10.0)), 1.0).is_err());
    }
}
