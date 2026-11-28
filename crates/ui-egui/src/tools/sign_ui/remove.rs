//! Lion PDF ▸ Sign ▸ Remove signature: take one signature (a digital signature's value, with or
//! without its field, or a Fill & Sign mark) out of a copy of the document. A digital signature is
//! removed only by the holder of its signing identity, proven with the same digital ID. The copy
//! is saved beside the original, never over it; a failed save undoes the removal.

use craft::host::PickRequest;
use craft::ui::contract::Bind;
use craft::ui::patterns::form_dialogs::FormDialog;
use pdfcraft_engine::clear_signature::{Kind, Removable, Target, said};
use pdfcraft_engine::{DocId, SignatureStatus, sign};

use super::DigitalIdEntry;
use crate::PdfCraftApp;

/// The open Remove signature dialog.
#[derive(Clone, PartialEq)]
pub struct RemoveSigDraft {
    doc: DocId,
    items: Vec<Removable>,
    selected: Option<usize>,
    /// Keep the emptied signature field, ready to sign again.
    keep_field: bool,
    /// The digital ID that proves the signature is the user's, by its place in the list.
    identity: Option<usize>,
    password: String,
}

impl Default for RemoveSigDraft {
    fn default() -> Self {
        Self { doc: DocId(0), items: Vec::new(), selected: None, keep_field: false, identity: None, password: String::new() }
    }
}

impl RemoveSigDraft {
    fn chosen(&self) -> Option<&Removable> {
        self.selected.and_then(|i| self.items.get(i)).filter(|r| r.blocked.is_none())
    }
}

/// Whether removing `r` needs its signer's identity: a digital signature's value does.
fn needs_identity(r: &Removable) -> bool {
    matches!(r.target, Target::Field(_)) && !matches!(r.kind, Kind::Visual { .. })
}

fn kind_label(r: &Removable) -> String {
    match r.kind {
        Kind::Digital => tl!("Digital signature").to_string(),
        Kind::Certification(p) => crate::i18n::fmt(tl!("Certification signature (level {p})"), &[("p", &p.to_string())]),
        Kind::DocumentTimestamp => tl!("Document timestamp").to_string(),
        Kind::Visual { initials: false } => tl!("Visual signature (Fill & Sign, not digital)").to_string(),
        Kind::Visual { initials: true } => tl!("Visual initials (Fill & Sign, not digital)").to_string(),
    }
}

/// The signature's line under its title: field, page, date and validation.
fn details(r: &Removable) -> String {
    let mut sub = Vec::new();
    if let Some(f) = &r.field {
        sub.push(crate::i18n::fmt(tl!("Field: {field}"), &[("field", f)]));
    }
    if let Some(p) = r.page {
        sub.push(crate::i18n::fmt(tl!("Page {p}"), &[("p", &(p + 1).to_string())]));
    }
    if let Some(date) = &r.date {
        sub.push(sign::pdf::display_date(date));
    }
    if let Some(status) = r.status {
        sub.push(
            match status {
                SignatureStatus::Valid => tl!("validated: valid"),
                SignatureStatus::Unknown => tl!("validated: validity unknown"),
                SignatureStatus::Invalid => tl!("validated: invalid"),
            }
            .to_string(),
        );
    }
    sub.join(" · ")
}

/// A wording from the engine, translated, with its names filled in.
fn worded(wording: (&'static str, String)) -> String {
    said((tl!(wording.0), wording.1))
}

impl PdfCraftApp {
    /// Opens Remove signature for the active document, with `field` chosen when given.
    pub fn start_remove_signature(&mut self, field: Option<&str>) {
        let Some((_, id)) = self.active_ids() else { return };
        self.refresh_os_key_store_ids();
        match self.session.removable_signatures(id) {
            Ok(items) => {
                let selected = field
                    .and_then(|f| items.iter().position(|r| r.field.as_deref() == Some(f)))
                    .or_else(|| items.iter().position(|r| r.blocked.is_none()));
                let identity = self.digital_ids.iter().position(|e| e.unusable.is_none());
                self.remove_sig = Some(RemoveSigDraft { doc: id, items, selected, keep_field: false, identity, password: String::new() });
            }
            Err(e) => self.notify_error(e),
        }
    }

    /// One pass of the open Remove signature dialog.
    pub(crate) fn remove_signature_pass(&mut self, ui: &mut egui::Ui) {
        let Some(mut draft) = self.remove_sig.clone() else { return };
        let ids: Vec<DigitalIdEntry> = self.digital_ids.clone();
        let mut open = true;
        let ready =
            draft.chosen().is_some_and(|r| !needs_identity(r) || draft.identity.is_some_and(|i| ids.get(i).is_some_and(|e| e.unusable.is_none())));
        let out = FormDialog::new(tl!("Remove signature"), Bind::bound(&mut draft))
            .opened(Bind::bound(&mut open))
            .ok_label(tl!("Remove and save as…"))
            .ok_enabled(ready)
            .show(ui, |ui, _, d| {
                body(ui, d, &ids);
                true
            });
        if out.closed.is_some() && out.accepted.is_none() {
            self.remove_sig = None;
            return;
        }
        if out.accepted.is_some() {
            self.remove_sig = None;
            self.confirm_remove_signature(draft);
            return;
        }
        self.remove_sig = Some(draft);
    }

    /// Remove and save as…: ask where the new version goes (never over the original), then
    /// remove and save there.
    fn confirm_remove_signature(&mut self, draft: RemoveSigDraft) {
        let Some(chosen) = draft.chosen().cloned() else { return };
        let Some(doc) = self.session.get(draft.doc) else { return };
        let stem = std::path::Path::new(&doc.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
        let folder = doc.path.clone();
        let id = draft.doc;
        let identity = draft.identity.and_then(|i| self.digital_ids.get(i)).map(|e| e.path.clone());
        let mut request =
            PickRequest::save("sign.unsigned").file_name(crate::i18n::fmt(tl!("{name}_unsigned.pdf"), &[("name", &stem)])).filter("PDF", &["pdf"]);
        if let Some(parent) = folder.as_deref().and_then(|p| std::path::Path::new(p).parent()) {
            request = request.default_directory(parent);
        }
        let (keep_field, password) = (draft.keep_field, draft.password);
        let target = self.pick_target(Some(id));
        self.pick_file(request, target, move |app, ctx, picked| {
            let Some(dest) = picked.first_path() else { return };
            app.remove_signature_to(ctx, id, &chosen, keep_field, &dest, identity.as_deref(), &password);
        });
    }

    /// Remove `chosen` from document `id` (undoable) and check that it is gone; if it isn't, undo
    /// it again. Returns the edit position before the removal, for [`Self::undo_back_to`].
    fn apply_removal(&mut self, ctx: &egui::Context, id: DocId, chosen: &Removable, keep_field: bool) -> Result<u64, String> {
        let visuals = |app: &Self| app.session.removable_signatures(id).map(|l| l.iter().filter(|r| matches!(r.kind, Kind::Visual { .. })).count());
        let before = visuals(self).map_err(|e| e.to_string())?;
        let edit = match &chosen.target {
            Target::Field(f) => pdfcraft_engine::Edit::ClearSignature { field: f.clone(), remove_field: !keep_field },
            Target::Annotation { page, index } => pdfcraft_engine::Edit::DeleteAnnotation { page: *page, index: *index },
            Target::None => return Err(tl!("this signature can't be removed on its own").to_string()),
        };
        let mark = self.session.get(id).and_then(|doc| doc.edit_position()).unwrap_or_default();
        self.session.apply(id, edit).map_err(|e| e.to_string())?;
        let gone = match &chosen.target {
            Target::Field(f) => self.session.get(id).is_some_and(|doc| !doc.signatures.iter().any(|s| &s.field == f && (s.signed || !keep_field))),
            _ => visuals(self).is_ok_and(|n| n < before),
        };
        self.refresh_view_of(ctx, id);
        if !gone {
            let _ = self.session.undo(id);
            self.refresh_view_of(ctx, id);
            return Err(tl!("it was still in the document afterwards, so the change was undone").to_string());
        }
        Ok(mark)
    }

    /// Undo edits back to edit position `mark`: the removal, and whatever saving added after it,
    /// never an edit the user made before it.
    fn undo_back_to(&mut self, ctx: &egui::Context, id: DocId, mark: u64) {
        while self.session.get(id).and_then(|doc| doc.edit_position()).is_some_and(|p| p > mark) {
            if self.session.undo(id).is_err() {
                break;
            }
        }
        self.refresh_view_of(ctx, id);
    }

    /// Remove `chosen` and save the document to `dest`, a file other than its own. When the
    /// save fails the removal is undone, so the document is as it was and nothing is claimed.
    #[allow(clippy::too_many_arguments)]
    fn remove_signature_to(
        &mut self,
        ctx: &egui::Context,
        id: DocId,
        chosen: &Removable,
        keep_field: bool,
        dest: &std::path::Path,
        identity: Option<&str>,
        password: &str,
    ) {
        let Some(doc) = self.session.get(id) else { return };
        if doc.path.as_deref().is_some_and(|p| crate::same_file(p, &dest.to_string_lossy())) {
            self.notify_tr("Choose another file: the original is kept as it is, and the version without the signature is saved next to it.");
            return;
        }
        if needs_identity(chosen)
            && let Err(error) = self.verify_removal_identity(id, chosen, identity, password)
        {
            self.notify_fmt("The signature was not removed: {e}", &[("e", &error)]);
            return;
        }
        let mark = match self.apply_removal(ctx, id, chosen, keep_field) {
            Ok(mark) => mark,
            Err(e) => {
                self.notify_fmt("The signature was not removed: {e}", &[("e", &e)]);
                return;
            }
        };
        if !self.save_doc_to(ctx, id, dest, crate::editing::SaveTarget::As) {
            // The save said why; the document goes back to how it was.
            self.undo_back_to(ctx, id, mark);
            return;
        }
        let Some(doc) = self.session.get(id) else { return };
        let file = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dest.display().to_string());
        let field = chosen.field.as_deref().unwrap_or("");
        let what = match (&chosen.target, keep_field) {
            (Target::Annotation { page, .. }, _) => {
                crate::i18n::fmt(tl!("Removed the visual signature on page {p}"), &[("p", &(page + 1).to_string())])
            }
            (_, true) => crate::i18n::fmt(tl!("Removed the signature {field} and kept its empty field"), &[("field", field)]),
            (_, false) => crate::i18n::fmt(tl!("Removed the signature {field} with its field"), &[("field", field)]),
        };
        let history = if doc.is_signed() {
            tl!("Other signatures remain, so the removed one is still in the file's earlier revision; check the remaining signatures again.")
        } else {
            tl!("The new file has no signature left and no earlier revision holding it.")
        };
        self.notify(crate::i18n::fmt(tl!("{what}. Saved {file}. {history}"), &[("what", &what), ("file", &file), ("history", history)]));
        self.set_right(Some(crate::RightPanel::Signatures));
    }

    /// Proves the user holds the signature's signing identity: the chosen digital ID must be the
    /// certificate that signed, and its key must sign a digest bound to this document and field.
    fn verify_removal_identity(&mut self, doc: DocId, chosen: &Removable, reference: Option<&str>, password: &str) -> Result<(), String> {
        use std::io::Read;
        let field = chosen.field.as_deref().ok_or(tl!("This signature has no field"))?;
        let (certificate, binding) = {
            let doc = self.session.get(doc).ok_or(tl!("The document was closed"))?;
            let signature = doc.signatures.iter().find(|s| s.signed && s.field == field).ok_or(tl!("This signature is no longer in the document"))?;
            let certificate = signature.certificate.clone().ok_or(tl!("The signature has no certificate to prove ownership against"))?;
            let binding = sign::keys::DigestAlg::Sha256.digest(&[&doc.bytes, field.as_bytes()]);
            (certificate, binding)
        };
        let reference = reference.ok_or(tl!("Choose the digital ID that made this signature."))?;
        let identity = if reference.starts_with("windows:") {
            super::windows_id(reference)?
        } else if reference.starts_with("keychain:") {
            super::keychain_id(reference)?
        } else {
            let mut bytes = Vec::new();
            std::fs::File::open(reference).map_err(|e| e.to_string())?.take(16_777_217).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() > 16_777_216 {
                return Err(tl!("The digital ID file exceeds 16 MiB").to_string());
            }
            sign::pkcs12::open(&bytes, password).map_err(|e| match e {
                sign::SignError::WrongPassword => tl!("The password is incorrect.").to_string(),
                e => e.to_string(),
            })?
        };
        if identity.certificate.fingerprint() != certificate.fingerprint() {
            return Err(tl!("That digital ID did not make this signature. Choose the one that did.").to_string());
        }
        sign::ownership::prove(&identity, &certificate, &binding).map_err(|e| e.to_string())
    }
}

/// The dialog's body: the signatures, what removing the chosen one does, and the proof of identity.
fn body(ui: &mut egui::Ui, d: &mut RemoveSigDraft, ids: &[DigitalIdEntry]) {
    if d.items.is_empty() {
        ui.label(tl!("This document has no signature that can be removed."));
        ui.add_space(6.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(tl!(
                    "A signature drawn into a page's content, like a scanned page, is neither a field nor an annotation, so it can't be removed on its own. Redact that area, or delete the picture with Edit PDF when the signature is an image of its own."
                ))
                .weak(),
            )
            .wrap(),
        );
        return;
    }
    ui.label(tl!("Choose the signature to remove:"));
    ui.add_space(4.0);
    let items = d.items.clone();
    egui::ScrollArea::vertical().id_salt("remove-signatures").max_height(220.0).show(ui, |ui| {
        use craft::ui::contract::{Props as _, Show as _, ShowContainer as _};
        craft::ui::choice::RadioGroup::new(tl!("Signatures"), Bind::bound(&mut d.selected)).show(ui, |ui, _| {
            for (i, r) in items.iter().enumerate() {
                let mut head = kind_label(r);
                match (r.signer.as_deref(), &r.kind) {
                    (Some(signer), _) => head.push_str(&format!(" — {signer}")),
                    (None, Kind::Visual { .. }) => {}
                    (None, _) => head.push_str(&format!(" — {}", tl!("unknown signer"))),
                }
                craft::ui::choice::Radio::new(Some(i)).label(head).enabled(r.blocked.is_none()).show(ui);
                ui.indent(("remove-sig-row", i), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(details(r)).small().weak()).wrap());
                    if let Some(b) = &r.blocked {
                        ui.add(egui::Label::new(egui::RichText::new(worded(b.wording())).small()).wrap());
                    }
                });
            }
        });
    });
    let Some(chosen) = d.chosen().cloned() else { return };
    if !chosen.effects.is_empty() {
        ui.add_space(6.0);
        for effect in &chosen.effects {
            ui.add(egui::Label::new(format!("• {}", worded(effect.wording()))).wrap());
        }
    }
    if matches!(chosen.target, Target::Field(_)) {
        ui.add_space(6.0);
        crate::widgets::check(ui, &mut d.keep_field, tl!("Keep the empty signature field, ready to sign again"));
    }
    if needs_identity(&chosen) {
        ui.add_space(8.0);
        ui.label(tl!("Only the signer can remove a digital signature. Prove it with the digital ID that signed:"));
        let usable: Vec<(usize, &DigitalIdEntry)> = ids.iter().enumerate().filter(|(_, e)| e.unusable.is_none()).collect();
        if usable.is_empty() {
            ui.label(egui::RichText::new(tl!("No digital ID is set up. Add it in Sign with a Digital ID first.")).weak());
            return;
        }
        let options = usable.iter().map(|(i, e)| (Some(*i), format!("{} — {}", e.name, e.issuer))).collect();
        crate::widgets::select(ui, tl!("Digital ID"), &mut d.identity, options);
        let file = d.identity.and_then(|i| ids.get(i)).is_some_and(|e| !e.path.starts_with("windows:") && !e.path.starts_with("keychain:"));
        if file {
            ui.horizontal(|ui| {
                let label = ui.label(tl!("Password"));
                ui.add(egui::TextEdit::singleline(&mut d.password).password(true).desired_width(220.0)).labelled_by(label.id);
            });
        }
    }
}
