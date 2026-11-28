//! Lion PDF ▸ Sign ▸ Timestamp: a trusted RFC 3161 document timestamp, saved as a copy. Only a
//! digest of the document goes to the server, from a worker thread; the UI alone writes the
//! result, so a cancelled or failed request leaves the file and the document as they were.

use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use craft::host::PickRequest;
use craft::ui::contract::Bind;
use craft::ui::dialog::{DialogResult, dialog_id};
use craft::ui::patterns::form_dialogs::{DialogFields, HostedParamDialog, Param, ParamDialog, ParamKind};
use craft::ui::runtime::env::Label;
use pdfcraft_engine::{DocId, SignOptions};

use crate::PdfCraftApp;

const KEY: &str = "sign.timestamp";
/// The services offered by name; anything else is typed.
const SERVERS: [(&str, &str); 2] =
    [("Sectigo (default)", pdfcraft_platform::timestamp::DEFAULT_SERVER), ("FreeTSA (its certificate must be trusted separately)", "https://freetsa.org/tsr")];
const CUSTOM: &str = "custom";

/// A timestamp request on its way.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct TimestampRun {
    doc: DocId,
    generation: u64,
    path: PathBuf,
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
}

/// The server the dialog's fields name.
fn server_of(fields: &DialogFields) -> String {
    match fields.text("service") {
        Some(CUSTOM) | None => fields.text("server").unwrap_or_default().trim().to_string(),
        Some(url) => url.to_string(),
    }
}

impl PdfCraftApp {
    /// Opens Timestamp, the last server chosen.
    pub(crate) fn open_timestamp(&mut self, ctx: &egui::Context) {
        if self.active_ids().is_none() {
            return;
        }
        let server = if self.timestamp_server.trim().is_empty() { SERVERS[0].1.to_string() } else { self.timestamp_server.clone() };
        let mut fields = DialogFields::new();
        let listed = SERVERS.iter().any(|(_, url)| *url == server);
        fields.set("service", if listed { server.clone() } else { CUSTOM.to_string() });
        fields.set("server", if listed { String::new() } else { server });
        let dialog = HostedParamDialog::new(ctx, KEY, fields, |fields| {
            let server = server_of(fields);
            let refusal = pdfcraft_platform::timestamp::TimestampServer::new(&server).err().map(|e| e.to_string());
            let mut services: Vec<(String, Label)> = SERVERS.iter().map(|(name, url)| (url.to_string(), Label::from(tl!(name)))).collect();
            services.push((CUSTOM.to_string(), Label::from(tl!("Another server…"))));
            let rows = vec![
                Param::new("service", tl!("Timestamp service"), ParamKind::choice(services, None)),
                Param::new("server", tl!("Server address"), ParamKind::text("")).visible_when("service", CUSTOM),
            ];
            let mut dialog = ParamDialog::new(KEY, tl!("Timestamp"), rows)
                .values(Bind::bound(fields))
                .ok_label(tl!("Timestamp and save…").into())
                .ok_enabled(refusal.is_none())
                .footer_note(tl!(
                    "The server receives a digest of the document, not the document. Its certificate must be trusted for the timestamp to validate."
                ));
            if let Some(why) = refusal {
                dialog = dialog.ok_reason(Label::from(why));
            }
            dialog
        });
        crate::dialogs::host::open(self, dialog);
    }

    /// Timestamp's OK: remember the server, ask where the copy goes, then request the timestamp.
    pub(crate) fn timestamp_pass(&mut self, ctx: &egui::Context) {
        if let Some(DialogResult::Ok(result)) = self.dialogs.take_by_id::<HostedParamDialog>(dialog_id(KEY)) {
            let server = server_of(&result.fields);
            self.timestamp_server = server.clone();
            if !self.commit_form_typing(ctx) {
                return;
            }
            let Some((_, doc)) = self.active_ids() else { return };
            let name = self.session.get(doc).map(|d| d.name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string()).unwrap_or_default();
            let folder = self.session.get(doc).and_then(|d| d.path.clone());
            let mut request = PickRequest::save("sign.timestamped").file_name(format!("{name}_timestamped.pdf")).filter("PDF", &["pdf"]);
            if let Some(parent) = folder.as_deref().and_then(|p| std::path::Path::new(p).parent()) {
                request = request.default_directory(parent);
            }
            self.pick_file(request, None, move |app, ctx, picked| {
                let Some(path) = picked.first_path() else { return };
                if let Err(e) = app.start_timestamp(ctx, doc, path, &server) {
                    app.notify_fmt("Timestamping failed: {e}", &[("e", &e)]);
                }
            });
        }
        self.poll_timestamp();
    }

    /// Sends the document's digest to `url` on a worker.
    fn start_timestamp(&mut self, ctx: &egui::Context, doc: DocId, path: PathBuf, url: &str) -> Result<(), String> {
        if self.timestamp_run.is_some() {
            return Err(tl!("A timestamp request is already running.").to_string());
        }
        let server = pdfcraft_platform::timestamp::TimestampServer::new(url).map_err(|e| e.to_string())?;
        if !self.commit_typing_in(ctx, doc) {
            return Err(tl!("Finish editing the document first.").to_string());
        }
        let generation = self.session.get(doc).ok_or(tl!("The document was closed."))?.edit_generation();
        let (cos, opts) = self.session.signing_input(doc, SignOptions::default()).map_err(|e| e.to_string())?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let repaint = ctx.clone();
            std::thread::Builder::new()
                .name("lion-timestamp".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pdfcraft_engine::sign::timestamp_document(&cos, &server, &opts.date).map_err(|e| e.to_string())
                    }))
                    .unwrap_or_else(|_| Err("The timestamp worker stopped unexpectedly.".into()));
                    let _ = tx.send(result);
                    repaint.request_repaint();
                })
                .map_err(|e| format!("Could not start the timestamp request: {e}"))?;
            self.timestamp_run = Some(TimestampRun { doc, generation, path, rx });
            self.notify_tr("Waiting for the timestamp server…");
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (server, cos, opts, generation, path);
            Err(tl!("Timestamping needs the desktop app.").to_string())
        }
    }

    /// Writes a finished timestamp: only when the document did not change meanwhile.
    fn poll_timestamp(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(job) = &self.timestamp_run else { return };
            let result = match job.rx.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("The timestamp worker stopped unexpectedly.".into()),
            };
            let Some(job) = self.timestamp_run.take() else { return };
            let result = result.and_then(|bytes| {
                let bytes = Arc::new(bytes);
                if self.session.get(job.doc).is_none_or(|d| d.edit_generation() != job.generation) {
                    return Err(tl!("The document changed during timestamping. No file was written.").to_string());
                }
                self.session.check_save(job.doc, &bytes).map_err(|e| e.to_string())?;
                craft::atomic::atomic_write(&job.path, bytes.as_slice()).map_err(|e| format!("Could not save: {e}"))?;
                // Our own write is not a change on disk to reload.
                self.watch.known_now(job.doc, &job.path);
                self.session.mark_signed(job.doc, bytes, Some(job.path.to_string_lossy().into_owned())).map_err(|e| e.to_string())?;
                if let Some(view) = self.views.iter_mut().find(|v| v.id == job.doc) {
                    view.invalidate_content();
                }
                self.set_right(Some(crate::RightPanel::Signatures));
                self.notify_fmt("Timestamped and saved to {path}", &[("path", &job.path.display().to_string())]);
                Ok(())
            });
            if let Err(error) = result {
                self.notify_fmt("Timestamping failed: {e}", &[("e", &error)]);
            }
        }
    }
}
