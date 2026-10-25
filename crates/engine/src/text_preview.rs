//! The page beneath an inline text editor, with the source glyphs removed on a COS clone.

use std::sync::Arc;

use pdfcraft_render::{PageRenderer, RenderConfig, RenderPool, RenderRequest, RenderedPage};

use crate::{Document, guard};

pub struct TextEditPreview {
    page: usize,
    bytes: Arc<Vec<u8>>,
    config: RenderConfig,
    bounds: Option<[f64; 4]>,
}

impl TextEditPreview {
    /// Original display-space bounds when an isolated image uses its own preview canvas.
    pub fn content_bounds(&self) -> Option<[f64; 4]> {
        self.bounds
    }

    pub fn renderer(&self) -> RenderPool {
        RenderPool::new(self.bytes.clone(), 1, self.config.clone())
    }

    pub fn render_background(&self, scale: f32) -> Result<RenderedPage, String> {
        if !scale.is_finite() || scale <= 0.0 {
            return Err("preview scale must be finite and positive".into());
        }
        guard(|| {
            let mut renderer = PageRenderer::new(self.bytes.clone(), self.config.clone());
            let out = renderer.render(RenderRequest { page: self.page, scale, ..Default::default() });
            if let Some(e) = &out.error {
                return Err(e.clone());
            }
            Ok(out)
        })?
    }
}

impl Document {
    /// Background for retyping an added text item, without changing its saved content.
    pub fn added_text_preview(&self, page: usize, index: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            if !self.added().iter().filter(|a| a.page == page).nth(index).is_some_and(|a| matches!(a.content, crate::AddedContent::Text(_))) {
                return Err("the selected added item is not text".into());
            }
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::delete_content(&mut cos, page, index).map_err(|e| e.to_string())?;
            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: self.config.clone(), bounds: None })
        })?
    }

    /// Measure a draft without changing bytes, dirty state, or undo history.
    pub fn text_edit_layout(
        &self,
        page: usize,
        block: usize,
        text: &str,
        style: &pdfcraft_edit::BlockStyle,
    ) -> Result<pdfcraft_edit::text::TextEditLayout, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            pdfcraft_edit::text::text_edit_layout(&editor.cos, page, block, text, style).map_err(|e| e.to_string())
        })?
    }

    /// Embedded source faces for a text editor, bounded and validated as sfnt programs.
    /// This is read-only; no source font is installed or exported.
    pub fn text_editor_fonts(&self, page: usize) -> Vec<pdfcraft_edit::text::TextEditorFont> {
        self.editor.as_ref().and_then(|e| pdfcraft_edit::text::text_editor_fonts(&e.cos, page).ok()).unwrap_or_default()
    }

    /// Preparing the background never changes working bytes, dirty state or undo history.
    pub fn text_edit_preview(&self, page: usize, block: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::clear_block(&mut cos, page, block).map_err(|e| e.to_string())?;
            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: self.config.clone(), bounds: None })
        })?
    }

    /// The page beneath an added item while it is dragged: `index` counts the page's added
    /// items, as `Edit::DeleteContent` does. Read-only, like [`Self::text_edit_preview`].
    pub fn added_content_preview(&self, page: usize, index: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::delete_content(&mut cos, page, index).map_err(|e| e.to_string())?;
            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: self.config.clone(), bounds: None })
        })?
    }

    /// An added item alone, on a transparent page: what follows the pointer while it is dragged,
    /// so the page under its first place doesn't come along. Read-only, like
    /// [`Self::added_content_preview`].
    pub fn added_content_alone(&self, page: usize, index: usize) -> Result<TextEditPreview, String> {
        guard(|| {
            let editor = self.editor.as_ref().ok_or("the document can't be read")?;
            let mut cos = editor.cos.clone();
            pdfcraft_edit::isolate_content(&mut cos, page, index).map_err(|e| e.to_string())?;
            // The page clips away off-page pixels; an image's own canvas keeps them for dragging back.
            let bounds = self.added().iter().filter(|a| a.page == page).nth(index).and_then(|a| match &a.content {
                crate::AddedContent::Image(image) => Some(image.rect),
                _ => None,
            });
            let bounds = if let Some([left, bottom, right, top]) = bounds {
                let bounds = [left - 1.0, bottom - 1.0, right + 1.0, top + 1.0];
                if !bounds.iter().all(|v| v.is_finite()) || right <= left || top <= bottom {
                    return Err("the image preview has invalid bounds".into());
                }
                let info = self.info.pages.get(page).ok_or("the preview page does not exist")?;
                let [left, bottom, right, top] = bounds.map(|v| v as f32);
                let corners = [(left, bottom), (right, bottom), (left, top), (right, top)].map(|(x, y)| info.view_to_user(x, info.height - y));
                if !corners.iter().flatten().all(|v| v.is_finite()) {
                    return Err("the image preview bounds exceed the supported range".into());
                }
                let rect = [
                    corners.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min),
                    corners.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min),
                    corners.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max),
                    corners.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max),
                ];
                let p = pdfcraft_model::pages(&cos).get(page).cloned().ok_or("the preview page does not exist")?;
                cos.update_dict(p.obj, |d| {
                    let box_value = pdfcraft_cos::Object::Array(rect.into_iter().map(|v| pdfcraft_cos::Object::Real(f64::from(v))).collect());
                    d.set(b"MediaBox".to_vec(), box_value.clone());
                    d.set(b"CropBox".to_vec(), box_value);
                })
                .map_err(|e| e.to_string())?;
                Some(bounds)
            } else {
                None
            };

            let bytes = Arc::new(pdfcraft_cos::write_incremental(&cos, &pdfcraft_cos::SaveOptions::default()).map_err(|e| e.to_string())?);
            Ok(TextEditPreview { page, bytes, config: RenderConfig { transparent: true, ..self.config.clone() }, bounds })
        })?
    }
}
