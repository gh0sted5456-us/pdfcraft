//! Use a certificate (execution plan M9.6): Digitally sign and Certify (drag a rectangle, or
//! click an empty signature field), the Sign with a Digital ID / Configure New Digital ID /
//! Sign as dialogs, the Signatures panel, and the signature message bar.

use std::path::PathBuf;

use craft::host::PickRequest;
use craft::review::{SavedSignature, SignatureImage};
use pdfcraft_engine::sign::{self, Appearance, AppearanceLabels, Certificate, DigitalId, Name, PrivateKey};
use pdfcraft_engine::{SignOptions, SignatureInfo, SignatureStatus, StatusCaptions};

use crate::FilePurpose;
use crate::PdfCraftApp;
use crate::canvas::{DocView, Snapshot, SnapshotKind};

pub(crate) mod cert_viewer;
pub(crate) mod dialog;
mod panel;
pub(crate) mod tool;

#[cfg(test)]
use dialog::detail_lines;
use dialog::graphic_of;
pub(crate) use dialog::{appearance_labels, status_captions};
pub use panel::{CertViewer, PanelAction, certificate_pem};
pub(crate) use panel::{banner, panel, status_tone};

/// A digital ID the app knows about (Acrobat: Digital ID files). The file stays where it is;
/// its password is asked for at each signing.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DigitalIdEntry {
    pub path: String,
    #[serde(default)]
    pub fingerprint: String,
    pub name: String,
    pub issuer: String,
    pub email: String,
    pub expires: String,
    /// Why the ID can't sign (a Windows store certificate whose key PdfCraft can't use): shown
    /// greyed with the reason, and never selectable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unusable: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignStep {
    /// Sign with a Digital ID: pick one.
    Choose,
    /// Configure a Digital ID for signing: from a file or a new self-signed one.
    Configure,
    /// Sign as "Name": appearance, password, Sign.
    SignAs,
}

/// The key algorithms of a new digital ID (Acrobat offers RSA; P-256 also works in browsers).
pub const KEY_ALGORITHMS: [(&str, &str); 4] =
    [("2048-bit RSA", "rsa2048"), ("3072-bit RSA", "rsa3072"), ("4096-bit RSA", "rsa4096"), ("256-bit ECDSA (P-256)", "p256")];

#[derive(Clone, Debug, PartialEq)]
pub struct NewIdDraft {
    /// `true`: create a self-signed ID; `false`: use an ID from a file.
    pub create: bool,
    pub name: String,
    pub unit: String,
    pub organization: String,
    pub email: String,
    pub country: String,
    pub key: usize,
    pub password: String,
    pub confirm: String,
    /// Use a Digital ID from a file: its path and password.
    pub file: String,
    pub file_password: String,
}

impl Default for NewIdDraft {
    fn default() -> Self {
        Self {
            create: true,
            name: String::new(),
            unit: String::new(),
            organization: String::new(),
            email: String::new(),
            country: String::new(),
            key: 0,
            password: String::new(),
            confirm: String::new(),
            file: String::new(),
            file_password: String::new(),
        }
    }
}

impl Drop for NewIdDraft {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.password.zeroize();
        self.confirm.zeroize();
        self.file_password.zeroize();
    }
}

/// What a visible signature shows on its left (Acrobat: Configure Signature Appearance ▸ Text,
/// Draw or Image). The one last signed with is remembered.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SignGraphic {
    /// The signer's name in large type (when Name is on).
    #[default]
    Name,
    /// The saved Fill & Sign signature, as it is when signing.
    FillSign,
    /// An imported PNG or JPEG.
    Image(SignatureImage),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SignDraft {
    pub page: usize,
    /// The signature rectangle (user space); `None`: invisible.
    pub rect: Option<[f64; 4]>,
    /// Sign this empty signature field instead.
    pub field: Option<String>,
    /// Certify (DocMDP) with these permissions: 1 none, 2 form fill and signing, 3 also comments.
    pub certify: Option<u8>,
    pub protection: SignProtection,
    pub protected_pages: Vec<usize>,
    pub choosing_pages: Option<pdfcraft_engine::DocId>,
    pub step: SignStep,
    pub selected: Option<usize>,
    pub password: String,
    pub reason: String,
    pub location: String,
    pub appearance: Appearance,
    pub timestamp: bool,
    pub graphic: SignGraphic,
    pub background: SignBackground,
    /// The image the dialog holds (remembered or chosen), so Image can be chosen again.
    pub image: Option<SignatureImage>,
    pub new_id: NewIdDraft,
    pub error: Option<String>,
    /// Sign with a Digital ID ▸ Remove was clicked for this list entry: asking to confirm.
    pub confirm_remove: Option<usize>,
}

impl SignDraft {
    /// Starts at Choose when there are IDs to show (even only unusable ones, so their reasons
    /// are seen), with the first usable one selected.
    fn new(page: usize, rect: Option<[f64; 4]>, field: Option<String>, certify: Option<u8>, ids: &[DigitalIdEntry]) -> Self {
        Self {
            page,
            rect,
            field,
            certify,
            protection: match certify {
                Some(1) => SignProtection::NoChanges,
                Some(3) => SignProtection::Comments,
                Some(_) => SignProtection::Forms,
                None => SignProtection::None,
            },
            protected_pages: vec![page],
            choosing_pages: None,
            step: if ids.is_empty() { SignStep::Configure } else { SignStep::Choose },
            selected: ids.iter().position(|e| e.unusable.is_none()),
            password: String::new(),
            reason: String::new(),
            location: String::new(),
            appearance: Appearance::default(),
            timestamp: false,
            graphic: SignGraphic::Name,
            background: SignBackground::Lion,
            image: None,
            new_id: NewIdDraft::default(),
            error: None,
            confirm_remove: None,
        }
    }
}

impl Drop for SignDraft {
    fn drop(&mut self) {
        use zeroize::Zeroize as _;
        self.password.zeroize();
    }
}

/// The empty draft a dialog value starts from; a real one comes from `start_signing`.
impl Default for SignDraft {
    fn default() -> Self {
        Self::new(0, None, None, None, &[])
    }
}

/// What a page asks of the signing flow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SignView {
    /// A rectangle was drawn: (page, user-space rect).
    pub drawn: Option<(usize, [f64; 4])>,
    /// An empty signature field was clicked (its name).
    pub field: Option<String>,
    pub clear: Option<String>,
    pub context: Option<String>,
}

/// Where new digital IDs are saved: next to the recovery folder (`…/PdfCraft/Digital IDs`, or
/// `PdfCraftData/Digital IDs` in portable mode).
fn id_dir() -> Option<PathBuf> {
    crate::recovery::default_dir().and_then(|d| d.parent().map(|p| p.join("Digital IDs")))
}

/// Save a new digital ID as `<stem>.p12`, or `<stem> 2.p12`, … when the name is taken. The file
/// is always created fresh, never opened through a file or link already at the name (checking
/// first and then writing would let one be planted in between), and on Unix only its owner can
/// read it: it holds the private key.
fn save_new_id_file(dir: &std::path::Path, stem: &str, p12: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::{ErrorKind, Write};
    for i in 1..=10_000u32 {
        let path = dir.join(if i == 1 { format!("{stem}.p12") } else { format!("{stem} {i}.p12") });
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
        let file = match opts.open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            // Windows refuses `create_new` on a folder with "access denied"; the name is taken all
            // the same. A folder we can't write to, with nothing at the name, still fails here.
            Err(e) if e.kind() == ErrorKind::PermissionDenied && path.symlink_metadata().is_ok() => continue,
            Err(e) => return Err(e),
        };
        // The block closes the file before a failed one is removed (Windows can't remove an open file).
        let written = {
            let mut file = file;
            file.write_all(p12).and_then(|()| file.sync_all())
        };
        if let Err(e) = written {
            let _ = std::fs::remove_file(&path);
            return Err(e);
        }
        return Ok(path);
    }
    Err(std::io::Error::new(ErrorKind::AlreadyExists, "no free file name for the digital ID"))
}

/// A Keychain identity by its `keychain:` reference.
fn keychain_id(reference: &str) -> Result<DigitalId, String> {
    #[cfg(target_os = "macos")]
    return sign::keychain::find(reference).map_err(|e| e.to_string());
    #[cfg(not(target_os = "macos"))]
    Err(format!("{reference}: Keychain identities are only available on macOS"))
}

/// A Windows store identity by its `windows:` reference.
fn windows_id(reference: &str) -> Result<DigitalId, String> {
    #[cfg(target_os = "windows")]
    return sign::windows::find(reference).map_err(|e| e.to_string());
    #[cfg(not(target_os = "windows"))]
    Err(format!("{reference}: Windows certificate store identities are only available on Windows"))
}

pub fn entry_for(path: &str, c: &Certificate) -> DigitalIdEntry {
    DigitalIdEntry {
        path: path.to_string(),
        fingerprint: c.fingerprint(),
        name: c.display_name(),
        issuer: c.issuer.common_name().map(str::to_string).unwrap_or_else(|| c.issuer.display()),
        email: c.subject.email().unwrap_or("").to_string(),
        expires: format!("{:04}.{:02}.{:02}", c.not_after.year, c.not_after.month, c.not_after.day),
        unusable: None,
    }
}

/// A greyed entry for a Windows store certificate that can't sign, with the reason.
#[cfg(target_os = "windows")]
fn entry_unusable(u: &sign::windows::Unusable) -> DigitalIdEntry {
    DigitalIdEntry {
        path: format!("windows:{}", u.fingerprint.split(' ').collect::<String>().to_ascii_lowercase()),
        fingerprint: String::new(),
        name: u.subject.clone(),
        issuer: String::new(),
        email: String::new(),
        expires: String::new(),
        unusable: Some(u.reason.clone()),
    }
}

impl PdfCraftApp {
    /// Export certificate: write `c` as PEM into a chosen folder, named after its subject.
    pub(crate) fn export_certificate(&mut self, c: &Certificate) {
        let name = format!("{}.cer", craft::docpath::names::safe_file_name(&c.display_name(), "attachment"));
        self.write_files(&[(name, std::sync::Arc::new(certificate_pem(c).into_bytes()))], "Export certificate");
    }

    /// Start signing: the rectangle (or field) is known; show Sign with a Digital ID, after a
    /// dynamic form's preSign scripts for the field.
    pub fn start_signing(&mut self, ctx: &egui::Context, page: usize, rect: Option<[f64; 4]>, field: Option<String>, certify: Option<u8>) {
        self.refresh_os_key_store_ids();
        let mut draft = SignDraft::new(page, rect, field.clone(), certify, &self.digital_ids);
        draft.background = self.sign_background.clone();
        draft.graphic = match &self.sign_graphic {
            SignGraphic::FillSign if self.signature.is_none() => SignGraphic::Name,
            g => g.clone(),
        };
        if let SignGraphic::Image(image) = &self.sign_graphic {
            draft.image = Some(image.clone());
        }
        self.sign_draft = Some(draft);
        // A dynamic form's preSign scripts run now, as signing the field begins, so what they
        // change and say shows before the signer confirms (DA 31's date the signature and lock the
        // date). Signing ended without a signature takes them back ([`Self::settle_presign`]).
        if let Some(f) = &field
            && let Some((_, doc)) = self.active_ids()
        {
            self.run_presign(ctx, doc, f);
        }
    }

    /// Run signature field `field`'s preSign scripts in document `doc` and keep the step pending.
    fn run_presign(&mut self, ctx: &egui::Context, doc: pdfcraft_engine::DocId, field: &str) {
        match self.session.run_sign_event(doc, field, pdfcraft_engine::js::SignEvent::Before) {
            Ok(true) => {
                self.presign = self.session.get(doc).map(|d| (doc, d.undo_depth()));
                self.refresh_view_of(ctx, doc);
                let out = self.session.take_js_output(doc);
                self.handle_js(ctx, doc, out);
            }
            Ok(false) => {}
            Err(e) => self.notify_error(e),
        }
    }

    /// Take back a pending preSign step once signing has ended without a signature: the Sign
    /// dialog closed and no save picker is open for it. Only a step still the last one is taken
    /// back. Called every frame.
    pub(crate) fn settle_presign(&mut self, ctx: &egui::Context) {
        let Some((doc, depth)) = self.presign else { return };
        if self.picks_pending() {
            return;
        }
        if self.sign_draft.is_some() {
            return;
        }
        self.presign = None;
        if self.session.get(doc).is_some_and(|d| d.undo_depth() == depth) && self.session.undo(doc).is_ok() {
            self.refresh_view_of(ctx, doc);
        }
    }

    /// Show document `doc`'s changed form in its view.
    fn refresh_view_of(&mut self, ctx: &egui::Context, doc: pdfcraft_engine::DocId) {
        if let Some(i) = self.views.iter().position(|v| v.id == doc)
            && let Some(info) = self.session.get(doc).map(|d| d.info.clone())
        {
            self.views[i].document_changed(ctx, &info);
        }
    }

    /// List OS key store signing identities (after the file-based IDs).
    fn refresh_os_key_store_ids(&mut self) {
        self.digital_ids.retain(|e| !e.path.starts_with("keychain:") && !e.path.starts_with("windows:"));
        #[cfg(target_os = "macos")]
        if self.os_key_store_ids {
            match sign::keychain::identities(None) {
                Ok(ids) => {
                    for id in ids {
                        let mut entry = entry_for(&sign::keychain::reference(&id.certificate), &id.certificate);
                        // Smart card identities say so ("… (smart card)").
                        if let Some(name) = id.friendly_name {
                            entry.name = name;
                        }
                        self.digital_ids.push(entry);
                    }
                }
                Err(e) => self.notify_fmt("The Keychain's digital IDs couldn't be listed: {e}", &[("e", &e.to_string())]),
            }
        }
        #[cfg(target_os = "windows")]
        if self.os_key_store_ids {
            match sign::windows::list() {
                Ok(listing) => {
                    for id in &listing.ids {
                        self.digital_ids.push(entry_for(&sign::windows::reference(&id.certificate), &id.certificate));
                    }
                    // Certificates without a private key aren't identities; the rest are shown
                    // greyed with why they can't sign (issue #179).
                    for u in listing.unusable.iter().filter(|u| !u.no_private_key) {
                        self.digital_ids.push(entry_unusable(u));
                    }
                }
                Err(e) => self.notify_fmt("The Windows store's digital IDs couldn't be listed: {e}", &[("e", &e.to_string())]),
            }
        }
    }

    /// Add a digital ID file to the list (or select it if it's there).
    pub fn add_digital_id(&mut self, path: &str, cert: &Certificate) -> usize {
        if let Some(i) = self.digital_ids.iter().position(|e| e.path == path) {
            self.digital_ids[i] = entry_for(path, cert);
            return i;
        }
        self.digital_ids.push(entry_for(path, cert));
        self.digital_ids.len() - 1
    }

    /// Take the digital ID file at `index` off the list (issue #1396). The file itself stays on
    /// disk. Keychain and Windows store IDs belong to the OS and are listed again on every
    /// signing, so they can't be removed here. The open dialog's selection follows the list.
    pub fn remove_digital_id(&mut self, index: usize) -> bool {
        if self.digital_ids.get(index).is_none_or(|e| e.path.starts_with("keychain:") || e.path.starts_with("windows:")) {
            return false;
        }
        self.digital_ids.remove(index);
        if let Some(d) = self.sign_draft.as_mut() {
            d.confirm_remove = None;
            d.selected = match d.selected {
                Some(s) if s > index => Some(s - 1),
                Some(s) if s == index => self.digital_ids.iter().position(|e| e.unusable.is_none()),
                s => s,
            };
            if self.digital_ids.is_empty() {
                d.step = SignStep::Configure;
            }
        }
        true
    }

    /// Configure New Digital ID ▸ Create: a self-signed ID saved as a .p12 next to the app data.
    /// Returns its list index.
    fn create_digital_id(&mut self) -> Result<usize, String> {
        self.create_digital_id_in(id_dir())
    }

    /// [`Self::create_digital_id`], saving into `dir` (`None`: there is no folder to save in).
    fn create_digital_id_in(&mut self, dir: Option<PathBuf>) -> Result<usize, String> {
        let (path, cert) = {
            let Some(d) = self.sign_draft.as_ref().map(|d| &d.new_id) else { return Err("nothing to create".into()) };
            if d.name.trim().is_empty() {
                return Err("Enter a name.".into());
            }
            // The message is a catalogue key, so it spells the number out; keep the two in step.
            const _: () = assert!(sign::pkcs12::MIN_PASSWORD_CHARS == 8);
            if d.password.chars().count() < sign::pkcs12::MIN_PASSWORD_CHARS {
                return Err("The password must have at least 8 characters.".into());
            }
            if d.password != d.confirm {
                return Err("The passwords do not match.".into());
            }
            let key = match KEY_ALGORITHMS[d.key.min(KEY_ALGORITHMS.len() - 1)].1 {
                "rsa3072" => PrivateKey::generate_rsa(3072),
                "rsa4096" => PrivateKey::generate_rsa(4096),
                "p256" => PrivateKey::generate_p256(),
                _ => PrivateKey::generate_rsa(2048),
            }
            .map_err(|e| e.to_string())?;
            let now = self.session.now_secs();
            let dn = Name::build(&d.name, &d.unit, &d.organization, &d.email, &d.country);
            let serial = sign::keys::DigestAlg::Sha256.digest(&[d.name.as_bytes(), &now.to_be_bytes()])[..8].to_vec();
            let cert = Certificate::self_signed(&dn, &key, sign::Time::from_unix(now), 5, &serial).map_err(|e| e.to_string())?;
            let id = DigitalId { key, certificate: cert.clone(), chain: Vec::new(), friendly_name: Some(d.name.trim().to_string()) };
            let p12 = sign::pkcs12::write(&id, &d.password).map_err(|e| e.to_string())?;
            let dir = dir.ok_or("There is no folder to save the digital ID in.")?;
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let stem: String = d.name.trim().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
            let path = save_new_id_file(&dir, &stem, &p12).map_err(|e| e.to_string())?;
            (path, cert)
        };
        Ok(self.add_digital_id(&path.to_string_lossy(), &cert))
    }

    /// Configure New Digital ID ▸ Use a Digital ID from a file.
    fn import_digital_id(&mut self) -> Result<usize, String> {
        let (file, cert) = {
            let Some(d) = self.sign_draft.as_ref().map(|d| &d.new_id) else { return Err("nothing to import".into()) };
            let bytes = std::fs::read(&d.file).map_err(|e| format!("{}: {e}", d.file))?;
            let id = sign::pkcs12::open(&bytes, &d.file_password).map_err(|e| match e {
                sign::SignError::WrongPassword => "The password is incorrect.".to_string(),
                e => e.to_string(),
            })?;
            (d.file.clone(), id.certificate)
        };
        Ok(self.add_digital_id(&file, &cert))
    }

    /// Sign as …: open the ID, sign, save the signed file (Save As), and show it.
    fn finish_signing(&mut self, ctx: &egui::Context) -> Result<(), String> {
        // What's typed in a form field is signed with the document (#166). A rejected value keeps
        // the dialog open; the notice says why.
        if !self.commit_form_typing(ctx) {
            return Err(String::new());
        }
        let Some((_, doc_id)) = self.active_ids() else { return Err("no document".into()) };
        let d = self.sign_draft.as_ref().ok_or("nothing to sign")?;
        let entry = d.selected.and_then(|i| self.digital_ids.get(i)).filter(|e| e.unusable.is_none()).cloned().ok_or("Choose a digital ID.")?;
        let id = if entry.path.starts_with("keychain:") {
            keychain_id(&entry.path)?
        } else if entry.path.starts_with("windows:") {
            windows_id(&entry.path)?
        } else {
            let bytes = std::fs::read(&entry.path).map_err(|e| format!("{}: {e}", entry.path))?;
            sign::pkcs12::open(&bytes, &d.password).map_err(|e| match e {
                sign::SignError::WrongPassword => "The password is incorrect.".to_string(),
                e => e.to_string(),
            })?
        };
        let some = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_string());
        // Only a visible signature shows its graphic.
        let graphic = match self.graphic_sig(&d.graphic) {
            Some(saved) if d.rect.is_some() || d.field.is_some() => {
                Some(crate::fill_sign::graphic(&saved).ok_or("The saved signature has nothing to draw.")?)
            }
            _ => None,
        };
        let choice = d.graphic.clone();
        let opts = SignOptions {
            field: d.field.clone(),
            page: d.page,
            rect: d.rect,
            reason: some(&d.reason),
            location: some(&d.location),
            certify: d.certify,
            protected_pages: match d.protection {
                SignProtection::ExistingPages => (0..self.session.get(doc_id).map_or(0, |doc| doc.info.pages.len())).collect(),
                SignProtection::SignedPage => vec![d.page],
                SignProtection::SelectedPages if d.protected_pages.is_empty() => return Err("Select at least one page to protect.".into()),
                SignProtection::SelectedPages => d.protected_pages.clone(),
                _ => Vec::new(),
            },
            allow_insert: true,
            appearance: d.appearance.clone(),
            // The labels the preview shows: in the interface language, or all in English.
            appearance_labels: appearance_labels(crate::i18n::current()).0,
            graphic,
            background: d.background.image()?.map(|image| image.graphic()),
            ..SignOptions::default()
        };
        // Signing saves, as in Acrobat: choose where (a cancelled save cancels signing). The
        // document is signed once the user has chosen.
        let name = self.session.get(doc_id).map(|d| d.name.clone()).unwrap_or_default();
        let folder = self.session.get(doc_id).and_then(|d| d.path.clone());
        let stem = name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string();
        let sign_and_save = move |app: &mut Self, ctx: &egui::Context, path: PathBuf| -> Result<(), String> {
            // Sign what was typed while the save panel was open, too (#166); a refused value
            // has said why already.
            if !app.commit_typing_in(ctx, doc_id) {
                return Err(String::new());
            }
            if let Some(url) = timestamp {
                return app.start_timestamp(doc_id, path, Some((id, opts, choice, background)), &url);
            }
            // The form's preSign step (run when signing began) is signed with it; if signing fails
            // the step stays pending, and is taken back if signing ends without a signature.
            let field = opts.field.clone();
            let pending = app.presign.take();
            let signed = match app.session.sign(doc_id, &id, opts) {
                Ok(s) => s,
                Err(e) => {
                    app.presign = pending;
                    return Err(e.to_string());
                }
            };
            if let Err(e) = craft::atomic::atomic_write(&path, signed.as_slice()) {
                // Not saved, so not signed: the preSign step stays pending as well.
                app.presign = pending;
                return Err(format!("Could not save: {e}"));
            }
            // Our own write is not a change on disk to reload (#431).
            #[cfg(not(target_arch = "wasm32"))]
            app.watch.known_now(doc_id, &path);
            app.session.mark_signed(doc_id, signed, Some(path.to_string_lossy().into_owned())).map_err(|e| e.to_string())?;
            // Then its postSign scripts; what they change is a change after signing, to save.
            // What they say shows once signing is done, after the notice that it was saved.
            // The file is signed and saved by now: a postSign step that fails is said, not a failure.
            let after = field.as_deref().and_then(|f| app.session.run_sign_event(doc_id, f, pdfcraft_engine::js::SignEvent::After).err());
            let said = app.session.take_js_output(doc_id);
            #[cfg(target_os = "macos")]
            crate::macos_association::repair(&path);
            app.sign_graphic = choice;
            app.sign_background = background;
            if let Some(view) = app.views.iter_mut().find(|v| v.id == doc_id) {
                view.invalidate_content();
            }
            app.set_right(Some(crate::RightPanel::Signatures));
            app.notify_fmt("Signed and saved to {path}", &[("path", &path.display().to_string())]);
            app.handle_js(ctx, doc_id, said);
            if let Some(e) = after {
                app.notify_fmt("Signed and saved, but the form's postSign script couldn't finish: {e}", &[("e", &e.to_string())]);
            }
            Ok(())
        };
        // The signature's page, rectangle and field refer to the document as it is now:
        // the pick is dropped (with a notice) if it is edited or switched away from
        // meanwhile. An error after the pick arrives as a notice.
        let mut request = PickRequest::save(FilePurpose::SignedCopy.key()).file_name(format!("{stem}_signed.pdf")).filter("PDF", &["pdf"]);
        if let Some(parent) = folder.as_deref().and_then(|p| std::path::Path::new(p).parent()) {
            request = request.default_directory(parent);
        }
        let target = self.pick_target(Some(doc_id));
        self.pick_first(request, target, move |app: &mut Self, ctx: &egui::Context, path: PathBuf| {
            if let Err(e) = sign_and_save(app, ctx, path)
                && !e.is_empty()
            {
                app.notify(e);
            }
        });
        Ok(())
    }

    /// The saved signature or image `g` stands for (`None`: the name).
    fn graphic_sig(&self, g: &SignGraphic) -> Option<SavedSignature> {
        graphic_of(self.signature.as_ref(), g)
    }

    /// Sign as ▸ Choose image…: answered only into the signing dialog that asked.
    pub(crate) fn pick_sign_graphic_image(&mut self) {
        let request = PickRequest::open(FilePurpose::SignGraphic.key()).filter(tl!("Image"), &["png", "jpg", "jpeg"]);
        self.pick_first(request, None, |app: &mut Self, _: &egui::Context, path: PathBuf| {
            if app.sign_draft.is_none() {
                return;
            }
            // Scanned paper becomes transparent, as for Fill & Sign image signatures.
            let image = std::fs::File::open(path)
                .map_err(pdfcraft_engine::signature_image::SignatureImageError::from)
                .and_then(pdfcraft_engine::SignatureImage::read)
                .map(|read| {
                    let [w, h] = read.size();
                    SignatureImage::new(w as u32, h as u32, read.rgba().to_vec())
                        .remove_white_background(craft::review::signature::PAPER_CUTOFF, craft::review::signature::PAPER_FEATHER)
                });
            match (image, app.sign_draft.as_mut()) {
                (Ok(image), Some(d)) => {
                    if background {
                        d.background = SignBackground::Image(image);
                    } else {
                        d.graphic = SignGraphic::Image(image.clone());
                        d.image = Some(image);
                    }
                    d.error = None;
                }
                (Ok(_), None) => {}
                (Err(e), _) => app.notify_fmt("Couldn't import the signature image: {e}", &[("e", &e.to_string())]),
            }
        });
    }

    /// Trust a certificate (Signatures panel ▸ Add to trusted certificates), revalidating.
    pub fn trust_certificate(&mut self, cert: Certificate) {
        let mut certs = self.session.trusted_certificates().to_vec();
        if !certs.iter().any(|c| c.raw == cert.raw) {
            certs.push(cert);
        }
        self.apply_trusted_certificates(certs);
    }

    /// Replace the trust store and redraw from the rebuilt display copy. One helper
    /// so no caller can revalidate without invalidating: trust changes verdicts,
    /// verdicts choose what the display copy shows, and the old rasters would
    /// otherwise linger while the badge already follows the new verdict.
    /// Trust alone is not an edit: nothing becomes dirty.
    pub fn apply_trusted_certificates(&mut self, certs: Vec<Certificate>) {
        self.session.set_trusted_certificates(certs);
        for view in &mut self.views {
            view.invalidate_content();
        }
    }

    /// Draw signature status captions in `lang` ([`status_captions`]) and redraw the pages, once
    /// per language change: the session rebuilds the documents whose captions it draws.
    pub(crate) fn apply_status_captions(&mut self, lang: crate::i18n::Lang) {
        if self.status_captions_lang == Some(lang) {
            return;
        }
        self.status_captions_lang = Some(lang);
        // Only a document that draws a signature state shows the words: the others keep their
        // pages, selection and search results.
        let drawing: Vec<pdfcraft_engine::DocId> = self
            .views
            .iter()
            .map(|v| v.id)
            .filter(|id| self.session.get(*id).is_some_and(|d| d.signatures.iter().any(|s| s.verdict_shown)))
            .collect();
        if !self.session.set_signature_captions(status_captions(lang)) {
            return;
        }
        for view in self.views.iter_mut().filter(|v| drawing.contains(&v.id)) {
            view.invalidate_content();
        }
    }

    /// A file name without its PDF extension, for snapshot tab names.
    fn file_stem(name: &str) -> String {
        name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string()
    }

    /// Open saved revision `n` (1 = the oldest) of the active document as a new document.
    /// A repeat reuses the tab: every click used to open another identical one.
    pub fn open_revision(&mut self, n: usize) {
        let Some((_, id)) = self.active_ids() else { return };
        let Some(doc) = self.session.get(id) else { return };
        // The revision, as `Session::open_revision` reads it. An unknown `n` skips the lookup:
        // the whole file could otherwise match the latest revision's tab instead of erroring.
        let ends = doc.revision_ends();
        if let Some(&end) = n.checked_sub(1).and_then(|k| ends.get(k))
            && let Ok(revision) = doc.file_prefix(end)
            && let Some(index) = self.find_snapshot(SnapshotKind::Revision, &doc.name, &revision)
        {
            self.active = Some(index);
            return;
        }
        let from = doc.name.clone();
        match self.session.open_revision(id, n) {
            Ok(new) => {
                // The reading-colours preference is app-wide: the newly opened revision starts with it.
                self.session.set_reading_colors(new, self.reading_colors);
                let Some(doc) = self.session.get(new) else { return };
                let mut view = DocView::new(new, &doc.info, self.view_defaults, self.highlight_fields);
                if self.full_screen() {
                    view.enter_full_screen(self.full_screen_fill);
                }
                view.snapshot = Some(Snapshot { kind: SnapshotKind::Revision, revision: n, from });
                self.views.push(view);
                self.active = Some(self.views.len() - 1);
            }
            Err(e) => self.notify_fmt("Couldn't open revision {n}: {e}", &[("n", &n.to_string()), ("e", &e.to_string())]),
        }
    }

    /// A tab already showing these exact snapshot bytes: its index, so a repeat
    /// switches to it instead of opening another identical tab. An exact match on
    /// the source document wins, so the banner names the file opened from; a
    /// bytes-only match still reuses the tab (incremental saves append, so one
    /// file's revision can be byte-identical to another tab's, and a snapshot's
    /// own repeats name the snapshot itself).
    fn find_snapshot(&self, kind: SnapshotKind, from: &str, bytes: &[u8]) -> Option<usize> {
        let mut fallback = None;
        for (i, v) in self.views.iter().enumerate() {
            let same_kind = v.snapshot.as_ref().is_some_and(|s| s.kind == kind);
            let same_bytes = same_kind && self.session.get(v.id).is_some_and(|d| d.file_bytes().is_ok_and(|f| f.as_slice() == bytes));
            if !same_bytes {
                continue;
            }
            if v.snapshot.as_ref().is_some_and(|s| s.from == from) {
                return Some(i);
            }
            fallback.get_or_insert(i);
        }
        fallback
    }

    /// Open the signed version of a signature as a new document (View signed version).
    /// A repeat reuses the tab: every click used to open another identical one.
    pub fn view_signed_version(&mut self, len: usize, revision: usize) {
        let Some((_, id)) = self.active_ids() else { return };
        let Some(doc) = self.session.get(id) else { return };
        // The file as signed: a certificate-protected document's `bytes` are a decrypted rewrite,
        // which a signature's byte range does not describe.
        let bytes = match doc.file_prefix(len) {
            Ok(bytes) => bytes,
            Err(e) => {
                let e = e.to_string();
                self.notify_fmt("Couldn't open the signed version: {e}", &[("e", &e)]);
                return;
            }
        };
        if let Some(index) = self.find_snapshot(SnapshotKind::SignedVersion, &doc.name, &bytes) {
            self.active = Some(index);
            return;
        }
        let from = doc.name.clone();
        let name = format!("{} (signed version)", Self::file_stem(&from));
        match self.session.open(format!("{name}.pdf"), None, bytes, doc.password.clone().as_deref()) {
            Ok(new) => {
                // The reading-colours preference is app-wide: the newly opened copy starts with it.
                self.session.set_reading_colors(new, self.reading_colors);
                let Some(doc) = self.session.get(new) else { return };
                let mut view = DocView::new(new, &doc.info, self.view_defaults, self.highlight_fields);
                if self.full_screen() {
                    view.enter_full_screen(self.full_screen_fill);
                }
                view.snapshot = Some(Snapshot { kind: SnapshotKind::SignedVersion, revision, from });
                self.views.push(view);
                self.active = Some(self.views.len() - 1);
            }
            Err(e) => self.notify_fmt("Couldn't open the signed version: {e}", &[("e", &e.to_string())]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh folder per call: tests run in parallel.
    fn scratch() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("pdfcraft-sign-ui-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Create a digital ID named "Grace Hopper" in `dir`, as Configure New Digital ID ▸ Create
    /// does. Returns where it was saved.
    fn create_in(dir: &std::path::Path) -> Result<PathBuf, String> {
        let mut app = PdfCraftApp::for_unit_tests();
        let mut draft = SignDraft::new(0, None, None, None, &[]);
        // P-256 keeps the test fast.
        let key = KEY_ALGORITHMS.iter().position(|k| k.1 == "p256").unwrap();
        draft.new_id.name = "Grace Hopper".into();
        draft.new_id.key = key;
        draft.new_id.password = "secret12".into();
        draft.new_id.confirm = "secret12".into();
        app.sign_draft = Some(draft);
        let i = app.create_digital_id_in(Some(dir.to_path_buf()))?;
        Ok(PathBuf::from(&app.digital_ids[i].path))
    }

    #[test]
    fn an_exported_certificate_is_written_inside_the_chosen_folder() {
        use craft::host::{AppInfo, Host, Picked, ScriptedFiles, Wake};
        use craft::platform::clipboard::{Clipboard, EventClipboard};
        use craft::platform::open::FileIntake;
        use craft::platform::watch::FileWatch;
        use std::sync::Arc;

        let out = PathBuf::from("/scripted/out");
        let files = Arc::new(ScriptedFiles::answering([Picked::Paths(vec![out.clone()])]));
        let host = Host::builder(AppInfo { id: "pdfcraft", name: "PdfCraft", version: "0" }).files(files.clone()).build();
        let (intake, _) = FileIntake::new(Wake::none());
        let clipboard = Clipboard::new(EventClipboard::new(|| 0, |_| Ok(0)));
        let services = crate::AppServices::new(Wake::none(), clipboard, None, None, intake, FileWatch::new(Wake::none()).inline());
        let mut app = PdfCraftApp::with_host(Arc::new(host), services);
        let key = PrivateKey::generate_p256().unwrap();
        let cert = Certificate::self_signed(&Name::build("../x", "", "", "", ""), &key, sign::Time::from_unix(0), 1, &[1]).unwrap();
        app.export_certificate(&cert);
        app.process_picks(&egui::Context::default());
        let pem = String::from_utf8(files.file(&out.join(".._x.cer")).unwrap()).unwrap();
        assert!(pem.starts_with("-----BEGIN CERTIFICATE-----"), "{pem}");
    }

    #[test]
    fn a_language_change_leaves_documents_without_a_drawn_state_alone() {
        if !crate::i18n::needs_catalogs() {
            return;
        }
        let mut app = PdfCraftApp::for_unit_tests();
        let plain = app.session.create_from_text("t", "No signatures here.").unwrap();
        app.open_bytes(&egui::Context::default(), "plain.pdf", None, plain.as_ref().clone()).unwrap();
        app.views[0].session.texts.insert(0, std::sync::Arc::new(pdfcraft_render::PageText::default()));
        app.apply_status_captions(crate::i18n::Lang::from_code("de").unwrap());
        assert!(!app.views[0].session.texts.is_empty(), "its text, selection and search results stay");
    }

    #[test]
    fn status_captions_come_whole_from_one_language() {
        // All three in a language that has them, else all three in English: never a mix. Whether
        // a face draws them is the sign crate's call.
        use crate::i18n::{Lang, has};
        let english = StatusCaptions::default();
        assert_eq!(status_captions(Lang::EN), english);
        for lang in Lang::all().filter(|&l| l != Lang::EN) {
            let whole = [&english.valid, &english.unknown, &english.invalid].into_iter().all(|s| has(lang, s));
            let said = status_captions(lang);
            assert_eq!(said != english, whole, "{}: {said:?}", lang.code());
        }
    }

    #[test]
    fn status_captions_reach_the_sign_crate_in_logical_order() {
        // The sign crate receives the catalogue's logical order and shapes the words itself, so
        // they are never reversed a second time.
        if !crate::i18n::needs_catalogs() {
            return;
        }
        let ar = status_captions(crate::i18n::Lang::from_code("ar").unwrap());
        assert_eq!([ar.valid.as_str(), ar.unknown.as_str(), ar.invalid.as_str()], ["التوقيع صالح", "الصلاحية غير معروفة", "التوقيع غير صالح"]);
    }

    #[test]
    fn a_new_digital_id_skips_names_already_taken_without_writing_through_them() {
        let dir = scratch();
        // Another file's hard link at the first name, and a folder at the second (Windows refuses
        // `create_new` on a folder with "access denied" rather than "already exists").
        std::fs::write(dir.join("victim.txt"), "keep me").unwrap();
        std::fs::hard_link(dir.join("victim.txt"), dir.join("Grace Hopper.p12")).unwrap();
        std::fs::create_dir(dir.join("Grace Hopper 2.p12")).unwrap();
        let saved = create_in(&dir).unwrap();
        assert_eq!(saved, dir.join("Grace Hopper 3.p12"));
        assert_eq!(std::fs::read_to_string(dir.join("victim.txt")).unwrap(), "keep me");
        assert!(sign::pkcs12::open(&std::fs::read(&saved).unwrap(), "secret12").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_new_digital_id_is_not_written_through_a_dangling_link() {
        // `exists()` follows links, so a link to a file that doesn't exist yet looked free, and
        // `fs::write` then created the private key wherever the link pointed.
        let dir = scratch();
        let (link, target) = (dir.join("Grace Hopper.p12"), dir.join("elsewhere.p12"));
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&target, &link);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&target, &link);
        #[cfg(not(any(unix, windows)))]
        let made: std::io::Result<()> = {
            let _ = (&target, &link);
            Err(std::io::ErrorKind::Unsupported.into())
        };
        if let Err(e) = made {
            // Windows needs Developer Mode (or admin) for symlinks.
            eprintln!("skipped: can't create a symlink here: {e}");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        let saved = create_in(&dir).unwrap();
        assert!(!target.exists(), "nothing written through the link");
        assert_eq!(saved, dir.join("Grace Hopper 2.p12"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_new_digital_id_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch();
        let saved = create_in(&dir).unwrap();
        let mode = std::fs::metadata(&saved).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "the private key is not readable by others: {mode:o}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn listed(path: &str, name: &str) -> DigitalIdEntry {
        DigitalIdEntry {
            fingerprint: String::new(),
            path: path.into(),
            name: name.into(),
            issuer: String::new(),
            email: String::new(),
            expires: String::new(),
            unusable: None,
        }
    }

    /// Issue #1396: an imported ID can be taken off the list (its file stays), the dialog's
    /// selection follows, an OS key store ID or a stale index is refused, and the removal is saved.
    #[test]
    fn a_digital_id_file_is_removed_from_the_list_and_stays_removed() {
        let dir = scratch();
        let file = dir.join("old.p12");
        std::fs::write(&file, b"not opened").unwrap();
        let mut app = PdfCraftApp::for_unit_tests();
        app.digital_ids = vec![listed("windows:abc", "Store"), listed(&file.to_string_lossy(), "Old"), listed("/ids/new.p12", "New")];
        let mut draft = SignDraft::new(0, None, None, None, &app.digital_ids);
        draft.selected = Some(2);
        draft.confirm_remove = Some(1);
        app.sign_draft = Some(draft);

        assert!(!app.remove_digital_id(0), "OS key store IDs stay");
        assert!(!app.remove_digital_id(9), "an index past the list is refused");
        assert!(app.remove_digital_id(1));
        assert_eq!(app.digital_ids.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["Store", "New"]);
        assert!(file.exists(), "the file itself is not deleted");
        let d = app.sign_draft.as_ref().unwrap();
        assert_eq!((d.selected, d.confirm_remove, d.step), (Some(1), None, SignStep::Choose), "still New, no pending question");

        // Removing the selected ID selects the first usable one; removing the last file ID
        // leaves only the store ID, which isn't saved.
        assert!(app.remove_digital_id(1));
        assert_eq!(app.sign_draft.as_ref().unwrap().selected, Some(0));
        let mut fresh = PdfCraftApp::for_unit_tests();
        fresh.apply_settings(&app.settings_snapshot());
        assert!(fresh.digital_ids.is_empty(), "the removal survives a restart: {:?}", fresh.digital_ids);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The labels a signature is stamped with are all in the interface language, or all in
    /// English when the signature's font can't draw them there (or the language is right to
    /// left): never a mix, and never a label signing would refuse.
    #[test]
    fn appearance_labels_are_translated_or_all_english() {
        if !crate::i18n::needs_catalogs() {
            return;
        }
        use crate::i18n::{Lang, has, tr};
        let english = AppearanceLabels::default();
        assert_eq!(appearance_labels(Lang::EN), (english.clone(), false));
        // Latin-script catalogs the signature's font can draw: stamped in the language once every
        // label is translated there.
        for code in ["de", "es", "fr", "it", "pt-br", "pl", "tr", "sl", "hu"] {
            let lang = Lang::from_code(code).unwrap();
            if !english.fields().into_iter().all(|(_, source)| has(lang, source)) {
                continue;
            }
            let (labels, in_english) = appearance_labels(lang);
            assert!(!in_english, "{code}: {labels:?}");
            for ((field, ours), (_, source)) in labels.fields().into_iter().zip(english.fields()) {
                assert_eq!(ours, tr(lang, source), "{code}: {field}");
            }
        }
        for code in ["zh-hans", "zh-hant", "ja", "ru", "uk", "bg", "kk", "el", "te", "cs", "lt", "ar"] {
            assert_eq!(appearance_labels(Lang::from_code(code).unwrap()), (english.clone(), true), "{code}");
        }
        for lang in Lang::all() {
            let (labels, in_english) = appearance_labels(lang);
            assert_eq!(labels.first_undrawable(), None, "{}: signing would refuse {labels:?}", lang.code());
            assert_eq!(in_english, lang != Lang::EN && labels == english, "{}", lang.code());
        }
    }

    /// The preview's details are the stamp's: the label above the name, then each detail after
    /// its label, in the words given, and only the values when the labels are off.
    #[test]
    fn preview_lines_use_the_labels_signing_stamps() {
        if !crate::i18n::needs_catalogs() {
            return;
        }
        let (de, _) = appearance_labels(crate::i18n::Lang::from_code("de").unwrap());
        let all = Appearance { reason: true, location: true, ..Appearance::default() };
        let lines = detail_lines(&all, &de, "Ada", " Genehmigt ", "Berlin", "2026.10.14 09:30:00 Z");
        assert_eq!(lines, ["Digital signiert von", "Ada", "Grund: Genehmigt", "Ort: Berlin", "Datum: 2026.10.14 09:30:00 Z"]);
        let bare = Appearance { labels: false, ..all.clone() };
        assert_eq!(detail_lines(&bare, &de, "Ada", "Genehmigt", "", "D"), ["Ada", "Genehmigt", "D"]);
        let nothing = Appearance { name: false, date: false, ..all };
        assert!(detail_lines(&nothing, &de, "Ada", "  ", "", "D").is_empty());
    }

    fn signature(certify: Option<u8>, status: SignatureStatus) -> SignatureInfo {
        SignatureInfo {
            field: "Signature".into(),
            signed: true,
            page: None,
            rect: None,
            visible: false,
            signer: None,
            certificate: None,
            chain: Vec::new(),
            date: None,
            signing_time: None,
            reason: None,
            location: None,
            contact: None,
            sub_filter: None,
            doc_timestamp: false,
            certify,
            protection: None,
            integrity_valid: false,
            revision: 1,
            signed_len: 0,
            digest: None,
            algorithm: None,
            timestamp: false,
            timestamp_time: None,
            status,
            modification: sign::Modification::None,
            verdict_shown: false,
            details: Vec::new(),
        }
    }

    /// A valid certification doesn't hide a later signature that is invalid or can't be checked:
    /// the message bar names the problem, and says "Certified by" only when all are valid.
    #[test]
    fn the_message_bar_puts_a_signature_problem_before_the_certification() {
        use SignatureStatus::{Invalid, Unknown, Valid};
        let bar = |sigs: &[SignatureInfo]| banner(sigs).map(|b| (b.0, b.2));
        let certified = signature(Some(2), Valid);
        assert_eq!(bar(std::slice::from_ref(&certified)), Some(("badge-check", "Certified by {by}")));
        assert_eq!(bar(&[certified.clone(), signature(None, Valid)]), Some(("badge-check", "Certified by {by}")));
        assert_eq!(bar(&[certified.clone(), signature(None, Invalid)]), Some(("circle-x", "At least one signature is invalid.")));
        assert_eq!(bar(&[certified, signature(None, Unknown)]), Some(("triangle-alert", "At least one signature has problems.")));
        assert_eq!(bar(&[signature(Some(2), Unknown), signature(None, Valid)]), Some(("triangle-alert", "At least one signature has problems.")));
        assert_eq!(bar(&[signature(None, Valid)]), Some(("circle-check", "Signed and all signatures are valid.")));
        assert_eq!(bar(&[SignatureInfo { signed: false, ..signature(None, Invalid) }]), None);
    }

    #[test]
    fn dismissing_or_stepping_back_in_the_sign_dialog_clears_passwords() {
        use egui_kittest::{Harness, kittest::Queryable};

        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(|cc| {
            crate::PdfCraftApp::install_runtime(&cc.egui_ctx);
            {
                let mut app = PdfCraftApp::for_unit_tests();
                app.set_option(&cc.egui_ctx, "language", "en").unwrap();
                app.digital_ids = vec![listed("/ids/grace.p12", "Grace Hopper")];
                let mut draft = SignDraft::new(0, None, None, None, &app.digital_ids);
                draft.step = SignStep::SignAs;
                draft.password = "secret12".into();
                app.sign_draft = Some(draft);
                app
            }
        });
        h.run_steps(2);

        // Stepping back from SignAs to Choose clears the typed digital ID password.
        h.get_by_label("Back").click();
        h.run_steps(2);
        assert_eq!(h.state().sign_draft.as_ref().unwrap().step, SignStep::Choose);
        assert!(h.state().sign_draft.as_ref().unwrap().password.is_empty(), "Back from SignAs must clear password");

        // Stepping back from Configure to Choose resets the new-ID password fields.
        {
            let d = h.state_mut().sign_draft.as_mut().unwrap();
            d.step = SignStep::Configure;
            d.new_id.password = "newsecret12".into();
            d.new_id.confirm = "newsecret12".into();
            d.new_id.file_password = "filesecret12".into();
        }
        h.run_steps(2);
        h.get_by_label("Back").click();
        h.run_steps(2);
        let d = h.state().sign_draft.as_ref().unwrap();
        assert_eq!(d.step, SignStep::Choose);
        assert!(d.new_id.password.is_empty() && d.new_id.confirm.is_empty() && d.new_id.file_password.is_empty());

        // Dismissing the modal with Escape clears sign_draft completely.
        h.state_mut().sign_draft.as_mut().unwrap().password = "secret12".into();
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(!h.state().modal_open());
        assert!(h.state().sign_draft.is_none(), "sign_draft must be cleared when the sign dialog is dismissed");

        // Importing a digital ID file and signing work by borrowing the draft in place.
        let dir = scratch();
        let saved = create_in(&dir).unwrap();
        let out_pdf = dir.join("signed.pdf");
        let files = std::sync::Arc::new(craft::host::ScriptedFiles::answering([craft::host::Picked::Paths(vec![out_pdf.clone()])]));
        let host = craft::host::Host::builder(craft::host::AppInfo { id: "pdfcraft", name: "PdfCraft", version: "0" }).files(files).build();
        let (intake, _) = craft::platform::open::FileIntake::new(craft::host::Wake::none());
        let clipboard = craft::platform::clipboard::Clipboard::new(craft::platform::clipboard::EventClipboard::new(|| 0, |_| Ok(0)));
        let watch = craft::platform::watch::FileWatch::new(craft::host::Wake::none()).inline();
        let services = crate::AppServices::new(craft::host::Wake::none(), clipboard, None, None, intake, watch);
        let mut import_app = PdfCraftApp::with_host(std::sync::Arc::new(host), services);
        let mut import_draft = SignDraft::new(0, None, None, None, &[]);
        import_draft.new_id.create = false;
        import_draft.new_id.file = saved.to_string_lossy().into_owned();
        import_draft.new_id.file_password = "secret12".into();
        import_app.sign_draft = Some(import_draft);
        assert_eq!(import_app.import_digital_id(), Ok(0));
        assert_eq!(import_app.digital_ids[0].name, "Grace Hopper");

        let bytes = pdfcraft_engine::Session::new().create_blank(200.0, 200.0, 1).unwrap();
        import_app.open_bytes(&egui::Context::default(), "unsigned.pdf", None, bytes.as_ref().clone()).unwrap();
        let mut sign_draft = SignDraft::new(0, None, None, None, &import_app.digital_ids);
        sign_draft.step = SignStep::SignAs;
        sign_draft.password = "secret12".into();
        import_app.sign_draft = Some(sign_draft);
        assert_eq!(import_app.finish_signing(&egui::Context::default()), Ok(()));
        import_app.process_picks(&egui::Context::default());
        assert!(out_pdf.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum SignBackground {
    #[default]
    Lion,
    None,
    Image(#[serde(with = "crate::fill_sign::image_data")] SignatureImage),
}

impl SignBackground {
    fn image(&self) -> Result<Option<SignatureImage>, String> {
        match self {
            Self::Lion => SignatureImage::read(std::io::Cursor::new(crate::app_icon::png(false))).map(Some).map_err(|e| e.to_string()),
            Self::None => Ok(None),
            Self::Image(image) => Ok(Some(image.clone())),
        }
    }
}

/// The signing dialogs' state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignProtection {
    None,
    NoChanges,
    Forms,
    Comments,
    ExistingPages,
    SignedPage,
    SelectedPages,
}

impl SignProtection {
    fn label(self) -> &'static str {
        match self {
            Self::None => "No additional restrictions",
            Self::NoChanges => "Lock the entire document (PDF certification)",
            Self::Forms => "Allow forms and signatures (PDF certification)",
            Self::Comments => "Allow comments, forms and signatures (PDF certification)",
            Self::ExistingPages => "Protect existing pages; allow adding pages (PdfCraft)",
            Self::SignedPage => "Protect this signature's page (PdfCraft)",
            Self::SelectedPages => "Protect selected pages (PdfCraft)",
        }
    }
}

impl PdfCraftApp {
    pub(crate) fn choose_sign_pages(&mut self) {
        let Some((index, id)) = self.active_ids() else { return };
        let Some(draft) = &mut self.sign_draft else { return };
        draft.choosing_pages = Some(id);
        if let Some(view) = self.views.get_mut(index) {
            view.open_organize();
            view.select_pages(&draft.protected_pages);
            view.organize_pick = true;
            view.org_drag = None;
        }
        self.left = crate::LeftPanel::Tool("organize");
        self.left_open = true;
    }
}

impl PdfCraftApp {
    pub(crate) fn finish_sign_page_choice(&mut self, accept: bool) {
        let Some((index, id)) = self.active_ids() else { return };
        let Some(draft) = self.sign_draft.as_mut().filter(|d| d.choosing_pages == Some(id)) else { return };
        if let Some(view) = self.views.get_mut(index) {
            if accept {
                draft.protected_pages = view.selected.iter().copied().collect();
                draft.protected_pages.sort_unstable();
            }
            view.organize = false;
            view.organize_pick = false;
        }
        draft.choosing_pages = None;
        self.left = crate::organize_panel::workspace_panel(self.mode);
        self.dialog = Some(crate::Dialog::Sign);
    }
}

/// The signature as it will look: the graphic (or the name) on the left, the details beside it
/// (`lines`, from [`detail_lines`] with the labels signing will stamp).
type GraphicPreview = Option<(SavedSig, egui::TextureHandle)>;

/// Remove Signature (Acrobat: Clear Signature): the document's signatures that can be listed,
/// the one chosen, and whether its field stays. Nothing changes until a new file is chosen.
#[derive(Clone)]
pub struct RemoveSigDraft {
    pub doc: DocId,
    pub items: Vec<Removable>,
    pub selected: Option<usize>,
    /// Keep an empty signature field to sign again, instead of removing the field.
    pub keep_field: bool,
    /// Only for a file-based ID. Card PINs are handled exclusively by the OS provider.
    pub password: String,
}

impl std::fmt::Debug for RemoveSigDraft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoveSigDraft")
            .field("doc", &self.doc)
            .field("items", &self.items)
            .field("selected", &self.selected)
            .field("keep_field", &self.keep_field)
            .finish_non_exhaustive()
    }
}

impl RemoveSigDraft {
    fn chosen(&self) -> Option<&Removable> {
        self.selected.and_then(|i| self.items.get(i)).filter(|r| r.blocked.is_none())
    }
}

fn kind_label(r: &Removable) -> String {
    match r.kind {
        RemoveKind::Digital => tl!("Digital signature").to_string(),
        RemoveKind::Certification(p) => crate::i18n::fmt(tl!("Certification signature (level {p})"), &[("p", &p.to_string())]),
        RemoveKind::DocumentTimestamp => tl!("Document timestamp").to_string(),
        RemoveKind::Visual { initials: false } => tl!("Visual signature (Fill & Sign, not digital)").to_string(),
        RemoveKind::Visual { initials: true } => tl!("Visual initials (Fill & Sign, not digital)").to_string(),
    }
}

fn said(wording: (&'static str, String)) -> String {
    crate::i18n::fmt(tl!(wording.0), &[("names", &wording.1)])
}

impl PdfCraftApp {
    /// The page menu clears only a signature matching an available signing identity.
    pub(crate) fn start_remove_own_signature(&mut self, field: &str) {
        let cert = self
            .active_ids()
            .and_then(|(_, id)| self.session.get(id))
            .and_then(|d| d.signatures.iter().find(|s| s.field == field && s.signed && !s.doc_timestamp))
            .and_then(|s| s.certificate.clone());
        self.refresh_os_key_store_ids();
        let owned = cert.is_some_and(|cert| self.digital_ids.iter().any(|id| id.unusable.is_none() && id.fingerprint == cert.fingerprint()));
        if owned {
            self.start_remove_signature(Some(field));
        } else {
            self.notify("The matching signing identity is unavailable. Insert your card or add the matching digital ID.");
        }
    }
}

impl PdfCraftApp {
    /// Open Remove Signature for the active document, with `field` chosen when given.
    pub fn start_remove_signature(&mut self, field: Option<&str>) {
        let Some((_, id)) = self.active_ids() else { return };
        self.refresh_os_key_store_ids();
        match self.session.removable_signatures(id) {
            Ok(items) => {
                let selected = field
                    .and_then(|f| items.iter().position(|r| r.field.as_deref() == Some(f)))
                    .or_else(|| items.iter().position(|r| r.blocked.is_none()));
                self.remove_sig = Some(RemoveSigDraft { doc: id, items, selected, keep_field: false, password: String::new() });
                self.dialog = Some(crate::Dialog::RemoveSignature);
            }
            Err(e) => self.notify_error(e),
        }
    }
}

impl PdfCraftApp {
    /// The dialog's Remove and Save As: ask where the new version goes (never over the
    /// original), then remove and save there.
    fn confirm_remove_signature(&mut self) {
        let Some(draft) = self.remove_sig.take() else { return };
        let Some(chosen) = draft.chosen().cloned() else { return };
        let Some(doc) = self.session.get(draft.doc) else { return };
        let stem = std::path::Path::new(&doc.name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
        let keep_field = draft.keep_field;
        let password = draft.password;
        let id = draft.doc;
        #[cfg(not(target_arch = "wasm32"))]
        {
            // Tests and scripts choose the destination ahead, as for Save As.
            if let Some(dest) = self.save_override.clone() {
                self.remove_signature_to(id, &chosen, keep_field, &dest, &password);
                return;
            }
            let name = crate::i18n::fmt(tl!("{name}_unsigned.pdf"), &[("name", &stem)]);
            let dialog = rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(name);
            self.ask_one(crate::pickers::Ask::Save(dialog), Some(id), move |app, dest| {
                app.remove_signature_to(id, &chosen, keep_field, &dest.to_string_lossy(), &password);
            });
        }
        // The browser saves by downloading, with nowhere to choose first: remove, then download,
        // and undo the removal if the download fails.
        #[cfg(target_arch = "wasm32")]
        {
            let _ = stem;
            if matches!(chosen.target, RemoveTarget::Field(_)) {
                let _ = password;
                self.notify("Digital signature removal needs the matching private key on a desktop build. The signed document was kept.");
                return;
            }
            let mark = match self.apply_removal(id, &chosen, keep_field) {
                Ok(mark) => mark,
                Err(e) => {
                    self.notify_fmt("The signature was not removed: {e}", &[("e", &e)]);
                    return;
                }
            };
            let saved = self.views.iter().position(|v| v.id == id).is_some_and(|index| self.save_view(index, crate::SaveTarget::As));
            if !saved {
                self.undo_back_to(id, mark);
            }
        }
    }
}

impl PdfCraftApp {
    /// Remove `chosen` from document `id` (undoable) and check that it is gone; if it isn't,
    /// undo it again. Returns the edit position before the removal, for [`Self::undo_back_to`].
    fn apply_removal(&mut self, id: DocId, chosen: &Removable, keep_field: bool) -> Result<u64, String> {
        let visuals =
            |app: &Self| app.session.removable_signatures(id).map(|l| l.iter().filter(|r| matches!(r.kind, RemoveKind::Visual { .. })).count());
        let before = visuals(self).map_err(|e| e.to_string())?;
        let edit = match &chosen.target {
            RemoveTarget::Field(f) => pdfcraft_engine::Edit::ClearSignature { field: f.clone(), remove_field: !keep_field },
            RemoveTarget::Annotation { page, index } => pdfcraft_engine::Edit::DeleteAnnotation { page: *page, index: *index },
            RemoveTarget::None => return Err(tl!("this signature can't be removed on its own").to_string()),
        };
        let mark = self.session.get(id).and_then(|doc| doc.edit_position()).unwrap_or_default();
        self.session.apply(id, edit).map_err(|e| e.to_string())?;
        let gone = match &chosen.target {
            RemoveTarget::Field(f) => {
                self.session.get(id).is_some_and(|doc| !doc.signatures.iter().any(|s| &s.field == f && (s.signed || !keep_field)))
            }
            _ => visuals(self).is_ok_and(|n| n < before),
        };
        self.refresh_view_of(id);
        if !gone {
            let _ = self.session.undo(id);
            self.refresh_view_of(id);
            return Err(tl!("it was still in the document afterwards, so the change was undone").to_string());
        }
        Ok(mark)
    }
}

impl PdfCraftApp {
    /// Undo edits back to edit position `mark`, noted before the removal: the removal, and
    /// whatever saving added after it (flattening Fill & Sign marks, when that preference is
    /// on), and never an edit the user made before it.
    fn undo_back_to(&mut self, id: DocId, mark: u64) {
        while self.session.get(id).and_then(|doc| doc.edit_position()).is_some_and(|p| p > mark) {
            if self.session.undo(id).is_err() {
                break;
            }
        }
        self.refresh_view_of(id);
    }
}

impl PdfCraftApp {
    /// Remove `chosen` and save the document to `dest`, a file other than its own. When the
    /// save fails the removal is undone, so the document is as it was and nothing is claimed.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn remove_signature_to(&mut self, id: DocId, chosen: &Removable, keep_field: bool, dest: &str, password: &str) {
        let Some(doc) = self.session.get(id) else { return };
        if doc.path.as_deref().is_some_and(|p| crate::same_file(p, dest)) {
            self.notify_tr("Choose another file: the original is kept as it is, and the version without the signature is saved next to it.");
            return;
        }
        if matches!(chosen.target, RemoveTarget::Field(_))
            && let Err(error) = self.verify_removal_identity(id, chosen, password)
        {
            self.notify_fmt("The signature was not removed: {e}", &[("e", &error)]);
            return;
        }
        let mark = match self.apply_removal(id, chosen, keep_field) {
            Ok(mark) => mark,
            Err(e) => {
                self.notify_fmt("The signature was not removed: {e}", &[("e", &e)]);
                return;
            }
        };
        if !self.save_doc_to(id, dest, crate::SaveTarget::As) {
            // The save said why; the document goes back to how it was.
            self.undo_back_to(id, mark);
            return;
        }
        let Some(doc) = self.session.get(id) else { return };
        let file = std::path::Path::new(dest).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dest.to_string());
        let what = match (&chosen.target, keep_field) {
            (RemoveTarget::Annotation { page, .. }, _) => {
                crate::i18n::fmt(tl!("Removed the visual signature on page {p}"), &[("p", &(page + 1).to_string())])
            }
            (_, true) => {
                crate::i18n::fmt(tl!("Removed the signature {field} and kept its empty field"), &[("field", chosen.field.as_deref().unwrap_or(""))])
            }
            (_, false) => crate::i18n::fmt(tl!("Removed the signature {field} with its field"), &[("field", chosen.field.as_deref().unwrap_or(""))]),
        };
        let history = if doc.is_signed() {
            tl!("Other signatures remain, so the removed one is still in the file's earlier revision; check the remaining signatures again.")
        } else {
            tl!("The new file has no signature left and no earlier revision holding it.")
        };
        self.notify(crate::i18n::fmt(
            tl!("{what}. Saved {file}, checked to reopen with every page. {history}"),
            &[("what", &what), ("file", &file), ("history", history)],
        ));
        self.choose_right_panel(Some(crate::RightPanel::Signatures));
    }
}

impl PdfCraftApp {
    #[cfg(not(target_arch = "wasm32"))]
    fn verify_removal_identity(&mut self, doc: DocId, chosen: &Removable, password: &str) -> Result<(), String> {
        use std::io::Read;
        let field = chosen.field.as_deref().ok_or("This signature has no field")?;
        let (certificate, binding) = {
            let doc = self.session.get(doc).ok_or("The document was closed")?;
            let signature = doc.signatures.iter().find(|s| s.signed && s.field == field).ok_or("This signature is no longer in the document")?;
            let certificate = signature.certificate.clone().ok_or("The signature has no certificate to verify ownership against")?;
            let binding = sign::keys::DigestAlg::Sha256.digest(&[&doc.bytes, field.as_bytes()]);
            (certificate, binding)
        };
        self.refresh_os_key_store_ids();
        let reference = self
            .digital_ids
            .iter()
            .find(|id| id.unusable.is_none() && id.fingerprint == certificate.fingerprint())
            .map(|id| id.path.clone())
            .ok_or("The matching signing identity is unavailable. Insert the same CAC/card or add the matching digital ID.")?;
        let identity = if reference.starts_with("windows:") {
            windows_id(&reference)?
        } else if reference.starts_with("keychain:") {
            keychain_id(&reference)?
        } else {
            let mut bytes = Vec::new();
            std::fs::File::open(&reference).map_err(|e| e.to_string())?.take(16_777_217).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() > 16_777_216 {
                return Err("The digital ID file exceeds 16 MiB".into());
            }
            sign::pkcs12::open(&bytes, password).map_err(|e| e.to_string())?
        };
        sign::ownership::prove(&identity, &certificate, &binding).map_err(|e| e.to_string())
    }
}

/// The Remove Signature dialog. Returns `true` when it closes.
pub(crate) fn remove_dialog(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> bool {
    let dirty = app.remove_sig.as_ref().and_then(|d| app.session.get(d.doc)).is_some_and(|d| d.dirty);
    let Some(d) = app.remove_sig.as_mut() else { return true };
    title(ui, tl!("Remove Signature"));
    if d.items.is_empty() {
        ui.label(tl!("This document has no signature that can be removed."));
        ui.add_space(6.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(tl!(
                    "A signature that is part of a page's content, like a scanned page, is not a field or an annotation, so it can't be removed on its own, and PdfCraft doesn't erase parts of a page to hide it. To take it out, redact that area (Redact), or delete the picture with Edit PDF when the signature is an image of its own."
                ))
                .color(t.text_muted),
            )
            .wrap(),
        );
        ui.add_space(12.0);
        let mut close = false;
        crate::widgets::dialog_footer(ui, |ui| close = widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked());
        return close;
    }
    ui.label(tl!("Choose the signature to remove:"));
    ui.add_space(4.0);
    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
        for (i, r) in d.items.iter().enumerate() {
            let mut head = kind_label(r);
            match (r.signer.as_deref(), &r.kind) {
                (Some(signer), _) => head.push_str(&format!(" — {signer}")),
                (None, RemoveKind::Visual { .. }) => {}
                (None, _) => head.push_str(&format!(" — {}", tl!("unknown signer"))),
            }
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
            let enabled = r.blocked.is_none();
            ui.add_enabled_ui(enabled, |ui| {
                if ui.add(widgets::radio(d.selected == Some(i), egui::RichText::new(head).font(theme::semibold(12.5)))).clicked() {
                    d.selected = Some(i);
                }
            });
            ui.indent(("remove-sig-row", i), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(sub.join(" · ")).font(theme::regular(11.5)).color(t.text_muted)).wrap());
                if let Some(b) = &r.blocked {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(said(b.wording()))
                                .font(theme::regular(11.5))
                                .color(signature_status_style(SignatureStatus::Invalid).1),
                        )
                        .wrap(),
                    );
                }
            });
            ui.add_space(4.0);
        }
    });
    let Some(chosen) = d.chosen().cloned() else {
        // Nothing chosen yet: a blocked one was asked for (from its row in the Signatures panel),
        // or none can be removed at all.
        let any = d.items.iter().any(|r| r.blocked.is_none());
        ui.add_space(8.0);
        let text = if any { tl!("Choose a signature that can be removed.") } else { tl!("None of these signatures can be removed.") };
        ui.label(egui::RichText::new(text).color(t.text_muted));
        ui.add_space(12.0);
        let mut close = false;
        crate::widgets::dialog_footer(ui, |ui| {
            close = widgets::dialog_button(
                ui,
                if any { tl!("Cancel") } else { tl!("Close") },
                if any { widgets::DialogAction::Cancel } else { widgets::DialogAction::Back },
            )
            .clicked();
        });
        if close {
            app.remove_sig = None;
        }
        return close;
    };
    ui.add_space(8.0);
    let digital = matches!(chosen.target, RemoveTarget::Field(_));
    if digital {
        ui.add(widgets::checkbox(&mut d.keep_field, tl!("Keep an empty signature field to sign again")));
        let certificate = chosen.field.as_deref().and_then(|field| {
            app.session.get(d.doc).and_then(|doc| doc.signatures.iter().find(|s| s.signed && s.field == field)).and_then(|s| s.certificate.as_ref())
        });
        let file_id = certificate.is_some_and(|cert| {
            app.digital_ids
                .iter()
                .any(|id| id.fingerprint == cert.fingerprint() && !id.path.starts_with("windows:") && !id.path.starts_with("keychain:"))
        });
        if file_id {
            let label = ui.label(tl!("Digital ID password"));
            ui.add(widgets::line(&mut d.password).password(true).desired_width(260.0)).labelled_by(label.id);
        } else {
            ui.label("The matching CAC/card's private key must be available. Windows may ask for its PIN; a cached certificate is not enough.");
        }
        ui.add_space(6.0);
    }
    let others = chosen.effects.iter().any(|e| matches!(e, RemoveEffect::OtherSignatures(_)));
    let warning = match (digital, others) {
        (true, true) => tl!(
            "Removing this signature changes the document and invalidates this signature in the version that results. It can affect the other signatures and certifications. The original file is kept as it is."
        ),
        (true, false) => tl!(
            "Removing this signature changes the document and invalidates this signature in the version that results. The original file is kept as it is."
        ),
        (false, _) => {
            tl!("This is a picture of a signature, not a digital signature: only the mark on the page goes. The original file is kept as it is.")
        }
    };
    ui.add(egui::Label::new(egui::RichText::new(warning).font(theme::semibold(12.0))).wrap());
    for e in &chosen.effects {
        ui.add(egui::Label::new(egui::RichText::new(format!("• {}", said(e.wording()))).font(theme::regular(11.5)).color(t.text_muted)).wrap());
    }
    if dirty {
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("• {}", tl!("Unsaved changes to this document are saved in the new file too.")))
                    .font(theme::regular(11.5))
                    .color(t.text_muted),
            )
            .wrap(),
        );
    }
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(tl!("You choose where the new version is saved. Continue?")).font(theme::regular(12.0))).wrap());
    ui.add_space(12.0);
    let (mut go, mut close) = (false, false);
    crate::widgets::dialog_footer(ui, |ui| {
        go = widgets::dialog_button(ui, tl!("Remove and Save As…"), widgets::DialogAction::Destructive).clicked();
        close = widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked();
    });
    if go {
        app.confirm_remove_signature();
        return true;
    }
    if close {
        app.remove_sig = None;
    }
    close
}

use pdfcraft_engine::clear_signature::{Effect as RemoveEffect, Kind as RemoveKind, Removable, Target as RemoveTarget};

use pdfcraft_engine::DocId;

use dialog::title;
