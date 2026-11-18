//! The session: the open documents and the operations on them.

use std::sync::{Arc, OnceLock};

use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
use pdfcraft_render::{LayerOp, OpenError, ReadingColors, RenderConfig, RenderPool, inspect};

#[cfg(test)]
use crate::check_refresh_panic;
use crate::edit::{EditCtx, check_permission, run_edit};
use crate::history::{Keys, MAX_UNDO, Scope, keys_after, scope_of, uses_scripts};
use crate::{
    BookmarkPage, BookmarkRow, CombineSource, Combined, DataFormat, DocId, Document, Edit, EditError, Hairlines, HairlinesFixed, Hidden,
    ImageResolution, LOW_RESOLUTION_PRINT_DPI, Label, ObjectKind, OptimizeReport, RecoverySnapshot, SignOptions, SourceKind, SplitPart,
    StatusCaptions, SummarySort, TrustStore, XFA_REMOVED_PREFIX, XFA_SCRIPTS_OFF, apply_layer_state, carry_verdict_marks, comment_list,
    comment_summary, decode_text, display_bytes, display_config, guard, info_fields, js, local_utc_offset, note_warnings, office_to_pdf, open_source,
    open_source_with, optimize, outline_rows, parse_ymd, print, print_layer_states, print_raster, render_threads, scan_colours,
    set_default_layer_state, shown_under_redactions, signatures_of, source_kind, sync_xfa_state, undrawn_notices, use_layer_choices, writable, xfa,
    xfa_sync_datasets,
};

mod editing;
mod files;
mod save;
mod view;

#[derive(Default)]
pub struct Session {
    pub(crate) docs: Vec<Document>,
    // Detached sibling windows share IDs; unrelated CLI sessions still start at one.
    pub(crate) next_id: Arc<std::sync::atomic::AtomicU64>,
    /// Seconds since the Unix epoch, injected so saves are deterministic in tests.
    clock: Option<fn() -> i64>,
    /// Seconds to add to UTC for local time, read from whoever knows the zone (a browser, say).
    /// Separate from `clock` on purpose: `clock` alone is the test shortcut that takes the
    /// injected time AS local time and zeroes the zone, so injecting only it leaves either the
    /// local dates or the UTC ones wrong by the zone's offset.
    utc_offset: Option<fn() -> i64>,
    /// Certificates trusted for signing (Acrobat: Trusted Certificates).
    pub(crate) trust: Arc<TrustStore>,
    /// The words signature status layers are drawn with, in the interface language.
    pub(crate) captions: Arc<StatusCaptions>,
    /// Preferences ▸ JavaScript ▸ Enable Acrobat JavaScript, inverted (on by default).
    pub(crate) js_off: bool,
    /// Preferences ▸ Date format, when not the default (see [`dates`]).
    pub(crate) date_format: Option<String>,
    /// Preferences ▸ Date format ▸ Language, when not following the interface language.
    pub(crate) date_language: Option<String>,
    /// Highlight fields, as the viewer shows it: what forms' scripts read as
    /// `app.runtimeHighlight` (off by default, as in a session without a viewer).
    pub(crate) field_highlight: bool,
    /// What the XFA scripts' time limits are multiplied by (`None`: 1); see
    /// [`Session::with_xfa_time_scale`].
    pub(crate) xfa_time_scale: Option<u32>,
    /// The person's digital identities, tried on a certificate-protected document's envelopes.
    pub(crate) identities: Vec<Arc<pdfcraft_sign::DigitalId>>,
}

impl Session {
    /// Convert a vector/text XPS print intermediate with PdfCraft's own writer. Unsupported
    /// content is refused; there is no page rasterization or Microsoft PDF driver dependency.
    pub fn create_from_xps(&self, title: &str, bytes: &[u8]) -> Result<Arc<Vec<u8>>, EditError> {
        guard(|| self.write_new(&pdfcraft_create::from_xps(title, bytes)?))
            .map_err(|_| EditError::Source("the XPS print intermediate could not be converted".into()))?
    }
}

impl Session {
    /// The identities that open certificate-protected documents (replacing any set before).
    pub fn set_identities(&mut self, identities: Vec<pdfcraft_sign::DigitalId>) {
        self.identities = identities.into_iter().map(Arc::new).collect();
    }
}

impl Session {
    /// Add one identity (replacing one with the same certificate).
    pub fn add_identity(&mut self, id: pdfcraft_sign::DigitalId) {
        self.identities.retain(|i| i.certificate.raw != id.certificate.raw);
        self.identities.push(Arc::new(id));
    }
}

impl Session {
    pub fn clear_identities(&mut self) {
        self.identities.clear();
    }
}

impl Session {
    pub fn identities(&self) -> &[Arc<pdfcraft_sign::DigitalId>] {
        &self.identities
    }
}

impl Session {
    /// A fresh seed for [`CertificateProtection`], from the operating system's random
    /// generator. It is the file key's only secret, so there is no weaker fallback: without
    /// randomness nothing is encrypted.
    pub fn new_seed(&self) -> Result<[u8; 20], EditError> {
        pdfcraft_sign::envelope::new_seed().map_err(|e| EditError::Protection(format!("no file key: {e}")))
    }
}

impl Session {
    /// Set authorities trusted only for RFC 3161 timestamps; ordinary signer trust is unchanged.
    pub fn set_timestamp_certificates(&mut self, certs: Vec<pdfcraft_sign::Certificate>) {
        self.trust = Arc::new(TrustStore { timestamp_certs: certs, ..(*self.trust).clone() });
        self.refresh_signature_trust();
    }
}

impl Session {
    fn refresh_signature_trust(&mut self) {
        for doc in &mut self.docs {
            Self::revalidate(doc, self.trust.clone());
        }
    }
}

impl Session {
    /// The signatures of document `id` that Remove Signature lists, with what removing each would
    /// do and why some can't be removed ([`clear_signature::removable`]).
    pub fn removable_signatures(&self, id: DocId) -> Result<Vec<clear_signature::Removable>, EditError> {
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let editor = doc.editor.as_ref().ok_or_else(|| EditError::ReadOnly(doc.read_only_reason.clone().unwrap_or_default()))?;
        let mut list = guard(|| clear_signature::removable(&editor.cos, &doc.signatures)).map_err(EditError::Sign)?;
        for r in list.iter_mut().filter(|r| r.blocked.is_none()) {
            // What applying the edit would check (`check_permission`): a visual signature is a
            // comment, a digital one changes the document.
            let why = match (&doc.read_only_reason, &r.target) {
                (Some(reason), _) => Some(reason.clone()),
                (None, clear_signature::Target::Annotation { .. }) if !doc.allows_annotation() => {
                    Some("its security doesn't allow changing comments".into())
                }
                (None, clear_signature::Target::Annotation { .. }) => None,
                (None, _) if doc.permissions().is_some_and(|p| !p.modify()) => Some("its security doesn't allow changing it".into()),
                (None, _) => None,
            };
            r.blocked = why.map(clear_signature::Blocker::ReadOnly);
        }
        Ok(list)
    }
}

impl Session {
    /// Snapshot the current signing input for a background worker. Check its output with
    /// [`Session::check_save`] and ensure the document's generation is unchanged before saving.
    pub fn signing_input(&self, doc: DocId, mut opts: SignOptions) -> Result<(pdfcraft_cos::Document, SignOptions), EditError> {
        let d = self.get(doc).ok_or(EditError::NoDocument)?;
        let editor = d.editor.as_ref().ok_or_else(|| EditError::ReadOnly(d.read_only_reason.clone().unwrap_or_default()))?;
        if opts.date.is_empty() {
            opts.date = self.signing_date();
        }
        Ok((editor.cos.clone(), opts))
    }
}

impl Session {
    /// An empty sibling session sharing document identities and user preferences.
    pub fn new_peer(&self) -> Self {
        Self {
            docs: Vec::new(),
            next_id: self.next_id.clone(),
            clock: self.clock,
            utc_offset: self.utc_offset,
            trust: self.trust.clone(),
            js_off: self.js_off,
            date_format: self.date_format.clone(),
            date_language: self.date_language.clone(),
            xfa_time_scale: self.xfa_time_scale,
            identities: self.identities.clone(),
            captions: self.captions.clone(),
            field_highlight: self.field_highlight,
        }
    }
}

impl Session {
    /// Move a document to a sibling session without saving or reparsing it.
    /// Its identity, password, editor, Undo/Redo and autosave generations are preserved.
    /// A missing id leaves this session unchanged.
    pub fn detach_document(&mut self, id: DocId) -> Option<Self> {
        let index = self.docs.iter().position(|d| d.id == id)?;
        let mut peer = self.new_peer();
        peer.docs.push(self.docs.remove(index));
        Some(peer)
    }
}

impl Session {
    /// Move a live document between sibling sessions, preserving its identity.
    /// Missing, unrelated or colliding destinations leave both sessions unchanged.
    pub fn transfer_document(&mut self, id: DocId, target: &mut Self) -> Option<DocId> {
        let index = self.docs.iter().position(|d| d.id == id)?;
        if !Arc::ptr_eq(&self.next_id, &target.next_id) || target.get(id).is_some() {
            return None;
        }
        target.docs.push(self.docs.remove(index));
        Some(id)
    }
}

use super::clear_signature;
