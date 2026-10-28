//! Help ▸ Check for updates (issue #28): ask for the latest release and offer its download page.
//!
//! The desktop app supplies how to ask ([`PdfCraftApp::update_source`]), so this crate has no
//! network code; without a source (the web build, tests) the command opens the releases page.
//! Preferences can download a verified fork installer; activation is an explicit user action.
//! It asks only when the user does: there is no check at start (the owner's decision).

use std::sync::Arc;

use crate::{PdfCraftApp, theme, widgets};

/// Where every PdfCraft release is listed.
pub const RELEASES_PAGE: &str = "https://github.com/gh0sted5456-us/pdfcraft/releases";

/// The latest published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Its version tag, such as `v0.2.0`.
    pub version: String,
    /// Its page on [`RELEASES_PAGE`], where the downloads are.
    pub url: String,
    pub installer: Option<Installer>,
}

/// A platform installer published by this fork. The desktop validates its URL and digest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installer {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

pub type DownloadSource = Arc<dyn Fn(Installer, Arc<std::sync::atomic::AtomicU64>) -> Result<std::path::PathBuf, String> + Send + Sync>;
pub type InstallSource = Arc<dyn Fn(Installer, std::path::PathBuf, crate::last_session::LastSession) -> Result<(), String> + Send + Sync>;

#[derive(Default)]
enum Download {
    #[default]
    Idle,
    #[cfg(not(target_arch = "wasm32"))]
    Running {
        received: Arc<std::sync::atomic::AtomicU64>,
        installer: Installer,
        reply: std::sync::mpsc::Receiver<Result<std::path::PathBuf, String>>,
    },
    #[cfg(not(target_arch = "wasm32"))]
    Ready(Installer, std::path::PathBuf),
    #[cfg(not(target_arch = "wasm32"))]
    Failed(String),
}

/// Asks for the latest release (blocking; it runs on its own thread).
pub type UpdateSource = Arc<dyn Fn() -> Result<Release, String> + Send + Sync>;

/// Whether release `latest` (a tag such as `v0.2.0`) is newer than version `current` (`0.1.1`).
/// Pre-release and build suffixes are ignored; a version that doesn't parse is never newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse(latest), parse(current)), (Some(l), Some(c)) if l > c)
}

/// Display a version without this fork's release-tag namespace.
pub fn version_name(tag: &str) -> &str {
    tag.strip_prefix("fork-").unwrap_or(tag).trim_start_matches(['v', 'V'])
}

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let v = version_name(v.trim());
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = |required: bool| match parts.next() {
        Some(p) => p.parse::<u64>().ok(),
        None if required => None,
        None => Some(0),
    };
    let version = (next(true)?, next(false)?, next(false)?);
    parts.next().is_none().then_some(version)
}

/// Where a check is.
#[derive(Default)]
pub(crate) enum Check {
    #[default]
    Idle,
    #[cfg(not(target_arch = "wasm32"))]
    Running(std::sync::mpsc::Receiver<Result<Release, String>>),
    Done(Result<Release, String>),
}

#[derive(Default)]
pub(crate) struct Updates {
    pub(crate) check: Check,
    /// The Updates dialog is showing.
    pub(crate) open: bool,
    download: Download,
    resolved: Vec<pdfcraft_engine::DocId>,
}

impl PdfCraftApp {
    /// Help ▸ Check for updates: ask for the latest release and show the outcome.
    pub fn check_for_updates(&mut self) {
        let Some(source) = self.update_source.clone() else {
            self.open_url(RELEASES_PAGE);
            return;
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.updates.open = true;
            if matches!(self.updates.check, Check::Running(_)) || matches!(self.updates.download, Download::Running { .. }) {
                return;
            }
            self.updates.download = Download::Idle;
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                // The receiver may be gone (the app quit): nothing to report to then.
                let _ = tx.send(source());
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            });
            self.updates.check = Check::Running(rx);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = source;
            self.open_url(RELEASES_PAGE);
        }
    }

    /// Pick up a finished check (each frame).
    pub(crate) fn poll_updates(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Download::Running { installer, reply, .. } = &self.updates.download {
            match reply.try_recv() {
                Ok(Ok(path)) => self.updates.download = Download::Ready(installer.clone(), path),
                Ok(Err(error)) => self.updates.download = Download::Failed(error),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.updates.download = Download::Failed("The download stopped unexpectedly.".into())
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Check::Running(rx) = &self.updates.check {
            let result = match rx.try_recv() {
                Ok(r) => r,
                Err(std::sync::mpsc::TryRecvError::Empty) => return,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the update check stopped unexpectedly".into()),
            };
            self.updates.check = Check::Done(result);
        }
    }
}

/// The Updates dialog.
pub(crate) fn dialog(app: &mut PdfCraftApp, ctx: &egui::Context) {
    if !app.updates.open {
        return;
    }
    let t = theme::Tokens::get(ctx);
    let current = env!("CARGO_PKG_VERSION");
    let mut close = false;
    let mut download: Option<String> = None;
    let modal = crate::widgets::show_modal(ctx, "updates", |ui| {
        ui.set_width(420.0);
        ui.vertical_centered(|ui| {
            ui.add(crate::icons::image("cloud", 22.0, t.accent));
        });
        crate::widgets::dialog_heading(ui, tl!("Check for updates"));
        ui.add_space(8.0);
        match &app.updates.check {
            Check::Idle => {
                ui.label(tl!("No check has run yet."));
            }
            #[cfg(not(target_arch = "wasm32"))]
            Check::Running(_) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(tl!("Checking for a newer version…"));
                });
            }
            Check::Done(Ok(r)) if is_newer(&r.version, current) => {
                let version = version_name(&r.version);
                ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{app} {v} is available."), &[("app", crate::APP_NAME), ("v", version)])).strong());
                ui.label(
                    egui::RichText::new(crate::i18n::fmt(
                        tl!("You have version {c}. Download the new version from its release page."),
                        &[("c", current)],
                    ))
                    .color(t.text_muted),
                );
                download = Some(r.url.clone());
            }
            Check::Done(Ok(_)) => {
                ui.label(crate::i18n::fmt(tl!("{app} {c} is up to date."), &[("app", crate::APP_NAME), ("c", current)]));
            }
            Check::Done(Err(e)) => {
                ui.label(crate::i18n::fmt(tl!("Couldn't check for updates: {e}"), &[("e", &e.to_string())]));
                ui.label(
                    egui::RichText::new(crate::i18n::fmt(
                        tl!("You have version {c}. All releases are listed at {page}."),
                        &[("c", current), ("page", RELEASES_PAGE)],
                    ))
                    .color(t.text_muted),
                );
            }
        }
        ui.add_space(10.0);
        ui.label(
            egui::RichText::new(tl!("Asks GitHub for the latest release. Nothing is downloaded or installed automatically."))
                .color(t.text_muted)
                .small(),
        );
        ui.add_space(12.0);
        crate::widgets::dialog_footer(ui, |ui| {
            if let Some(url) = download.take() {
                let get = widgets::dialog_button(ui, tl!("Download"), widgets::DialogAction::Confirm).clicked();
                let later = widgets::dialog_button(ui, tl!("Later"), widgets::DialogAction::Cancel).clicked();
                close = get || later;
                download = get.then_some(url);
            } else if widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked() {
                close = true;
            }
        });
    });
    if modal.should_close() {
        close = true;
        download = None;
    }
    if close {
        app.updates.open = false;
        if let Some(url) = download {
            app.open_url(&url);
        }
    }
}

/// Fork release status inside the bounded Preferences pane.
pub(crate) fn preferences(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    ui.label(egui::RichText::new(format!("{} {}", crate::APP_NAME, env!("CARGO_PKG_VERSION"))).strong());
    ui.label("Personal fork · gh0sted5456-us/pdfcraft");
    ui.add_space(10.0);
    #[cfg(not(target_arch = "wasm32"))]
    let busy = matches!(app.updates.check, Check::Running(_)) || matches!(app.updates.download, Download::Running { .. });
    #[cfg(target_arch = "wasm32")]
    let busy = false;
    if ui.add_enabled_ui(!busy, |ui| widgets::dialog_button(ui, tl!("Check for updates"), widgets::DialogAction::Confirm)).inner.clicked() {
        app.check_for_updates();
        app.updates.open = false;
    }
    ui.add_space(12.0);
    match &app.updates.check {
        Check::Idle => {
            ui.label(tl!("No check has run yet."));
        }
        #[cfg(not(target_arch = "wasm32"))]
        Check::Running(_) => {
            ui.spinner();
            ui.label(tl!("Checking for a newer version…"));
        }
        Check::Done(Err(error)) => {
            ui.label(format!("Could not check for updates: {error}"));
        }
        Check::Done(Ok(release)) => {
            let release = release.clone();
            ui.label(format!("Latest release: {}", version_name(&release.version)));
            if is_newer(&release.version, env!("CARGO_PKG_VERSION")) {
                ui.label(egui::RichText::new(format!("Update available: {}", version_name(&release.version))).strong());
                if let Some(_installer) = release.installer.clone().filter(|_| app.update_download.is_some()) {
                    #[cfg(not(target_arch = "wasm32"))]
                    if !busy
                        && !matches!(app.updates.download, Download::Ready(..))
                        && widgets::dialog_button(ui, tl!("Download"), widgets::DialogAction::Confirm).clicked()
                    {
                        app.download_update(_installer);
                    }
                } else {
                    ui.label("No in-app installer is published for this platform. Use the release page below.");
                }
            } else {
                ui.label(tl!("Up to date"));
            }
            if ui.link("Release notes").clicked() {
                app.open_url(&release.url);
            }
        }
    }
    ui.add_space(12.0);
    match &app.updates.download {
        Download::Idle => {}
        #[cfg(not(target_arch = "wasm32"))]
        Download::Running { received, installer, .. } => {
            let bytes = received.load(std::sync::atomic::Ordering::Relaxed).min(installer.size);
            let progress = bytes as f32 / installer.size.max(1) as f32;
            ui.add(egui::ProgressBar::new(progress).show_percentage().text(format!(
                "{:.1} / {:.1} MB",
                bytes as f64 / 1048576.0,
                installer.size as f64 / 1048576.0
            )));
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        #[cfg(not(target_arch = "wasm32"))]
        Download::Failed(error) => {
            ui.label(format!("Update download failed: {error}"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        Download::Ready(installer, path) => {
            ui.label("Download complete · SHA-256 verified");
            let available = app.update_install.is_some() && app.update_can_close(ui.ctx());
            let _ = (installer, path);
            if ui
                .add_enabled_ui(available, |ui| widgets::dialog_button(ui, "Install update", widgets::DialogAction::Confirm))
                .inner
                .on_hover_text("Save changes, install the verified update, and restart with your PDFs open.")
                .clicked()
            {
                app.updates.resolved.clear();
                app.continue_update(ui.ctx());
            }
            ui.label("Open PDFs will return after restarting. Unsaved changes are resolved before installation.");
            if !available {
                ui.label("Finish pending file opens and close other Lion PDF windows or minimized document dialogs first.");
            }
        }
    }
    ui.add_space(12.0);
    ui.label("Updates come only from this fork's published GitHub releases. Nothing is checked, downloaded or installed automatically.");
    if ui.link("All fork releases").clicked() {
        app.open_url(RELEASES_PAGE);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl PdfCraftApp {
    fn download_update(&mut self, installer: Installer) {
        let Some(source) = self.update_download.clone() else { return };
        let (tx, reply) = std::sync::mpsc::channel();
        let received = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let progress = received.clone();
        let request = installer.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(source(request, progress));
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
        self.updates.download = Download::Running { received, installer, reply };
    }
}

impl PdfCraftApp {
    pub(crate) fn first_update_dirty(&self) -> Option<usize> {
        (0..self.views.len()).find(|&i| self.has_unsaved_work(i) && self.views.get(i).is_some_and(|v| !self.updates.resolved.contains(&v.id)))
    }

    fn update_can_close(&self, ctx: &egui::Context) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if !self.pending_opens.is_empty() {
            return false;
        }
        !self.dialog_shelf.holds_document_drafts() && crate::window_drop::inspect(ctx).as_array().is_none_or(|windows| windows.len() <= 1)
    }

    pub(crate) fn resolve_update(&mut self, ctx: &egui::Context, choice: Option<bool>) {
        let Some(save) = choice else {
            self.updates.resolved.clear();
            return;
        };
        let Some(index) = self.first_update_dirty() else {
            self.continue_update(ctx);
            return;
        };
        let Some(id) = self.views.get(index).map(|v| v.id) else { return };
        if save {
            self.active = Some(index);
            let ctx = ctx.clone();
            self.save_then(index, crate::editing::SaveTarget::InPlace, move |app| {
                app.updates.resolved.push(id);
                app.continue_update(&ctx);
            });
        } else {
            self.updates.resolved.push(id);
            self.continue_update(ctx);
        }
    }

    fn continue_update(&mut self, ctx: &egui::Context) {
        if !self.update_can_close(ctx) {
            self.notify("Finish pending file opens and close other Lion PDF windows or minimized document dialogs before installing.");
            return;
        }
        if self.first_update_dirty().is_some() {
            self.save_prompt_seen = false;
            self.close_request = Some(crate::CloseRequest::Update);
            ctx.request_repaint();
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Download::Ready(installer, path) = &self.updates.download
            && let Some(install) = self.update_install.clone()
        {
            match install(installer.clone(), path.clone(), self.open_session()) {
                Ok(()) => {
                    self.shutdown_recovery();
                    self.quit(ctx);
                }
                Err(error) => {
                    self.updates.resolved.clear();
                    self.notify(error);
                }
            }
        }
    }
}
