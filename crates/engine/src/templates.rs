//! Portable, versioned PDF templates. The file is a PDF with a catalog metadata dictionary.
use crate::{DocId, EditError, Session, guard};
use pdfcraft_cos::{Dict, Object, PdfString};
use std::sync::Arc;

pub use pdfcraft_create::templates::{STARTERS, Starter, dimensions, find};
pub const EXTENSION: &str = "pdfcraft-template";
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct TemplateInfo {
    pub title: String,
    pub description: String,
    pub category: String,
}

impl TemplateInfo {
    fn validate(&self) -> Result<(), EditError> {
        if self.title.trim().is_empty()
            || self.title.chars().count() > 120
            || self.description.chars().count() > 1000
            || self.category.chars().count() > 64
            || self.title.chars().chain(self.category.chars()).any(char::is_control)
        {
            return Err(EditError::Invalid(
                "Template title is required (at most 120 characters); description is limited to 1000 and category to 64 characters".into(),
            ));
        }
        Ok(())
    }
}

fn size(bytes: &[u8]) -> Result<(), EditError> {
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        Err(EditError::Invalid("A template must be a nonempty PDF no larger than 64 MiB".into()))
    } else {
        Ok(())
    }
}

impl Session {
    /// Create an editable starter, including real AcroForm widgets where the layout calls for them.
    pub fn create_starter(&self, id: &str, paper: &str) -> Result<Arc<Vec<u8>>, EditError> {
        guard(|| {
            let mut doc = pdfcraft_create::templates::create(id, paper)?;
            for field in pdfcraft_create::templates::fields(id, paper)? {
                let kind =
                    if field.checkbox { pdfcraft_forms::NewField::CheckBox } else { pdfcraft_forms::NewField::Text { multiline: field.multiline } };
                pdfcraft_forms::add_field(&mut doc, 0, field.rect, &kind, Some(&field.name))?;
            }
            self.write_new(&doc)
        })
        .map_err(|_| EditError::Source("The starter template could not be created".into()))?
    }

    /// Snapshot the current contents as a reusable template; never save or modify the source tab.
    pub fn export_template(&self, id: DocId, info: &TemplateInfo) -> Result<Arc<Vec<u8>>, EditError> {
        info.validate()?;
        let source = self.get(id).ok_or(EditError::NoDocument)?;
        if source.is_signed() || source.security_summary().is_some() {
            return Err(EditError::Invalid("Remove signatures and password security before exporting a reusable template".into()));
        }
        if !source.allows_copying() {
            return Err(EditError::Invalid("The document does not permit copying".into()));
        }
        size(&source.bytes)?;
        guard(|| {
            let mut doc = self.cos(id)?.clone();
            let root = doc.root().ok_or_else(|| EditError::Invalid("The document has no catalog".into()))?;
            let mut meta = Dict::new();
            meta.set(b"Version".to_vec(), Object::Int(1));
            meta.set(b"Title".to_vec(), PdfString::text(&info.title));
            meta.set(b"Description".to_vec(), PdfString::text(&info.description));
            meta.set(b"Category".to_vec(), PdfString::text(&info.category));
            doc.update_dict(root, |d| d.set(b"PdfCraftTemplate".to_vec(), Object::Dict(meta))).map_err(|e| EditError::Write(e.to_string()))?;
            let bytes = self.write_new(&doc)?;
            size(&bytes)?;
            Ok(bytes)
        })
        .map_err(|_| EditError::Source("The document could not be exported as a template".into()))?
    }

    /// Validate a template and return a fresh working copy with the template marker removed.
    pub fn import_template(&self, bytes: Arc<Vec<u8>>) -> Result<(TemplateInfo, Arc<Vec<u8>>), EditError> {
        size(&bytes)?;
        guard(|| {
            let mut doc = crate::open_source("Template", &bytes)?;
            if doc.trailer().get(b"Encrypt").is_some() || pdfcraft_sign::signatures(&doc, &bytes, &Default::default()).iter().any(|s| s.signed) {
                return Err(EditError::Invalid("Reusable templates must not contain digital signatures or password security".into()));
            }
            let root = doc.root().ok_or_else(|| EditError::Invalid("The template has no catalog".into()))?;
            let catalog = doc.get(root);
            let meta = catalog
                .as_dict()
                .and_then(|d| d.get(b"PdfCraftTemplate"))
                .and_then(|v| doc.resolve(v).as_dict().cloned())
                .ok_or_else(|| EditError::Invalid("This PDF is not a PdfCraft template".into()))?;
            if meta.int(b"Version") != Some(1) {
                return Err(EditError::Invalid("Unsupported PdfCraft template version; this build supports version 1".into()));
            }
            let read = |key: &[u8]| {
                meta.get(key)
                    .and_then(|v| doc.resolve(v).as_string().map(|s| s.to_text()))
                    .ok_or_else(|| EditError::Invalid("The template metadata is incomplete".into()))
            };
            let info = TemplateInfo { title: read(b"Title")?, description: read(b"Description")?, category: read(b"Category")? };
            info.validate()?;
            doc.update_dict(root, |d| {
                d.remove(b"PdfCraftTemplate");
            })
            .map_err(|e| EditError::Write(e.to_string()))?;
            let working = self.write_new(&doc)?;
            size(&working)?;
            Ok((info, working))
        })
        .map_err(|_| EditError::Source("The template could not be read".into()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn templates_preserve_editable_text_fields_and_the_source_document() {
        let mut session = Session::new();
        let bytes = session.create_starter("intake-form", "letter").unwrap();
        let id = session.open_new("Intake.pdf", bytes).unwrap();
        let original = session.get(id).unwrap().bytes.clone();
        let info = TemplateInfo { title: "My intake".into(), description: "Reusable details".into(), category: "Forms".into() };
        let exported = session.export_template(id, &info).unwrap();
        assert_eq!(session.get(id).unwrap().bytes, original);
        let (loaded, working) = session.import_template(exported).unwrap();
        assert_eq!(loaded, info);
        let next = session.open_new("Copy.pdf", working).unwrap();
        assert_eq!(session.get(next).unwrap().form.len(), 5);
        assert!(session.get(next).unwrap().dirty);
        let text = crate::export::Exporter::new(session.get(next).unwrap()).text(0).unwrap();
        assert!(text.contains("Client intake"));
        assert!(session.import_template(original).is_err());
        session.apply(id, crate::Edit::SetPageLocks { pages: vec![0], locked: true }).unwrap();
        let locked = session.export_template(id, &info).unwrap();
        let (_, working) = session.import_template(locked).unwrap();
        let locked_id = session.open_new("Locked copy.pdf", working).unwrap();
        assert_eq!(session.get(locked_id).unwrap().protected_pages(), [0]);
        assert!(session.create_starter("not-a-template", "letter").is_err());
        assert!(session.import_template(Arc::new(b"not a PDF".to_vec())).is_err());
    }
    #[test]
    fn all_starters_reopen_with_the_expected_pages_and_live_fields() {
        let mut session = Session::new();
        for starter in STARTERS {
            let bytes = session.create_starter(starter.id, "a4").unwrap();
            let id = session.open_new(starter.title, bytes).unwrap();
            let doc = session.get(id).unwrap();
            assert_eq!(doc.info.pages.len(), starter.pages);
            assert_eq!(doc.form.len(), pdfcraft_create::templates::fields(starter.id, "a4").unwrap().len());
        }
    }

    #[test]
    fn template_versions_metadata_and_signed_sources_are_checked() {
        let mut session = Session::new();
        let source = session.create_starter("letter", "letter").unwrap();
        let id = session.open_new("Letter.pdf", source).unwrap();
        let info = TemplateInfo { title: "Letter".into(), description: String::new(), category: "Business".into() };
        let package = session.export_template(id, &info).unwrap();
        for version in [0, 1, 2] {
            let mut malformed = pdfcraft_cos::Document::open(package.clone()).unwrap();
            let root = malformed.root().unwrap();
            let mut metadata = Dict::new();
            metadata.set(b"Version".to_vec(), Object::Int(version));
            malformed.update_dict(root, |d| d.set(b"PdfCraftTemplate".to_vec(), Object::Dict(metadata))).unwrap();
            let bad = session.write_new(&malformed).unwrap();
            assert!(session.import_template(bad).is_err(), "unsupported or incomplete metadata must be refused");
        }
        let invalid = TemplateInfo { title: " ".into(), ..info.clone() };
        assert!(session.export_template(id, &invalid).is_err());
        let identity = pdfcraft_sign::pkcs12::open(include_bytes!("../../sign/tests/data/ec-p256.p12"), "test").unwrap();
        let signed = session.sign(id, &identity, crate::SignOptions::default()).unwrap();
        let signed_id = session.open("Signed.pdf", None, signed, None).unwrap();
        assert!(session.export_template(signed_id, &info).is_err());
        session.apply(id, crate::Edit::Protect(crate::Protection { permissions_password: Some("owner".into()), ..Default::default() })).unwrap();
        assert!(session.export_template(id, &info).is_err());
    }

    #[test]
    fn template_labels_do_not_overlap_fillable_fields() {
        let mut session = Session::new();
        for starter in STARTERS {
            for paper in ["letter", "a4"] {
                let fields = pdfcraft_create::templates::fields(starter.id, paper).unwrap();
                if fields.is_empty() {
                    continue;
                }
                let bytes = session.create_starter(starter.id, paper).unwrap();
                let id = session.open_new(starter.title, bytes).unwrap();
                let doc = session.get(id).unwrap();
                let mut renderer = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), Default::default());
                let result =
                    renderer.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Text, ..Default::default() });
                let text = result.text.unwrap();
                let page = &doc.info.pages[0];
                for field in fields {
                    let [x0, y0] = page.user_to_view(field.rect[0] as f32, field.rect[1] as f32);
                    let [x1, y1] = page.user_to_view(field.rect[2] as f32, field.rect[3] as f32);
                    for glyph in &text.glyphs {
                        let overlap_x = glyph.rect[2].min(x0.max(x1)) - glyph.rect[0].max(x0.min(x1));
                        let overlap_y = glyph.rect[3].min(y0.max(y1)) - glyph.rect[1].max(y0.min(y1));
                        assert!(overlap_x <= 0.5 || overlap_y <= 0.5, "{} / {paper}: a label overlaps {}", starter.id, field.name);
                    }
                }
            }
        }
    }
}
