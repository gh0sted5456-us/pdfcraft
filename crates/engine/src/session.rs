//! The session: the open documents and the operations on them.

use std::sync::{Arc, OnceLock};

use pdfcraft_cos::{SaveOptions, write_full, write_incremental};
use pdfcraft_render::{LayerOp, OpenError, ReadingColors, RenderConfig, RenderPool, inspect};

#[cfg(test)]
use crate::check_refresh_panic;
use crate::edit::{EditCtx, check_permission, run_edit};
use crate::history::{Keys, MAX_UNDO, Scope, keys_after, scope_of, uses_scripts};
use crate::{
    BookmarkPage, BookmarkRow, CombineSource, Combined, DataFormat, DocId, Document, Edit, EditError, Hidden, ImageResolution,
    LOW_RESOLUTION_PRINT_DPI, Label, ObjectKind, OptimizeReport, RecoverySnapshot, SignOptions, SourceKind, SplitPart, SummarySort, TrustStore,
    XFA_REMOVED_PREFIX, XFA_SCRIPTS_OFF, apply_layer_state, comment_list, comment_summary, decode_text, display_bytes, guard, info_fields, js,
    local_utc_offset, note_warnings, open_source, open_source_with, optimize, outline_rows, parse_ymd, print, print_raster, render_threads,
    scan_colours, set_default_layer_state, shown_under_redactions, signatures_of, source_kind, sync_xfa_state, undrawn_notices, use_layer_choices,
    writable, xfa, xfa_sync_datasets,
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
    /// Preferences ▸ JavaScript ▸ Enable Acrobat JavaScript, inverted (on by default).
    pub(crate) js_off: bool,
    /// Preferences ▸ Date format, when not the default (see [`dates`]).
    pub(crate) date_format: Option<String>,
    /// Preferences ▸ Date format ▸ Language, when not following the interface language.
    pub(crate) date_language: Option<String>,
    /// What the XFA scripts' time limits are multiplied by (`None`: 1); see
    /// [`Session::with_xfa_time_scale`].
    pub(crate) xfa_time_scale: Option<u32>,
    /// The person's digital identities, tried on a certificate-protected document's envelopes.
    pub(crate) identities: Vec<Arc<pdfcraft_sign::DigitalId>>,
}

impl Session {
    /// The rest of opening, from the document as parsed (`bytes` are what the renderer reads).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_from(
        context: &OpenContext,
        name: String,
        path: Option<String>,
        bytes: Arc<Vec<u8>>,
        password: Option<&str>,
        cos: Result<Result<pdfcraft_cos::Document, pdfcraft_cos::CosError>, Box<dyn std::any::Any + Send>>,
        certificate: Option<CertificateKey>,
    ) -> Result<PreparedDocument, OpenError> {
        // The renderer authenticates on its own. It cannot use the owner password of R2–R4
        // files, so give it the user password that owner authentication recovers.
        let (info, render_password) = match inspect(bytes.clone(), password) {
            Ok(info) => (info, password.map(str::to_owned)),
            Err(OpenError::WrongPassword) => {
                let user = match &cos {
                    Ok(Ok(d)) => recovered_render_password(d),
                    _ => None,
                };
                let user = user.ok_or(OpenError::WrongPassword)?;
                (inspect(bytes.clone(), Some(&user))?, Some(user))
            }
            Err(e) => return Err(e),
        };
        // A dynamic XFA form is a shell around an XML template; lay the template out into real
        // pages and fields so the rest of the engine works on it. The original bytes stay: the
        // laid-out form is one appended revision.
        let opts = SaveOptions { mod_date: context.now.map(pdfcraft_cos::pdf_date), ..SaveOptions::default() };
        // Write `work` as one more revision and reopen it: what the document then is.
        let rebase = |work: &pdfcraft_cos::Document| -> Result<(Arc<Vec<u8>>, DocInfo, pdfcraft_cos::Document), String> {
            let new_bytes = write_incremental(work, &opts).map(Arc::new).map_err(|e| e.to_string())?;
            let new_info = inspect(new_bytes.clone(), render_password.as_deref()).map_err(|e| e.to_string())?;
            let new_cos = pdfcraft_cos::Document::open_with_password(new_bytes.clone(), password).map_err(|e| e.to_string())?;
            Ok((new_bytes, new_info, new_cos))
        };
        let (bytes, info, cos, xfa) = match (info.xfa, cos) {
            (Some(pdfcraft_render::Xfa::Dynamic), Ok(Ok(cos))) if pdfcraft_xfa::existing_layout(&cos).is_none() => {
                let mut work = cos.clone();
                let laid_out = guard(|| xfa_layout(&mut work))
                    .unwrap_or_else(|m| Err(format!("laying it out failed unexpectedly ({m})")))
                    .and_then(|report| rebase(&work).map(|(b, i, c)| (b, i, c, report)));
                match laid_out {
                    Ok((b, i, c, report)) => (b, i, Ok(Ok(c)), Some(report)),
                    Err(e) => {
                        let mut info = info;
                        info.warnings.push(format!("This dynamic XFA form could not be laid out: {e}"));
                        (bytes, info, Ok(Ok(cos)), None)
                    }
                }
            }
            // A static XFA form, or a dynamic one laid out earlier: its datasets may hold values
            // filled in by another viewer since; give the fields those values.
            (Some(_), Ok(Ok(cos))) => {
                let report = pdfcraft_xfa::existing_layout(&cos);
                let mut work = cos.clone();
                let synced =
                    guard(|| xfa_values_from_datasets(&mut work)).unwrap_or_else(|m| Err(format!("reading its data failed unexpectedly ({m})")));
                match synced {
                    Ok(names) if names.is_empty() => (bytes, info, Ok(Ok(cos)), report),
                    Ok(names) => match rebase(&work) {
                        Ok((b, mut i, c)) => {
                            // The fields were rewritten from the XFA data: say so, and which.
                            let shown: Vec<&str> = names.iter().take(5).map(String::as_str).collect();
                            let more = if names.len() > shown.len() { format!(" and {} more", names.len() - shown.len()) } else { String::new() };
                            i.warnings.push(format!(
                                "{} form field value{} were taken from this form's XFA data (filled in by another viewer): {}{more}",
                                names.len(),
                                if names.len() == 1 { "" } else { "s" },
                                shown.join(", ")
                            ));
                            (b, i, Ok(Ok(c)), report)
                        }
                        Err(e) => {
                            let mut info = info;
                            info.warnings.push(format!("The values in this form's XFA data could not be applied: {e}"));
                            (bytes, info, Ok(Ok(cos)), report)
                        }
                    },
                    Err(e) => {
                        let mut info = info;
                        info.warnings.push(format!("The values in this form's XFA data could not be applied: {e}"));
                        (bytes, info, Ok(Ok(cos)), report)
                    }
                }
            }
            (_, cos) => (bytes, info, cos, None),
        };
        // A laid-out XFA form: its initialize and calculate scripts run now, as on opening in
        // Acrobat; what they change is one more revision, still nothing to save.
        let mut script_output = js::JsOutput::default();
        let mut ran_away = false;
        let xfa_template = match (&xfa, &cos) {
            (Some(_), Ok(Ok(c))) => xfa::template(c),
            _ => None,
        };
        let (bytes, info, cos) = match (&xfa, cos) {
            (Some(_), Ok(Ok(cos))) if !context.js_off => match xfa_template.clone() {
                Some(tpl) => {
                    let mut work = cos.clone();
                    let pages = info.pages.len();
                    let ran = guard(|| {
                        let mut run = xfa::XfaRun {
                            page_count: pages,
                            out: &mut script_output,
                            datasets: None,
                            ran_away: false,
                            time_scale: context.xfa_time_scale,
                        };
                        let changes = xfa::on_open(&mut work, &tpl, &mut run);
                        if run.ran_away {
                            ran_away = true;
                        }
                        changes
                    })
                    .unwrap_or_else(|m| Err(format!("its scripts failed unexpectedly ({m})")));
                    match ran {
                        Ok(changes) if work.is_modified() => match rebase(&work) {
                            Ok((b, mut i, c)) => {
                                // The file as saved differs from what opening shows: say so.
                                if !changes.is_empty() {
                                    i.warnings.push(format!(
                                        "This form's scripts changed it on opening ({}); saving keeps those changes",
                                        changes.describe()
                                    ));
                                }
                                (b, i, Ok(Ok(c)))
                            }
                            Err(e) => {
                                script_output.errors.push(format!("The form's scripts could not be applied: {e}"));
                                (bytes, info, Ok(Ok(cos)))
                            }
                        },
                        Ok(_) => (bytes, info, Ok(Ok(cos))),
                        Err(e) => {
                            script_output.errors.push(format!("The form's scripts could not run: {e}"));
                            (bytes, info, Ok(Ok(cos)))
                        }
                    }
                }
                None => (bytes, info, Ok(Ok(cos))),
            },
            (_, cos) => (bytes, info, cos),
        };
        // The catalog leads to no page, but the reader found pages by scanning: gather the page
        // objects into a page tree as one more revision, or page edits address none of them
        // (#1656). Kept only when the reader then shows the same pages, and never on a signed
        // document, where writing the repair could rewrite the file under its signatures.
        let (bytes, info, cos) = match cos {
            Ok(Ok(cos)) if !info.pages.is_empty() && pdfcraft_organize::page_count(&cos).unwrap_or(0) == 0 => {
                let mut work = cos.clone();
                let signed = || pdfcraft_sign::pdf::list(&cos, cos.bytes(), &context.trust).iter().any(|s| s.signed);
                let rebuilt = match pdfcraft_organize::recover_page_tree(&mut work) {
                    Ok(n) if n > 0 && !signed() => rebase(&work).ok().filter(|(_, i, _)| same_pages(&i.pages, &info.pages)),
                    _ => None,
                };
                match rebuilt {
                    Some((b, i, mut c)) => {
                        for line in cos.repair_log() {
                            c.note_repair(line.clone());
                        }
                        c.note_repair(format!("the page tree is missing; its {} pages were found by scanning the file", i.pages.len()));
                        (b, i, Ok(Ok(c)))
                    }
                    None => (bytes, info, Ok(Ok(cos))),
                }
            }
            cos => (bytes, info, cos),
        };
        // XFA notes survive the document being re-read after edits.
        let xfa_warnings: Vec<String> = if info.xfa.is_some() { info.warnings.clone() } else { Vec::new() };
        let mut document = Self::build_document(&context.trust, name, path, bytes, info, cos, render_password, password, certificate, xfa);
        // A script that ran away on open turns the form's scripts off for this document.
        document.xfa_template = if ran_away { None } else { xfa_template };
        document.js_output.append(script_output);
        if ran_away {
            document.xfa_scripts_off = true;
            document.js_output.errors.push(XFA_SCRIPTS_OFF.into());
        }
        note_warnings(&mut document.xfa_warnings, &xfa_warnings);
        Ok(PreparedDocument { document })
    }
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
            doc.trust = self.trust.clone();
            let Some(editor) = doc.editor.as_mut() else { continue };
            // Keep verdict refresh and renderer refresh together. Viewer caches are
            // invalidated by the UI trust path; trust alone changes no document artwork.
            let signatures = if doc.signatures.is_empty() {
                Vec::new()
            } else if certificate_secured(&editor.cos) {
                // `doc.bytes` are a decrypted copy; signatures cover the file itself. Where it
                // cannot be written, the verdicts stay as they were.
                match working_file(&editor.cos) {
                    Ok(file) => signatures_of(&editor.cos, &file, &doc.trust, &doc.sig_cache),
                    Err(_) => doc.signatures.as_ref().clone(),
                }
            } else {
                signatures_of(&editor.cos, &doc.bytes, &doc.trust, &doc.sig_cache)
            };
            doc.display = display_bytes(Some(editor), &doc.bytes);
            doc.signatures = Arc::new(signatures);
            doc.renderer = RenderPool::new(doc.display.clone(), render_threads(), doc.config.clone());
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

use super::*;
