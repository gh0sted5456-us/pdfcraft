//! Trusted timestamps use the existing signer, with transport off the UI thread.

use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

use pdfcraft_engine::{DocId, SignOptions, sign};

use crate::{
    PdfCraftApp,
    sign_ui::{SignBackground, SignGraphic},
    widgets,
};

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) struct TimestampRun {
    doc: DocId,
    generation: u64,
    path: PathBuf,
    choice: Option<(SignGraphic, SignBackground)>,
    #[cfg(not(target_arch = "wasm32"))]
    rx: std::sync::mpsc::Receiver<Result<Vec<u8>, String>>,
}

const SERVERS: [(&str, &str); 2] =
    [("Sectigo (default)", pdfcraft_platform::timestamp::DEFAULT_SERVER), ("FreeTSA (manual trust)", "https://freetsa.org/tsr")];

pub(crate) fn server_selector(ui: &mut egui::Ui, server: &mut String) {
    let name = SERVERS.iter().find(|(_, url)| *url == server.trim()).map_or("Custom…", |(name, _)| *name);
    let label = ui.label("Timestamp service");
    egui::ComboBox::from_id_salt("timestamp-service")
        .selected_text(name)
        .width(220.0)
        .show_ui(ui, |ui| {
            for (name, url) in SERVERS {
                ui.selectable_value(server, url.to_string(), name);
            }
            ui.selectable_value(server, String::new(), "Custom…");
        })
        .response
        .labelled_by(label.id);
    let label = ui.label("Timestamp server");
    ui.add(widgets::line(server).desired_width(440.0).hint_text("https://…")).labelled_by(label.id);
    if server.trim() == SERVERS[1].1 {
        ui.label("FreeTSA may require separately trusting its certificate. No trust is added automatically.");
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    #[allow(clippy::unwrap_used)]
    fn harness(path: &str) -> Harness<'static, PdfCraftApp> {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        let bytes = app.session.create_blank(300.0, 200.0, 1).unwrap();
        app.open_bytes("Synthetic.pdf", None, bytes.as_ref().clone()).unwrap();
        app.save_override = Some(path.to_string());
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_| app);
        h.run_steps(3);
        h
    }

    #[test]
    fn timestamp_footer_buttons_match_dialog_actions_in_both_themes() {
        for kind in [crate::theme::ThemeKind::Light, crate::theme::ThemeKind::Dark] {
            let mut h = harness("unused.pdf");
            crate::theme::apply(&h.ctx, kind);
            h.state_mut().execute("sign.timestamp");
            h.run_steps(3);
            let cancel = h.get_by_label("Cancel").rect();
            let confirm = h.get_all_by_label("Timestamp").last().unwrap().rect();
            let card = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("dialog").with("card"))).unwrap();
            assert!(cancel.right() < confirm.left());
            assert!((cancel.center().y - confirm.center().y).abs() < 0.5);
            assert!((cancel.union(confirm).center().x - card.center().x).abs() < 1.0, "confirmation group belongs at the center");
            assert!((confirm.bottom() - (card.bottom() - 16.0)).abs() <= 2.0, "only the dialog padding below the footer: {confirm:?} in {card:?}");
            for rect in [cancel, confirm] {
                assert!(rect.width() >= 80.0 && (rect.height() - 28.0).abs() < 0.5, "shared action size: {rect:?}");
                let fill = h
                    .output()
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        egui::Shape::Rect(r)
                            if rect.contains_rect(r.rect) && r.rect.center() == rect.center() && r.fill != egui::Color32::TRANSPARENT =>
                        {
                            Some(r.fill)
                        }
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(fill, crate::theme::Tokens::get(&h.ctx).field, "actions are matte at rest");
            }
            h.get_by_label("Cancel").click();
            h.run_steps(2);
            assert!(h.state().dialog.is_none());
            assert!(h.state().timestamp_run.is_none(), "Cancel never starts a request");
        }
    }

    #[test]
    fn timestamp_selector_keeps_sectigo_default_and_remembers_an_explicit_alternative() {
        let mut h = harness("unused.pdf");
        assert_eq!(h.state().timestamp_server, SERVERS[0].1);
        h.state_mut().execute("sign.timestamp");
        h.run_steps(3);
        h.get_by_label("Timestamp service").click();
        h.run_steps(2);
        h.get_by_label("FreeTSA (manual trust)").click();
        h.run_steps(2);
        assert_eq!(h.state().timestamp_server, SERVERS[1].1);
        assert!(h.state().timestamp_run.is_none(), "choosing a service makes no request");
        h.get_by_label("FreeTSA may require separately trusting its certificate. No trust is added automatically.");
        let saved = h.state().persist();
        let mut restored = PdfCraftApp::new();
        restored.restore(&saved);
        assert_eq!(restored.timestamp_server, SERVERS[1].1);
        h.get_by_label("Timestamp service").click();
        h.run_steps(2);
        h.get_by_label("Custom…").click();
        h.run_steps(2);
        assert!(h.state().timestamp_server.is_empty());
        h.get_by_label("Timestamp server").click();
        h.run_steps(1);
        h.get_by_label("Timestamp server").type_text("https://example.com/tsa");
        h.run_steps(2);
        assert_eq!(h.state().timestamp_server, "https://example.com/tsa");
        h.get_by_label("Timestamp service").click();
        h.run_steps(2);
        h.get_by_label("Sectigo (default)").click();
        h.run_steps(2);
        assert_eq!(h.state().timestamp_server, SERVERS[0].1);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().dialog.is_none() && h.state().timestamp_run.is_none());
    }

    #[test]
    fn timestamp_command_opens_the_server_dialog_and_remembers_only_the_url() {
        let mut h = harness("unused.pdf");
        assert!(h.state_mut().execute("sign.timestamp"));
        h.run_steps(2);
        assert_eq!(h.state().dialog, Some(crate::Dialog::Timestamp));
        h.get_by_label("Timestamp server");
        h.state_mut().timestamp_server = "https://example.com/tsa".into();
        let saved = h.state().persist();
        let mut restored = PdfCraftApp::new();
        restored.restore(&saved);
        assert_eq!(restored.timestamp_server, "https://example.com/tsa");
        assert!(restored.timestamp_run.is_none());
    }

    #[test]
    fn malformed_timestamp_keeps_the_file_and_document_unchanged_and_can_be_cancelled() {
        use std::io::{Read, Write};
        let folder = std::env::temp_dir().join(format!("pdfcraft-timestamp-ui-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("existing.pdf");
        std::fs::write(&path, b"preserve this existing file").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(std::time::Duration::from_secs(5))).unwrap();
            let mut bytes = [0; 4096];
            assert!(stream.read(&mut bytes).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nbad").unwrap();
        });
        let mut h = harness(&path.to_string_lossy());
        let id = h.state().views[0].id;
        let before = h.state().session.get(id).unwrap().bytes.clone();
        h.state_mut().timestamp_server = url;
        h.state_mut().execute("sign.timestamp");
        h.run_steps(2);
        h.get_all_by_label("Timestamp").last().unwrap().click();
        h.run_steps(2);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
        while h.state().timestamp_run.is_some() && std::time::Instant::now() < deadline {
            h.run_steps(1);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(h.state().timestamp_run.is_none());
        assert_eq!(h.state().session.get(id).unwrap().bytes, before);
        assert_eq!(std::fs::read(&path).unwrap(), b"preserve this existing file");
        server.join().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        h.state_mut().timestamp_run = Some(TimestampRun { doc: id, generation: 0, path: path.clone(), choice: None, rx });
        h.run_steps(2);
        h.get_by_label("Cancel").click();
        h.run_steps(2);
        assert!(h.state().timestamp_run.is_none());
        assert!(tx.send(Err("cancelled".into())).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"preserve this existing file");
    }
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    crate::widgets::dialog_heading(ui, tl!("Timestamp"));
    ui.label("Add a trusted timestamp to the document and save a copy. The server receives a cryptographic digest, not your PDF.");
    ui.add_space(8.0);
    server_selector(ui, &mut app.timestamp_server);
    ui.label(
        "Use the RFC 3161 server supplied by your certificate provider or organization. Its certificate must be trusted to validate the timestamp.",
    );
    if let Some(error) = &app.timestamp_error {
        ui.colored_label(egui::Color32::from_rgb(210, 65, 65), error);
    }
    ui.add_space(12.0);
    let mut close = false;
    widgets::dialog_footer(ui, |ui| {
        if widgets::dialog_button(ui, tl!("Timestamp"), widgets::DialogAction::Confirm).clicked() {
            match app.timestamp_copy() {
                Ok(()) => close = true,
                Err(error) => app.timestamp_error = Some(error),
            }
        }
        if widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked() {
            close = true;
        }
    });
    close
}

pub(crate) fn progress(app: &mut PdfCraftApp, ctx: &egui::Context) {
    if app.timestamp_run.is_none() {
        return;
    }
    let mut cancel = false;
    widgets::show_modal(ctx, "timestamp-progress", |ui| {
        crate::widgets::dialog_heading(ui, tl!("Timestamp"));
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Waiting for the timestamp server…");
        });
        widgets::dialog_footer(ui, |ui| {
            cancel = widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked();
        });
    });
    if cancel {
        // Only the UI writes the result. Dropping the receiver makes cancellation non-destructive.
        app.timestamp_run = None;
    }
}

impl PdfCraftApp {
    fn timestamp_copy(&mut self) -> Result<(), String> {
        pdfcraft_platform::timestamp::TimestampServer::new(&self.timestamp_server).map_err(|e| e.to_string())?;
        if !self.commit_form_typing() {
            return Err("Finish editing the form field first.".into());
        }
        let (_, doc) = self.active_ids().ok_or("No document is open.")?;
        let server = self.timestamp_server.clone();
        let save = move |app: &mut Self, path: PathBuf| {
            if let Err(error) = app.start_timestamp(doc, path, None, &server) {
                app.notify(error);
            }
        };
        if let Some(path) = &self.save_override {
            self.start_timestamp(doc, PathBuf::from(path), None, &self.timestamp_server.clone())
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let name = self.session.get(doc).map(|d| d.name.trim_end_matches(".pdf").trim_end_matches(".PDF")).unwrap_or("Document");
                let dialog = rfd::AsyncFileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{name}_timestamped.pdf"));
                if self.ask_one(crate::pickers::Ask::Save(dialog), Some(doc), save) { Ok(()) } else { Err(String::new()) }
            }
            #[cfg(target_arch = "wasm32")]
            {
                let _ = save;
                Err("Timestamping needs the desktop app.".into())
            }
        }
    }

    pub(crate) fn start_timestamp(
        &mut self,
        doc: DocId,
        path: PathBuf,
        signature: Option<(sign::DigitalId, SignOptions, SignGraphic, SignBackground)>,
        url: &str,
    ) -> Result<(), String> {
        if self.timestamp_run.is_some() {
            return Err("A timestamp request is already running.".into());
        }
        let server = pdfcraft_platform::timestamp::TimestampServer::new(url).map_err(|e| e.to_string())?;
        if !self.commit_typing_in(doc) {
            return Err("Finish editing the document first.".into());
        }
        let generation = self.session.get(doc).ok_or("The document was closed.")?.edit_generation();
        let (identity, opts, choice) = match signature {
            Some((id, opts, choice, background)) => (Some(id), opts, Some((choice, background))),
            None => (None, SignOptions::default(), None),
        };
        let (cos, opts) = self.session.signing_input(doc, opts).map_err(|e| e.to_string())?;
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            std::thread::Builder::new()
                .name("pdfcraft-timestamp".into())
                .spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match identity {
                            Some(id) => sign::sign_with_timestamp(&cos, &id, &opts, &server),
                            None => sign::timestamp_document(&cos, &server, &opts.date),
                        }
                        .map_err(|e| e.to_string())
                    }))
                    .unwrap_or_else(|_| Err("The timestamp worker stopped unexpectedly.".into()));
                    let _ = tx.send(result);
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                })
                .map_err(|e| format!("Could not start timestamp request: {e}"))?;
            self.timestamp_run = Some(TimestampRun { doc, generation, path, choice, rx });
            Ok(())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (server, cos, opts, identity, choice, generation, path);
            Err("Timestamping needs the desktop app.".into())
        }
    }

    pub(crate) fn poll_timestamp(&mut self) {
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
                    return Err("The document changed during timestamping. No file was written.".into());
                }
                self.session.check_save(job.doc, &bytes).map_err(|e| e.to_string())?;
                crate::editing::write_atomically(&job.path.to_string_lossy(), &bytes).map_err(|e| format!("Could not save: {e}"))?;
                self.watch.known_now(job.doc, &job.path.to_string_lossy());
                self.session.mark_signed(job.doc, bytes, Some(job.path.to_string_lossy().into_owned())).map_err(|e| e.to_string())?;
                if let Some((choice, background)) = job.choice {
                    self.sign_graphic = choice;
                    self.sign_background = background;
                }
                if let Some(view) = self.views.iter_mut().find(|v| v.id == job.doc) {
                    view.invalidate_content();
                }
                self.right = Some(crate::RightPanel::Signatures);
                self.notify(format!("Timestamped and saved to {}", job.path.display()));
                Ok(())
            });
            if let Err(error) = result {
                self.notify(format!("Timestamping failed: {error}"));
            }
        }
    }
}
