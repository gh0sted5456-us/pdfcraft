//! Optional, local Glass wallpaper. Never becomes part of a PDF.

use base64::Engine as _;
use pdfcraft_engine::SignatureImage;

fn texture_image(image: &SignatureImage, limit: usize) -> Option<egui::ColorImage> {
    let [width, height] = image.size();
    let limit = limit.clamp(1, 4096) as u32;
    if width.max(height) <= limit as usize {
        return Some(egui::ColorImage::from_rgba_unmultiplied(image.size(), image.rgba()));
    }
    let rgba = image::RgbaImage::from_raw(width as u32, height as u32, image.rgba().to_vec())?;
    let small = image::DynamicImage::ImageRgba8(rgba).thumbnail(limit, limit).into_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw()))
}

#[derive(Clone)]
pub struct GlassBackground {
    image: Option<SignatureImage>,
    texture: Option<egui::TextureHandle>,
    pub opacity: u8,
}

impl Default for GlassBackground {
    fn default() -> Self {
        Self { image: None, texture: None, opacity: 15 }
    }
}

impl GlassBackground {
    pub fn has_image(&self) -> bool {
        self.image.is_some()
    }

    pub fn clear(&mut self) {
        self.image = None;
        self.texture = None;
    }

    /// The decoder bounds file size, dimensions and allocation, and normalizes orientation.
    pub fn set_image(&mut self, image: SignatureImage) {
        self.image = Some(image);
        self.texture = None;
    }

    pub fn set_encoded(&mut self, encoded: &str) -> Result<(), String> {
        if encoded.is_empty() {
            self.clear();
            return Ok(());
        }
        let limit = pdfcraft_engine::signature_image::MAX_SIGNATURE_IMAGE_BYTES;
        if encoded.len() > limit.div_ceil(3) * 4 {
            return Err("Choose a PNG or JPEG image smaller than 4 MiB.".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| "Invalid background image encoding")?;
        let image = SignatureImage::from_bytes(&bytes).map_err(|e| e.to_string())?;
        self.set_image(image);
        Ok(())
    }

    pub fn persist(&self) -> serde_json::Value {
        serde_json::json!({
            "opacity": self.opacity.min(100),
            "png": self.image.as_ref().map(|image| base64::engine::general_purpose::STANDARD.encode(image.bytes().as_slice())),
        })
    }

    pub fn restore(&mut self, value: &serde_json::Value) {
        *self = Self::default();
        if let Some(opacity) = value["opacity"].as_u64().filter(|v| *v <= 100) {
            self.opacity = opacity as u8;
        }
        if let Some(encoded) = value["png"].as_str()
            && let Err(e) = self.set_encoded(encoded)
        {
            log::warn!("Glass background ignored: {e}");
        }
    }

    pub fn paint(&mut self, ui: &egui::Ui) {
        let Some(image) = &self.image else { return };
        if self.opacity == 0 {
            return;
        }
        if self.texture.is_none() {
            let Some(pixels) = texture_image(image, ui.input(|i| i.max_texture_side)) else { return };
            self.texture = Some(ui.ctx().load_texture("glass-background", pixels, egui::TextureOptions::LINEAR));
        }
        let Some(texture) = &self.texture else { return };
        let rect = ui.max_rect();
        if !rect.is_positive() || !rect.is_finite() {
            return;
        }
        let size = texture.size_vec2();
        // Cover the window without stretching, cropping equal amounts from opposite sides.
        let scale = (rect.width() / size.x).max(rect.height() / size.y);
        let visible = rect.size() / (size * scale);
        let uv = egui::Rect::from_center_size(egui::pos2(0.5, 0.5), visible);
        let alpha = (u16::from(self.opacity.min(100)) * 255 / 100) as u8;
        ui.painter().image(texture.id(), rect, uv, egui::Color32::from_white_alpha(alpha));
    }
}

impl crate::PdfCraftApp {
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn pick_glass_background(&mut self) {
        let dialog = rfd::AsyncFileDialog::new().set_title("Choose Glass background").add_filter("PNG or JPEG image", &["png", "jpg", "jpeg"]);
        self.ask_one(crate::pickers::Ask::File(dialog), None, |app, path| {
            let result =
                std::fs::File::open(path).map_err(pdfcraft_engine::signature_image::SignatureImageError::from).and_then(SignatureImage::read);
            match result {
                Ok(image) => app.glass_background.set_image(image),
                Err(e) => app.notify(e.to_string()),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image() -> SignatureImage {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 70, 110, 200])).write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        SignatureImage::from_bytes(bytes.get_ref()).unwrap()
    }

    #[test]
    fn background_texture_fits_small_graphics_devices() {
        assert_eq!(texture_image(&image(), 4).unwrap().size, [4, 2]);
        assert_eq!(texture_image(&image(), 0).unwrap().size, [1, 1]);
        assert_eq!(texture_image(&image(), usize::MAX).unwrap().size, [8, 4]);
    }

    #[test]
    fn wallpaper_paint_crops_without_distortion_and_uses_its_own_alpha() {
        let mut background = GlassBackground::default();
        background.set_image(image());
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(400.0, 400.0))
            .build_ui_state(|ui, background: &mut GlassBackground| background.paint(ui), background);
        h.run_steps(2);
        let mesh = h
            .output()
            .shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .unwrap();
        assert!(mesh.vertices.iter().all(|v| v.color.a() == 38));
        let uv_min = mesh.vertices.iter().map(|v| v.uv.x).fold(1.0, f32::min);
        let uv_max = mesh.vertices.iter().map(|v| v.uv.x).fold(0.0, f32::max);
        assert!((uv_min - 0.25).abs() < 0.02 && (uv_max - 0.75).abs() < 0.02);
        h.state_mut().opacity = 0;
        h.run_steps(2);
        assert!(!h.output().shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Mesh(_))));
    }

    #[test]
    fn app_options_validate_and_persist_the_background() {
        let mut app = crate::PdfCraftApp::new();
        let png = base64::engine::general_purpose::STANDARD.encode(image().bytes().as_slice());
        app.set_option("glass-background", &png).unwrap();
        app.set_option("glass-background-opacity", "27").unwrap();
        assert!(app.set_option("glass-background-opacity", "101").is_err());
        assert!(app.set_option("glass-background-opacity", "-1").is_err());
        let mut restored = crate::PdfCraftApp::new();
        restored.restore(&app.persist());
        assert_eq!(restored.glass_background.persist(), app.glass_background.persist());
        assert!(restored.views.is_empty());
        restored.set_option("glass-background", "").unwrap();
        assert!(!restored.glass_background.has_image());
    }

    #[test]
    fn wallpaper_round_trips_and_invalid_replacement_preserves_current_image() {
        let mut background = GlassBackground::default();
        background.set_image(image());
        background.opacity = 23;
        let value = background.persist();
        let mut restored = GlassBackground::default();
        restored.restore(&value);
        assert_eq!(restored.persist(), value);
        for invalid in ["not base64", "aGVsbG8="] {
            assert!(restored.set_encoded(invalid).is_err());
            assert_eq!(restored.persist(), value);
        }
        restored.set_encoded("").unwrap();
        assert!(!restored.has_image());
        restored.restore(&serde_json::json!({"opacity": 101, "png": "invalid"}));
        assert_eq!(restored.opacity, 15);
        assert!(!restored.has_image());
    }
}
