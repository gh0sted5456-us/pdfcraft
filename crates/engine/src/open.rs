//! Opening a document: preparing it away from the session, then registering it.

use super::*;
use std::sync::{Arc, OnceLock};

use pdfcraft_render::{DocInfo, OpenError, RenderConfig, RenderPool};

use crate::history::{Editor, Keys, TextBlocksCache};
use crate::{
    AppearanceCache, DocId, Document, Session, TrustStore, XfaLayout, display_bytes, guard, info_fields, render_threads, signatures_of, xfa,
};

/// Immutable session state needed while a document is prepared away from its owning thread.
#[derive(Clone)]
pub struct OpenContext {
    pub(super) now: Option<i64>,
    pub(super) js_off: bool,
    pub(super) trust: Arc<TrustStore>,
    pub(super) xfa_time_scale: u32,
    /// The session's identities, for a certificate-protected file.
    pub(super) identities: Vec<Arc<pdfcraft_sign::DigitalId>>,
}

/// A fully prepared document that has not yet been registered in a session.
pub struct PreparedDocument {
    pub(super) document: Document,
}

impl OpenContext {
    /// Prepare a document without mutating the session that supplied this context.
    pub fn prepare_open(
        &self,
        name: impl Into<String>,
        path: Option<String>,
        bytes: Arc<Vec<u8>>,
        password: Option<&str>,
    ) -> Result<PreparedDocument, OpenError> {
        let name = name.into();
        guard(|| Session::prepare_open_unguarded(self, name, path, bytes, password))
            .unwrap_or_else(|m| Err(OpenError::Invalid(format!("reading it failed unexpectedly ({m})"))))
    }
}

impl Session {
    /// Snapshot the session state that affects open-time scripts, dates and signature trust.
    pub fn open_context(&self) -> OpenContext {
        OpenContext {
            now: self.now(),
            js_off: self.js_off,
            trust: self.trust.clone(),
            xfa_time_scale: self.xfa_time_scale.unwrap_or(1),
            identities: self.identities.clone(),
        }
    }

    /// Prepare a document without adding it to this session.
    pub fn prepare_open(
        &self,
        name: impl Into<String>,
        path: Option<String>,
        bytes: Arc<Vec<u8>>,
        password: Option<&str>,
    ) -> Result<PreparedDocument, OpenError> {
        self.open_context().prepare_open(name, path, bytes, password)
    }

    /// Register a prepared document. This is the only open step that mutates the session.
    pub fn commit_open(&mut self, prepared: PreparedDocument) -> DocId {
        let id = DocId(self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1);
        let mut document = prepared.document;
        document.id = id;
        self.docs.push(document);
        if let Err(error) = self.run_document_event(id, "Open")
            && let Some(d) = self.docs.iter_mut().find(|d| d.id == id)
        {
            d.js_output.errors.push(format!("The document's open scripts could not be applied: {error}"));
        }
        id
    }

    /// Synchronous convenience API used by the CLI and automation.
    pub fn open(&mut self, name: impl Into<String>, path: Option<String>, bytes: Arc<Vec<u8>>, password: Option<&str>) -> Result<DocId, OpenError> {
        let prepared = self.prepare_open(name, path, bytes, password)?;
        Ok(self.commit_open(prepared))
    }

    fn prepare_open_unguarded(
        context: &OpenContext,
        name: String,
        path: Option<String>,
        bytes: Arc<Vec<u8>>,
        password: Option<&str>,
    ) -> Result<PreparedDocument, OpenError> {
        let mut cos = std::panic::catch_unwind(|| pdfcraft_cos::Document::open_with_password(bytes.clone(), password));
        // Certificate security: one of the identities opens it; the renderer and inspector get
        // a decrypted copy, the editor keeps the file as it is (saves stay encrypted).
        let mut certificate = None;
        if let Ok(Err(pdfcraft_cos::CosError::NeedsCertificate(envelopes))) = &cos {
            let key = unlock_with_identities(&context.identities, envelopes).ok_or(OpenError::NeedsCertificate)?;
            let opened = pdfcraft_cos::Document::open_with_credential(
                bytes.clone(),
                &pdfcraft_cos::Credential::Certificate { seed: key.seed, permissions: key.permissions },
            )
            .map_err(|e| OpenError::Invalid(e.to_string()))?;
            let plain = plain_copy(&opened).map_err(|e| OpenError::Invalid(e.to_string()))?;
            certificate = Some(key);
            cos = Ok(Ok(opened));
            return Self::prepare_from(context, name, path, plain, password, cos, certificate);
        }
        Self::prepare_from(context, name, path, bytes, password, cos, certificate)
    }

    /// The last step of opening: build the document record and register it.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build_document(
        trust: &Arc<TrustStore>,
        name: String,
        path: Option<String>,
        bytes: Arc<Vec<u8>>,
        mut info: DocInfo,
        cos: Result<Result<pdfcraft_cos::Document, pdfcraft_cos::CosError>, Box<dyn std::any::Any + Send>>,
        render_password: Option<String>,
        password: Option<&str>,
        certificate: Option<CertificateKey>,
        xfa: Option<XfaLayout>,
    ) -> Document {
        let config = RenderConfig { password: render_password.as_deref().map(Arc::from), ..Default::default() };
        let (mut editor, read_only_reason) = match cos {
            Ok(Ok(cos)) => {
                let keys = Keys { render: render_password.clone(), reopen: password.map(str::to_owned), certificate };
                let saved = cos.bytes().clone();
                (
                    Some(Editor {
                        cos,
                        saved,
                        saved_keys: keys.clone(),
                        unsaved_file: false,
                        undo: Vec::new(),
                        redo: Vec::new(),
                        position: 0,
                        keys,
                        appearances: AppearanceCache::default(),
                        field_appearances: pdfcraft_forms::FieldAppearanceCache::default(),
                        text_blocks: TextBlocksCache::default(),
                    }),
                    None,
                )
            }
            Ok(Err(e)) => (None, Some(e.to_string())),
            Err(_) => (None, Some("the document structure could not be read for editing".into())),
        };
        let sig_cache = Arc::new(pdfcraft_sign::DigestCache::default());
        // Signatures cover the file itself (`bytes` may be a decrypted copy of it).
        let signatures = editor.as_ref().map(|e| signatures_of(&e.cos, e.cos.bytes(), trust, &sig_cache)).unwrap_or_default();
        // The inspection reads only `/Info`; a file stating its title or author only in XMP still
        // shows them (#2373).
        if let Some(e) = &editor {
            for (field, value) in info_fields(&mut info, &e.cos) {
                if field.is_none() {
                    *field = value;
                }
            }
        }
        let display = display_bytes(editor.as_mut(), &bytes);
        let renderer = RenderPool::new(display.clone(), render_threads(), config.clone());
        let (form, adopted) = editor
            .as_ref()
            .map(|e| {
                let mut fields = pdfcraft_forms::fields_with_adopted(&e.cos);
                xfa::mark_script_buttons(&e.cos, &mut fields.items);
                (fields.items, fields.adopted_page_fields)
            })
            .unwrap_or_default();
        // Fields the file lists only as page widgets are adopted leniently; the file says so.
        if adopted > 0
            && let Some(e) = &mut editor
        {
            let line = if adopted == 1 {
                "the form's /Fields list omits 1 field; it was read from the page annotations".to_string()
            } else {
                format!("the form's /Fields list omits {adopted} fields; they were read from the page annotations")
            };
            e.cos.note_repair(line);
        }
        Document {
            id: DocId(0),
            name,
            path,
            bytes,
            display,
            info,
            renderer,
            password: render_password,
            dirty: false,
            generation: 0,
            snapshot_generation: 0,
            read_only_reason,
            form: Arc::new(form),
            marker_scan: OnceLock::new(),
            links: OnceLock::new(),
            signatures: Arc::new(signatures),
            trust: trust.clone(),
            sig_cache,
            editor,
            config,
            js_output: Default::default(),
            js_runtime: None,
            xfa,
            xfa_template: None,
            xfa_scripts_off: false,
            fonts_stale: false,
            xfa_warnings: Vec::new(),
            edit_notices: Vec::new(),
        }
    }
}
