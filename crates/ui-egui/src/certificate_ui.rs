//! Encrypt with Certificate (Acrobat: Protect ▸ Encrypt with certificate): the document is
//! encrypted for the holders of certificates (ISO 32000-2 §7.6.5, `/Adobe.PubSec`), each of whom
//! opens it with their own private key. Recipients come from certificate files (.cer, .crt,
//! .pem, .der) or from the person's own digital ID files; and the prompt that opens such a
//! document with one of those digital IDs.

use craft::host::PickRequest;
use craft::ui::contract::Bind;
use craft::ui::patterns::form_dialogs::FormDialog;
use craft::ui::sections::{DialogRole, FooterButton};
use pdfcraft_engine::sign::{self, Certificate, DigitalId};
use pdfcraft_engine::{Algorithm, CertificateProtection, CertificateRecipient, Changes, Edit, Printing, Protection};

use crate::PdfCraftApp;

/// One recipient in the dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipient {
    pub name: String,
    pub email: String,
    pub issuer: String,
    pub expires: String,
    /// The certificate, DER.
    pub der: Vec<u8>,
    /// Everything allowed; otherwise the dialog's restrictions apply.
    pub everything: bool,
    /// One of the person's own digital IDs: the saved document opens here with it.
    pub mine: bool,
}

impl Recipient {
    pub fn of(c: &Certificate, mine: bool) -> Self {
        let e = crate::sign_ui::entry_for("", c);
        Recipient { name: e.name, email: e.email, issuer: e.issuer, expires: e.expires, der: c.raw.clone(), everything: mine, mine }
    }
}

/// The dialog's state while it is open.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CertificateDraft {
    pub recipients: Vec<Recipient>,
    /// What recipients without "All permissions" may do, the compatibility and what is encrypted.
    pub protection: Protection,
    /// Adding one of the person's digital ID files: its path and password.
    pub own_id: String,
    pub own_password: String,
    pub error: Option<String>,
}

impl CertificateDraft {
    /// Add a recipient; one already listed (the same certificate) stays once, keeping the more
    /// generous of the two.
    pub fn add(&mut self, r: Recipient) {
        match self.recipients.iter_mut().find(|x| x.der == r.der) {
            Some(x) => {
                x.mine |= r.mine;
                x.everything |= r.everything;
            }
            None => self.recipients.push(r),
        }
    }

    /// Whether one of the recipients is one of the person's digital IDs.
    pub fn includes_me(&self) -> bool {
        self.recipients.iter().any(|r| r.mine)
    }

    /// Why Apply is disabled, if it is.
    pub fn problem(&self) -> Option<&'static str> {
        self.recipients.is_empty().then_some("Add at least one recipient.")
    }

    /// The `/P` bits a recipient without "All permissions" gets.
    pub fn restricted_bits(&self) -> i32 {
        Protection { permissions_password: Some(String::new()), ..self.protection.clone() }.permission_bits()
    }

    /// The edit Apply makes, with a fresh `seed` (`Session::new_seed`).
    pub fn edit(&self, seed: [u8; 20]) -> Edit {
        let restricted = self.restricted_bits();
        Edit::ProtectWithCertificates(CertificateProtection {
            recipients: self
                .recipients
                .iter()
                .map(|r| CertificateRecipient { certificate: r.der.clone(), permissions: if r.everything { -1 } else { restricted } })
                .collect(),
            algorithm: self.protection.algorithm,
            encrypt_metadata: self.protection.encrypt_metadata,
            seed,
        })
    }
}

/// Why a certificate can't be a recipient: the envelopes are RSA key transport only.
fn unusable(c: &Certificate) -> Option<&'static str> {
    (!matches!(c.public_key, sign::PublicKey::Rsa { .. })).then_some("Only certificates with RSA keys can be recipients.")
}

/// Recipients from a certificate file (DER, or PEM with one or more certificates).
pub fn recipients_from_file(path: &std::path::Path) -> Result<Vec<Recipient>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let certs = sign::x509::load_certificates(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if let Some(problem) = certs.iter().find_map(unusable) {
        return Err(problem.to_string());
    }
    Ok(certs.iter().map(|c| Recipient::of(c, false)).collect())
}

/// A digital ID file opened with its password.
pub fn open_digital_id(path: &str, password: &str) -> Result<DigitalId, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    sign::pkcs12::open(&bytes, password).map_err(|e| match e {
        sign::SignError::WrongPassword => "The password is incorrect.".to_string(),
        e => e.to_string(),
    })
}

/// The person's digital IDs that can open an encrypted document: the files (a Keychain or
/// Windows store key never leaves its store, and decrypting through it is not supported yet).
fn id_files(app: &PdfCraftApp) -> Vec<(String, String)> {
    app.digital_ids
        .iter()
        .filter(|e| e.unusable.is_none() && !e.path.starts_with("keychain:") && !e.path.starts_with("windows:"))
        .map(|e| (e.path.clone(), e.name.clone()))
        .collect()
}

impl PdfCraftApp {
    /// Add one of the person's digital ID files as a recipient with every permission; its
    /// identity also joins the session, so the saved document opens here.
    fn add_own_recipient(&mut self, d: &mut CertificateDraft) -> Result<(), String> {
        let (path, password) = (d.own_id.clone(), d.own_password.clone());
        if path.is_empty() {
            return Err("Choose a digital ID.".into());
        }
        let id = open_digital_id(&path, &password)?;
        if let Some(problem) = unusable(&id.certificate) {
            return Err(problem.to_string());
        }
        let cert = id.certificate.clone();
        self.session.add_identity(id);
        self.add_digital_id(&path, &cert);
        d.add(Recipient::of(&cert, true));
        d.own_password.clear();
        Ok(())
    }

    /// Apply the dialog: the document is encrypted for its recipients when it is saved. With
    /// no randomness for the file key nothing changes, and the notice says why.
    pub(crate) fn apply_certificate_draft(&mut self, ctx: &egui::Context, draft: &CertificateDraft) -> bool {
        let seed = match self.session.new_seed() {
            Ok(seed) => seed,
            Err(e) => {
                self.notify_fmt("Couldn't encrypt: {e}", &[("e", &e.to_string())]);
                return false;
            }
        };
        let edit = draft.edit(seed);
        if self.apply_edit(ctx, edit) {
            self.notify_tr("Certificate encryption will be applied when you save");
            true
        } else {
            false
        }
    }
}

/// The footer buttons after Apply and Cancel.
const ADD_FILE: usize = 2;

impl PdfCraftApp {
    /// Protect ▸ Encrypt with certificate: open the dialog, with no recipients yet.
    pub(crate) fn open_certificate_encryption(&mut self) {
        self.certificate_draft = Some(CertificateDraft::default());
    }

    /// One pass of the open Encrypt with Certificate dialog.
    pub(crate) fn certificate_pass(&mut self, ui: &mut egui::Ui) {
        let Some(mut draft) = self.certificate_draft.clone() else { return };
        let files = id_files(self);
        let mut open = true;
        let mut add_own = false;
        let problem = draft.problem();
        let mut dialog = FormDialog::new(tl!("Encrypt with Certificate"), Bind::bound(&mut draft))
            .opened(Bind::bound(&mut open))
            .ok_label(tl!("Apply"))
            .ok_enabled(problem.is_none());
        if cfg!(not(target_arch = "wasm32")) {
            dialog = dialog.extra_buttons([FooterButton::new(tl!("Add certificate file…"), DialogRole::Other)]);
        }
        if let Some(why) = problem {
            dialog = dialog.ok_reason(tl!(why));
        }
        let out = dialog.show(ui, |ui, _, d| {
            add_own = body(ui, d, &files);
            true
        });
        if out.closed.is_some() && out.accepted.is_none() {
            self.certificate_draft = None;
            return;
        }
        if out.accepted.is_some() {
            self.certificate_draft = None;
            self.apply_certificate_draft(ui.ctx(), &draft);
            return;
        }
        if add_own {
            draft.error = self.add_own_recipient(&mut draft).err();
        }
        self.certificate_draft = Some(draft);
        if out.picked == Some(ADD_FILE) {
            let request =
                PickRequest::open("protect.certificate").title(tl!("Add certificate file")).filter(tl!("Certificate"), &["cer", "crt", "pem", "der"]);
            self.pick_file(request, None, |app, _, picked| {
                let Some(path) = picked.first_path() else { return };
                // The answer belongs to the dialog that asked: none open now, nothing to fill.
                let Some(d) = app.certificate_draft.as_mut() else { return };
                match recipients_from_file(&path) {
                    Ok(list) => {
                        d.error = None;
                        for r in list {
                            d.add(r);
                        }
                    }
                    Err(e) => d.error = Some(e),
                }
            });
        }
    }

    /// The prompt for a document encrypted for certificate holders: which of the person's digital
    /// ID files opens it, and its password. Returns `false` when the open prompt is a password's.
    pub(crate) fn certificate_open_pass(&mut self, ui: &mut egui::Ui) -> bool {
        let files = id_files(self);
        let Some(prompt) = self.password_prompt.as_mut() else { return false };
        let Some(chosen) = prompt.digital_id.as_mut() else { return false };
        if chosen.path.is_empty()
            && let Some((p, _)) = files.first()
        {
            chosen.path = p.clone();
        }
        let message = crate::i18n::fmt(
            tl!("“{name}” is encrypted for specific people. Choose your digital ID and enter its password to open it."),
            &[("name", &prompt.name)],
        );
        let error = prompt.error.clone();
        let mut answer = chosen.clone();
        let mut open = true;
        let out = FormDialog::new(tl!("Digital ID required"), Bind::bound(&mut answer))
            .opened(Bind::bound(&mut open))
            .ok_label(tl!("Open"))
            .ok_enabled(!chosen.path.is_empty())
            .extra_buttons([FooterButton::new(tl!("Browse…"), DialogRole::Other)])
            .show(ui, |ui, _, a| {
                ui.add(egui::Label::new(&message).wrap());
                ui.add_space(8.0);
                let mut options: Vec<(String, String)> = files.iter().map(|(p, n)| (p.clone(), n.clone())).collect();
                // A file chosen with Browse… joins the list, named by its file name.
                if !a.path.is_empty() && !options.iter().any(|(p, _)| *p == a.path) {
                    let name = std::path::Path::new(&a.path).file_name().map_or_else(|| a.path.clone(), |n| n.to_string_lossy().into_owned());
                    options.push((a.path.clone(), name));
                }
                crate::widgets::select(ui, tl!("Digital ID"), &mut a.path, options);
                ui.horizontal(|ui| {
                    let label = ui.label(tl!("Password"));
                    ui.add(egui::TextEdit::singleline(&mut a.password).password(true).desired_width(f32::INFINITY)).labelled_by(label.id);
                });
                if let Some(e) = error.as_deref() {
                    ui.add(egui::Label::new(egui::RichText::new(tl!(e)).color(ui.visuals().error_fg_color)).wrap());
                }
                true
            });
        if out.accepted.is_some() {
            self.submit_digital_id(ui.ctx(), answer);
            return true;
        }
        if out.closed.is_some() {
            self.submit_password(ui.ctx(), None);
            return true;
        }
        if let Some(prompt) = self.password_prompt.as_mut() {
            prompt.digital_id = Some(answer);
        }
        if out.picked == Some(ADD_FILE) {
            let request = PickRequest::open("sign.digital_id").filter(tl!("Digital ID"), &["p12", "pfx"]);
            self.pick_file(request, None, |app, _, picked| {
                let Some(path) = picked.first_path() else { return };
                if let Some(chosen) = app.password_prompt.as_mut().and_then(|p| p.digital_id.as_mut()) {
                    chosen.path = path.to_string_lossy().into_owned();
                }
            });
        }
        true
    }

    /// Open the waiting document with the chosen digital ID: it joins the session's identities, and
    /// the open runs again. A digital ID that can't be read asks again, saying why.
    fn submit_digital_id(&mut self, ctx: &egui::Context, answer: DigitalIdAnswer) {
        match open_digital_id(&answer.path, &answer.password) {
            Ok(id) => {
                let cert = id.certificate.clone();
                self.session.add_identity(id);
                self.add_digital_id(&answer.path, &cert);
                self.retry_open(ctx);
            }
            Err(e) => {
                if let Some(prompt) = self.password_prompt.as_mut() {
                    prompt.error = Some(e);
                    prompt.digital_id = Some(DigitalIdAnswer { password: String::new(), ..answer });
                }
            }
        }
    }
}

/// What the digital ID prompt asks: which file, and its password.
#[derive(Clone, Default, PartialEq)]
pub struct DigitalIdAnswer {
    pub path: String,
    pub password: String,
}

/// Draws the dialog body; returns whether Add (your digital ID) was pressed.
fn body(ui: &mut egui::Ui, d: &mut CertificateDraft, files: &[(String, String)]) -> bool {
    let weak = ui.visuals().weak_text_color();
    ui.add(egui::Label::new(tl!("Only the recipients can open the document, each with the private key of their own certificate.")).wrap());
    ui.add_space(8.0);
    ui.label(egui::RichText::new(tl!("Recipients")).strong());
    ui.add_space(4.0);
    let mut remove = None;
    if d.recipients.is_empty() {
        ui.label(egui::RichText::new(tl!("No recipients yet.")).color(weak));
    } else {
        for (i, r) in d.recipients.iter_mut().enumerate() {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let name = if r.mine { crate::i18n::fmt(tl!("{name} (you)"), &[("name", &r.name)]) } else { r.name.clone() };
                ui.label(egui::RichText::new(name).strong());
                let sub = [r.email.as_str(), r.issuer.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
                if !sub.is_empty() {
                    ui.label(egui::RichText::new(sub).color(weak).small());
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Expires {date}"), &[("date", &r.expires)])).color(weak).small());
                    crate::widgets::check(ui, &mut r.everything, tl!("All permissions"));
                    if ui.button(tl!("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            });
            ui.add_space(4.0);
        }
    }
    if let Some(i) = remove {
        d.recipients.remove(i);
    }
    let mut add_own = false;
    if !files.is_empty() {
        ui.add_space(6.0);
        if d.own_id.is_empty() || !files.iter().any(|(p, _)| *p == d.own_id) {
            d.own_id = files[0].0.clone();
        }
        ui.horizontal_wrapped(|ui| {
            let options = files.iter().map(|(p, n)| (p.clone(), n.clone())).collect();
            crate::widgets::select(ui, tl!("Add your digital ID"), &mut d.own_id, options);
            let r = ui.add(egui::TextEdit::singleline(&mut d.own_password).password(true).hint_text(tl!("Password")).desired_width(160.0));
            if ui.button(tl!("Add")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                add_own = true;
            }
        });
    }
    ui.add_space(10.0);
    ui.separator();
    ui.add_space(6.0);
    {
        use crate::widgets::{check, radios, select};
        use pdfcraft_engine::Algorithm as A;
        let p = &mut d.protection;
        if !matches!(p.algorithm, A::Aes256 | A::Aes128) {
            p.algorithm = Algorithm::Aes256;
        }
        let algos = vec![
            (A::Aes256, tl!("Acrobat X and later (256-bit AES)").to_string()),
            (A::Aes128, tl!("Acrobat 7.0 and later (128-bit AES)").to_string()),
        ];
        select(ui, tl!("Compatibility"), &mut p.algorithm, algos);
        let encrypt = vec![(true, tl!("All document contents").to_string()), (false, tl!("All document contents except metadata").to_string())];
        radios(ui, tl!("Encrypt"), &mut p.encrypt_metadata, encrypt);
        let printing = vec![
            (Printing::None, tl!("None").to_string()),
            (Printing::Low, tl!("Low Resolution (150 dpi)").to_string()),
            (Printing::High, tl!("High Resolution").to_string()),
        ];
        select(ui, tl!("Printing allowed"), &mut p.printing, printing);
        let changes = vec![
            (Changes::None, tl!("None").to_string()),
            (Changes::Pages, tl!("Inserting, deleting, and rotating pages").to_string()),
            (Changes::FillSign, tl!("Filling in form fields and signing existing signature fields").to_string()),
            (Changes::CommentFillSign, tl!("Commenting, filling in form fields, and signing existing signature fields").to_string()),
            (Changes::AnyExceptExtract, tl!("Any except extracting pages").to_string()),
        ];
        select(ui, tl!("Changes allowed"), &mut p.changes, changes);
        check(ui, &mut p.copy, tl!("Enable copying of text, images, and other content"));
        check(ui, &mut p.accessibility, tl!("Enable text access for screen reader devices for the visually impaired"));
    }
    ui.add_space(8.0);
    if let Some(e) = &d.error {
        // Known error text is translated; anything else passes through.
        ui.add(egui::Label::new(egui::RichText::new(tl!(e)).color(ui.visuals().error_fg_color)).wrap());
    }
    if !d.recipients.is_empty() && !d.includes_me() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(tl!("None of the recipients is one of your digital IDs: you won't be able to open the saved document."))
                    .color(ui.visuals().warn_fg_color),
            )
            .wrap(),
        );
    }
    ui.label(
        egui::RichText::new(tl!("The restrictions apply to recipients without “All permissions”. They apply when you save.")).color(weak).small(),
    );
    add_own
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recipient(der: &[u8], mine: bool) -> Recipient {
        Recipient {
            name: "Ada".into(),
            email: String::new(),
            issuer: String::new(),
            expires: String::new(),
            der: der.to_vec(),
            everything: mine,
            mine,
        }
    }

    #[test]
    fn drafts_need_a_recipient_and_give_each_their_permissions() {
        let mut d = CertificateDraft::default();
        assert_eq!(d.problem(), Some("Add at least one recipient."));
        d.add(recipient(b"other", false));
        d.add(recipient(b"me", true));
        // The same certificate twice stays once, with the more generous permissions.
        d.add(recipient(b"other", false));
        assert_eq!(d.recipients.len(), 2);
        assert!(d.includes_me() && d.problem().is_none());
        d.protection.printing = Printing::None;
        d.protection.copy = false;
        let Edit::ProtectWithCertificates(p) = d.edit([7; 20]) else { panic!() };
        assert_eq!(p.seed, [7; 20]);
        assert_eq!(p.recipients[0].permissions, d.restricted_bits());
        assert_ne!(p.recipients[0].permissions, -1);
        assert_eq!(p.recipients[0].permissions & (1 << 2), 0, "no printing");
        assert_eq!(p.recipients[1].permissions, -1, "your own ID keeps everything");
    }
}
