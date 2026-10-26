//! Assistant setup and reviewed text requests. Suggestions never mutate a document.
use crate::{PdfCraftApp, widgets};
use pdfcraft_engine::assistant::Task;
#[cfg(not(target_arch = "wasm32"))]
use pdfcraft_engine::{DocId, assistant};
use pdfcraft_platform::assistant::Provider;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub provider: Provider,
    pub account: String,
    pub model: String,
    pub browser: bool,
    pub microsoft: pdfcraft_platform::assistant::Microsoft365Config,
}

#[derive(Default)]
pub(crate) struct Draft {
    pub settings: Settings,
    setup: bool,
    pub task: Task,
    #[cfg(not(target_arch = "wasm32"))]
    question: String,
    #[cfg(not(target_arch = "wasm32"))]
    text: String,
    #[cfg(not(target_arch = "wasm32"))]
    answer: String,
    #[cfg(not(target_arch = "wasm32"))]
    status: String,
    #[cfg(not(target_arch = "wasm32"))]
    scope: u8,
    #[cfg(not(target_arch = "wasm32"))]
    source: Option<(DocId, u64)>,
    #[cfg(not(target_arch = "wasm32"))]
    api_key: String,
    #[cfg(not(target_arch = "wasm32"))]
    profiles: Vec<pdfcraft_platform::assistant::Profile>,
    #[cfg(not(target_arch = "wasm32"))]
    models: Vec<pdfcraft_platform::assistant::Model>,
    #[cfg(not(target_arch = "wasm32"))]
    job: Option<Job>,
    #[cfg(not(target_arch = "wasm32"))]
    context_enabled: bool,
    #[cfg(not(target_arch = "wasm32"))]
    prepared_pages: Vec<usize>,
    #[cfg(not(target_arch = "wasm32"))]
    history: Vec<(String, String)>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_question: String,
    #[cfg(not(target_arch = "wasm32"))]
    connection: Option<(Provider, String, bool)>,
    #[cfg(not(target_arch = "wasm32"))]
    browser_open: bool,
    #[cfg(target_os = "windows")]
    browser_rect: Option<egui::Rect>,
    #[cfg(target_os = "windows")]
    browser_host: Option<crate::assistant_browser::Host>,
}
#[cfg(not(target_arch = "wasm32"))]
enum Update {
    Text(String, (DocId, u64)),
    Answer(String, Option<(DocId, u64)>),
    Microsoft(String),
    Models(Vec<pdfcraft_platform::assistant::Model>),
    Profiles(Vec<pdfcraft_platform::assistant::Profile>, Option<String>),
    Notice(String),
}
#[cfg(not(target_arch = "wasm32"))]
enum Message {
    Browser(String),
    Status(String),
    Done(Result<Update, String>),
}
#[cfg(not(target_arch = "wasm32"))]
struct Job {
    rx: std::sync::mpsc::Receiver<Message>,
    network: bool,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Draft {
    pub fn cancel(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.job = None;
        }
        self.api_key.clear();
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn start(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(std::sync::Arc<std::sync::atomic::AtomicBool>, std::sync::mpsc::Sender<Message>) -> Result<Update, String> + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = cancel.clone();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("pdfcraft-assistant".into()).spawn(move || {
            let outcome = pdfcraft_engine::guard(|| work(flag, tx.clone()))
                .unwrap_or_else(|_| Err("The Assistant worker stopped unexpectedly; the PDF was kept".into()));
            let _ = tx.send(Message::Done(outcome));
            ctx.request_repaint();
        });
        if spawned.is_err() {
            self.status = "Couldn't start the Assistant worker; try again shortly.".into();
            return;
        }
        self.job = Some(Job { rx, cancel, network: false });
        self.status = "Working…".into();
    }
    fn connect(
        &mut self,
        ctx: &egui::Context,
        work: impl FnOnce(std::sync::Arc<std::sync::atomic::AtomicBool>, std::sync::mpsc::Sender<Message>) -> Result<Update, String> + Send + 'static,
    ) {
        self.settings.enabled = true;
        self.start(ctx, work);
        if let Some(job) = &mut self.job {
            job.network = true;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn accept(&mut self, update: Update) {
        match update {
            Update::Text(text, key) if self.source == Some(key) => {
                self.text = text;
                self.history.clear();
                self.answer.clear();
                self.status = "Review the text below. Nothing has been sent.".into();
            }
            Update::Answer(text, key) if self.source == key => {
                if self.pending_question.len().saturating_add(text.len()) > 100_000 {
                    self.pending_question.clear();
                    self.status = "The answer exceeds the chat limit. Ask a narrower question or choose fewer pages.".into();
                    return;
                }
                self.history.push((std::mem::take(&mut self.pending_question), text.clone()));
                while self.history.len() > 6 || self.history.iter().map(|(q, a)| q.len() + a.len()).sum::<usize>() > 100_000 {
                    if self.history.is_empty() {
                        break;
                    }
                    self.history.remove(0);
                }
                self.answer = text;
                self.status = "Suggestions only. The PDF has not been changed.".into();
            }
            Update::Text(..) | Update::Answer(..) => self.status = "The document changed. Review its current text before asking again.".into(),
            Update::Models(models) => {
                self.models = models;
                self.status = "Choose a model available to your own account.".into();
            }
            Update::Profiles(profiles, selected) => {
                self.profiles = profiles;
                if let Some(id) = selected {
                    self.settings.account = id;
                }
                self.status = "Your saved ChatGPT accounts are ready.".into();
            }
            Update::Microsoft(account) => {
                self.settings.account = account;
                self.status = "Microsoft sign-in saved. Copilot access still depends on your license and organization policy.".into();
            }
            Update::Notice(message) => self.status = message,
        }
    }
}
pub(crate) fn open(app: &mut PdfCraftApp, command: &str) {
    app.assistant.setup = command == "ai.setup";
    app.assistant.task = match command {
        "ai.ask" => Task::Ask,
        "ai.suggest" => Task::Suggestions,
        "ai.translate" => Task::Translate,
        _ => Task::Summary,
    };
    app.dialog = Some(crate::Dialog::Assistant);
    if let Some(ctx) = &app.ctx {
        app.apply_theme(ctx);
    }
}
pub(crate) fn suspend(d: &mut Draft) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if d.job.take().is_some() {
            if d.question.is_empty() {
                d.question = std::mem::take(&mut d.pending_question);
            }
            d.status = "Request canceled locally while changing windows. A request already sent may still be processed.".into();
        }
        #[cfg(target_os = "windows")]
        {
            if let Some(host) = &d.browser_host {
                host.hide();
            }
            d.browser_rect = None;
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = d;
}

pub(crate) fn poll(app: &mut PdfCraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        #[cfg(target_os = "windows")]
        {
            app.assistant.browser_rect = None;
        }
        if app.dialog != Some(crate::Dialog::Assistant) {
            app.assistant.cancel();
            app.assistant.browser_open = false;
            #[cfg(target_os = "windows")]
            {
                app.assistant.browser_host = None;
            }
            return;
        }
        let key = app.active_ids().and_then(|(_, id)| app.session.get(id).map(|d| (id, d.edit_generation())));
        if app.assistant.source != key {
            if app.assistant.source.is_some() {
                app.assistant.status = "The document changed. Review its current text before asking again.".into();
            }
            app.assistant.cancel();
            app.assistant.text.clear();
            app.assistant.answer.clear();
            app.assistant.history.clear();
            app.assistant.prepared_pages.clear();
            app.assistant.source = key;
        }
        let mut messages = Vec::new();
        if let Some(job) = &app.assistant.job {
            loop {
                match job.rx.try_recv() {
                    Ok(Message::Done(result)) => {
                        messages.push(Message::Done(result));
                        break;
                    }
                    Ok(message) => messages.push(message),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        messages.push(Message::Done(Err("The Assistant worker disconnected".into())));
                        break;
                    }
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        for message in messages {
            match message {
                Message::Browser(url) => app.open_url(&url),
                Message::Status(message) => app.assistant.status = message,
                Message::Done(result) => {
                    app.assistant.job = None;
                    match result {
                        Ok(update) => app.assistant.accept(update),
                        Err(error) => app.assistant.status = error,
                    }
                }
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    crate::widgets::dialog_heading(ui, "AI Assistant");
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        ui.label("Connect your own AI account in the desktop app.");
        widgets::dialog_footer(ui, |ui| widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked()).inner
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native_body(ui, app)
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn browser_mode(d: &Draft) -> bool {
    d.settings.provider == Provider::CopilotPersonal || (d.settings.browser && d.settings.provider.browser_url().is_some())
}

#[cfg(not(target_arch = "wasm32"))]
fn connections(ui: &mut egui::Ui, d: &mut Draft) {
    use pdfcraft_platform::assistant as platform;
    let ctx = ui.ctx().clone();
    ui.label("Use your own account. PdfCraft includes no shared login or API key.");
    if d.settings.provider.browser_url().is_some() && d.settings.provider != Provider::CopilotPersonal {
        ui.horizontal_wrapped(|ui| {
            ui.selectable_value(&mut d.settings.browser, false, "Native connection");
            ui.selectable_value(&mut d.settings.browser, true, "Browser chat");
        });
    }
    if browser_mode(d) {
        ui.label("Sign in yourself inside the official provider website. Its subscription and usage limits apply. The Windows browser uses a separate PdfCraft profile; it does not import your Edge or Chrome login.");
        ui.label("PDF text is shared only by explicit review and copy/paste. No automatic file access. If the provider blocks embedded sign-in, open it in your browser.");
    } else {
        match d.settings.provider {
            Provider::ChatGpt => {
                ui.label("Eligible users can authorize their own ChatGPT plan. API billing is a separate choice; PdfCraft never switches to it automatically.");
                if !d.profiles.is_empty() {
                    let selected = d
                        .profiles
                        .iter()
                        .find(|p| p.client_id == d.settings.account)
                        .map_or("Choose an account", |p| if p.email.is_empty() { p.subject.as_str() } else { p.email.as_str() });
                    let before = d.settings.account.clone();
                    egui::ComboBox::from_id_salt("assistant-account").selected_text(selected).show_ui(ui, |ui| {
                        for p in &d.profiles {
                            ui.selectable_value(
                                &mut d.settings.account,
                                p.client_id.clone(),
                                if p.email.is_empty() { p.subject.as_str() } else { p.email.as_str() },
                            );
                        }
                    });
                    if before != d.settings.account {
                        d.models.clear();
                        d.settings.model.clear();
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    if widgets::dialog_button(ui, "Load saved accounts", widgets::DialogAction::Back).clicked() {
                        d.connect(&ctx, |_, _| Ok(Update::Profiles(platform::accounts()?, None)));
                    }
                    if widgets::dialog_button(ui, "Continue with ChatGPT", widgets::DialogAction::Confirm).clicked() {
                        let selected = (!d.settings.account.is_empty()).then(|| d.settings.account.clone());
                        d.connect(&ctx, move |cancel, tx| {
                            let mut login = platform::Login::begin(selected.as_deref())?;
                            tx.send(Message::Browser(login.url.clone())).map_err(|_| "Sign-in was closed")?;
                            loop {
                                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                                    return Err("Sign-in canceled".into());
                                }
                                if let Some(profile) = login.poll()? {
                                    return Ok(Update::Profiles(platform::accounts()?, Some(profile.client_id)));
                                }
                                std::thread::sleep(std::time::Duration::from_millis(50));
                            }
                        });
                    }
                    if widgets::dialog_button(ui, "Use another account", widgets::DialogAction::Back).clicked() {
                        d.settings.account.clear();
                        d.models.clear();
                        d.settings.model.clear();
                        d.status = "Continue with ChatGPT to add a separate account.".into();
                    }
                    if !d.settings.account.is_empty()
                        && widgets::dialog_button(ui, "Disconnect ChatGPT", widgets::DialogAction::Destructive).clicked()
                    {
                        let id = d.settings.account.clone();
                        d.connect(&ctx, move |_, _| {
                            let revoked = platform::disconnect(&id, true)?;
                            Ok(Update::Notice(
                                if revoked {
                                    "Disconnected this ChatGPT session."
                                } else {
                                    "Disconnected locally. Remote revocation was not confirmed; disconnect PdfCraft in ChatGPT settings too."
                                }
                                .into(),
                            ))
                        });
                        d.settings.account.clear();
                        d.models.clear();
                        d.settings.model.clear();
                    }
                });
                ui.hyperlink_to("Manage ChatGPT app usage and access", "https://chatgpt.com/#settings/Usage");
            }
            Provider::ClaudeSubscription => {
                ui.label("Uses the official native Claude Code CLI with a separate PdfCraft login for this computer user. Install Claude Code first. Document requests disable tools, MCP, hooks and saved sessions.");
                ui.horizontal_wrapped(|ui| {
                    if widgets::dialog_button(ui, "Sign in to Claude", widgets::DialogAction::Confirm).clicked() {
                        d.connect(&ctx, |cancel, _| {
                            platform::claude_login(&cancel)?;
                            Ok(Update::Notice("Claude sign-in finished. Check your connection before sending text.".into()))
                        });
                    }
                    if widgets::dialog_button(ui, "Check Claude connection", widgets::DialogAction::Back).clicked() {
                        d.connect(&ctx, |cancel, _| {
                            Ok(Update::Notice(
                                if platform::claude_connected(&cancel)? {
                                    "Connected to your Claude subscription."
                                } else {
                                    "No Claude subscription is connected."
                                }
                                .into(),
                            ))
                        });
                    }
                    if widgets::dialog_button(ui, "Disconnect Claude", widgets::DialogAction::Destructive).clicked() {
                        d.connect(&ctx, |cancel, _| {
                            platform::claude_logout(&cancel)?;
                            Ok(Update::Notice("Disconnected the PdfCraft Claude session.".into()))
                        });
                    }
                    ui.hyperlink_to("Get Claude Code", "https://code.claude.com/docs/en/setup");
                });
            }
            Provider::CopilotPersonal => {
                ui.label("Use your own Copilot subscription in browser chat. Microsoft does not offer personal-account access through its Microsoft 365 chat API.");
            }
            Provider::CopilotWork => microsoft_connection(ui, d, &ctx),
            provider => {
                ui.label("Use your own API key. Requests use the provider's API billing, separately from a chat subscription. The key is saved only in this user's OS credential store.");
                let label = ui.label("Personal API key");
                ui.add(widgets::line(&mut d.api_key).password(true).char_limit(8192).desired_width(ui.available_width())).labelled_by(label.id);
                ui.horizontal_wrapped(|ui| {
                    if widgets::dialog_button(ui, "Save personal API key", widgets::DialogAction::Confirm).clicked() {
                        let key = std::mem::take(&mut d.api_key);
                        d.connect(&ctx, move |_, _| {
                            platform::save_api_key(provider, &key)?;
                            Ok(Update::Notice("Your personal API key was saved in the OS credential store.".into()))
                        });
                    }
                    if widgets::dialog_button(ui, "Check saved key", widgets::DialogAction::Back).clicked() {
                        d.connect(&ctx, move |_, _| {
                            Ok(Update::Notice(
                                if platform::api_key_present(provider)? {
                                    "A personal API key is saved for this provider."
                                } else {
                                    "No personal API key is saved."
                                }
                                .into(),
                            ))
                        });
                    }
                    if widgets::dialog_button(ui, "Remove saved key", widgets::DialogAction::Destructive).clicked() {
                        d.connect(&ctx, move |_, _| {
                            platform::forget_api_key(provider)?;
                            Ok(Update::Notice("The saved personal API key was removed.".into()))
                        });
                    }
                });
            }
        }

        if d.settings.provider.requires_model() {
            let label = ui.label("Model ID");
            ui.add(widgets::line(&mut d.settings.model).char_limit(160).desired_width(ui.available_width())).labelled_by(label.id);
            if widgets::dialog_button(ui, "Load available models", widgets::DialogAction::Back).clicked() {
                let p = d.settings.provider;
                let account = d.settings.account.clone();
                d.connect(&ctx, move |_, _| Ok(Update::Models(platform::models(p, &account)?)));
            }
            if !d.models.is_empty() {
                egui::ComboBox::from_id_salt("assistant-model").selected_text("Choose a model").width(ui.available_width()).show_ui(ui, |ui| {
                    for model in &d.models {
                        ui.selectable_value(&mut d.settings.model, model.id.clone(), &model.label);
                    }
                });
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn microsoft_connection(ui: &mut egui::Ui, d: &mut Draft, ctx: &egui::Context) {
    use pdfcraft_platform::assistant as platform;
    ui.label("Microsoft 365 preview API: eligible work/school Copilot license and IT-approved public-client app required. Personal Microsoft accounts are not supported.");
    ui.label("Microsoft requires delegated read access to sites, mail, people, meeting transcripts, chats, channel messages and external items, plus offline sign-in. Ask your IT team before granting these permissions. Web grounding is disabled for each request; organization grounding may still apply.");
    let label = ui.label("Application (client) ID");
    ui.add(widgets::line(&mut d.settings.microsoft.client_id).char_limit(36).desired_width(ui.available_width())).labelled_by(label.id);
    let label = ui.label("Directory (tenant) ID");
    ui.add(widgets::line(&mut d.settings.microsoft.tenant_id).char_limit(36).desired_width(ui.available_width())).labelled_by(label.id);
    egui::ComboBox::from_id_salt("copilot-cloud").selected_text(d.settings.microsoft.cloud.label()).show_ui(ui, |ui| {
        for cloud in [platform::CopilotCloud::Global, platform::CopilotCloud::Government, platform::CopilotCloud::Defense] {
            ui.selectable_value(&mut d.settings.microsoft.cloud, cloud, cloud.label());
        }
    });
    ui.horizontal_wrapped(|ui| {
        if widgets::dialog_button(ui, "Continue with Microsoft 365", widgets::DialogAction::Confirm).clicked() {
            let config = d.settings.microsoft.clone();
            d.connect(ctx, move |cancel, tx| {
                let mut login = platform::CopilotLogin::begin(&config)?;
                let _ = tx.send(Message::Status(format!("Enter code {} at {}. Never give this code to another person.", login.code, login.url)));
                let _ = tx.send(Message::Browser(login.url.clone()));
                loop {
                    if let Some(account) = login.poll(&cancel)? {
                        return Ok(Update::Microsoft(account));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            });
        }
        if widgets::dialog_button(ui, "Check saved sign-in", widgets::DialogAction::Back).clicked() {
            let config = d.settings.microsoft.clone();
            d.connect(ctx, move |_, _| {
                let account = config.account()?;
                let saved = platform::copilot_connected(&account)?;
                if saved {
                    Ok(Update::Microsoft(account))
                } else {
                    Ok(Update::Notice("No Microsoft sign-in is saved for this configuration.".into()))
                }
            });
        }
        if widgets::dialog_button(ui, "Disconnect Microsoft 365", widgets::DialogAction::Destructive).clicked() {
            let config = d.settings.microsoft.clone();
            d.connect(ctx, move |_, _| {
                platform::copilot_disconnect(&config.account()?)?;
                Ok(Update::Notice("Removed the local Microsoft sign-in. Revoke application consent in your Microsoft account if needed.".into()))
            });
            d.settings.account.clear();
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn provider_name(provider: Provider) -> &'static str {
    match provider {
        Provider::ChatGpt => "ChatGPT",
        Provider::ClaudeSubscription => "Claude",
        Provider::CopilotPersonal => "Copilot · personal",
        Provider::CopilotWork => "Copilot · work/school",
        Provider::OpenAi => "OpenAI API",
        Provider::Anthropic => "Claude API",
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn forget_context(d: &mut Draft) {
    d.text.clear();
    d.answer.clear();
    d.history.clear();
    d.prepared_pages.clear();
}

/// Conversation and setup are separate views. Neither opens a provider without a deliberate action.
#[cfg(not(target_arch = "wasm32"))]
fn native_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    widgets::begin_dialog(ui);
    ui.spacing_mut().scroll.floating = false;
    let ctx = ui.ctx().clone();
    let t = crate::theme::Tokens::get(&ctx);
    let active = app.active_ids();
    let doc = active.and_then(|(_, id)| app.session.get(id));
    let d = &mut app.assistant;
    let connection = (d.settings.provider, d.settings.account.clone(), browser_mode(d));
    if d.connection.as_ref().is_some_and(|previous| previous != &connection) {
        d.cancel();
        forget_context(d);
        d.browser_open = false;
    }
    d.connection = Some(connection.clone());
    if !d.settings.enabled {
        if d.job.as_ref().is_some_and(|job| job.network) {
            d.job = None;
        }
        d.browser_open = false;
    }
    let mut privacy_edit = None;
    let mut external = None;
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(d.job.is_none(), |ui| {
            egui::ComboBox::from_id_salt("assistant-provider").selected_text(provider_name(d.settings.provider)).width(220.0).show_ui(ui, |ui| {
                for p in [
                    Provider::ChatGpt,
                    Provider::ClaudeSubscription,
                    Provider::CopilotPersonal,
                    Provider::CopilotWork,
                    Provider::OpenAi,
                    Provider::Anthropic,
                ] {
                    ui.selectable_value(&mut d.settings.provider, p, provider_name(p));
                }
            });
        });
        ui.label(
            egui::RichText::new(if browser_mode(d) {
                "Browser chat · your subscription"
            } else if d.settings.enabled {
                "Native connection"
            } else {
                "Connection not enabled"
            })
            .small()
            .color(t.text_muted),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_pill(ui, "app-window", if d.setup { "Back to chat" } else { "Manage connections" }, false).clicked() {
                d.setup = !d.setup;
            }
        });
    });
    if connection.0 != d.settings.provider {
        d.cancel();
        forget_context(d);
        d.models.clear();
        d.settings.account.clear();
        d.settings.model.clear();
        d.browser_open = false;
        if d.settings.provider == Provider::CopilotWork {
            d.settings.browser = true;
        }
    }
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(8.0);
    if d.setup {
        let height = (ui.available_height() - 62.0).max(70.0);
        ui.vertical_centered(|ui|{
            ui.set_width(ui.available_width().min(900.0));
            ui.with_layout(egui::Layout::top_down(egui::Align::Min),|ui|{
        egui::ScrollArea::vertical().id_salt("assistant-connections").max_height(height).auto_shrink([false,false]).show(ui,|ui|{
            ui.add_space(8.0);
            ui.heading("Your AI connections");
            ui.label("Choose your own subscription or API key. PdfCraft never includes a developer account or switches billing automatically.");
            ui.add_space(12.0);
            ui.add(widgets::checkbox(&mut d.settings.enabled,"Allow Assistant connections"));
            ui.small("Signing in or checking a connection enables it. PDF sharing has its own separate control.");
            ui.add_space(12.0);
            egui::Frame::new().fill(t.panel).stroke(egui::Stroke::new(0.5,t.border)).corner_radius(8.0).inner_margin(18.0).show(ui,|ui|{
                ui.set_width(ui.available_width().min(740.0));
                ui.strong(provider_name(d.settings.provider));ui.add_space(8.0);
                ui.add_enabled_ui(d.job.is_none(),|ui|connections(ui,d));
            });
            ui.add_space(12.0);
            ui.label(egui::RichText::new("API keys and native sign-in tokens stay in your operating system's credential store. Browser sessions use separate PdfCraft profiles.").small().color(t.text_muted));
        });
            });
        });
    } else {
        let before = (d.scope, d.context_enabled);
        let pages = doc
            .map(|doc| match d.scope {
                1 => (0..doc.info.pages.len()).collect::<Vec<_>>(),
                2 => active.and_then(|(i, _)| app.views.get(i)).map(|v| v.target_pages()).unwrap_or_default(),
                _ => vec![active.and_then(|(i, _)| app.views.get(i)).map_or(0, |v| v.current)],
            })
            .unwrap_or_default();
        if !d.prepared_pages.is_empty() && d.prepared_pages != pages {
            d.cancel();
            forget_context(d);
        }
        let allowed = doc
            .ok_or_else(|| "Open a PDF to share document context.".to_string())
            .and_then(|doc| doc.check_ai_pages(&pages).map_err(|e| e.to_string()));
        let height = ui.available_height().max(120.0);
        if ui.available_width() >= 880.0 {
            egui::Panel::right("assistant-document-pane")
                .default_size(280.0)
                .size_range(240.0..=360.0)
                .resizable(true)
                .frame(egui::Frame::new().fill(t.glass_toolbar_shade().unwrap_or(t.panel)).inner_margin(16.0))
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("assistant-document-scroll").max_height(height).auto_shrink([false, false]).show(ui, |ui| {
                        privacy_edit = context_pane(ui, d, doc, &pages, &allowed);
                    });
                });
        } else {
            egui::CollapsingHeader::new("Document context and privacy").id_salt("assistant-context-compact").show(ui, |ui| {
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    privacy_edit = context_pane(ui, d, doc, &pages, &allowed);
                });
            });
        }
        if before != (d.scope, d.context_enabled) {
            d.cancel();
            forget_context(d);
        }
        let ready_context = !d.context_enabled || (allowed.is_ok() && !d.text.trim().is_empty() && d.prepared_pages == pages);
        egui::Frame::new().inner_margin(egui::Margin::symmetric(18,10)).show(ui,|ui|{
            if browser_mode(d){
                ui.horizontal_wrapped(|ui|{
                    if widgets::dialog_button(ui,"Open browser chat",widgets::DialogAction::Confirm).clicked(){d.settings.enabled=true;d.browser_open=true;}
                    if ui.button("Open in external browser").clicked(){d.settings.enabled=true;external=d.settings.provider.browser_url();}
                    if ui.add_enabled(d.context_enabled&&ready_context&&privacy_edit.is_none(),egui::Button::new("Copy reviewed PDF context")).clicked(){
                        match assistant::prompt(Task::Ask,"Use the following reviewed PDF context to answer my questions with page references.",&d.text){Ok(prompt)=>{ctx.copy_text(format!("{}\n\n{prompt}",assistant::INSTRUCTIONS));d.status="Context copied. Paste it into the provider chat yourself to share it.".into();},Err(error)=>d.status=error}
                    }
                });
                ui.small("Sign in to the official provider website. PDF text is shared only by your explicit copy/paste; there is no automatic file access.");
                let available=(ui.available_height()-32.0).max(100.0);
                let(rect,_)=ui.allocate_exact_size(egui::vec2(ui.available_width(),available),egui::Sense::hover());
                if d.browser_open&&d.settings.enabled{
                    #[cfg(target_os="windows")]
                    if ctx.viewport_id()==egui::ViewportId::ROOT&&ui.clip_rect().contains_rect(rect){d.browser_rect=Some(rect);}else{
                        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Use Open in external browser in a detached window.",egui::FontId::proportional(15.0),t.text_muted);
                    }
                    #[cfg(not(target_os="windows"))]
                    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Use Open in external browser on this platform.",egui::FontId::proportional(15.0),t.text_muted);
                }else{
                    ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,"Your subscription, inside PdfCraft\nOpen browser chat to get started.",egui::FontId::proportional(18.0),t.text_muted);
                }
                #[cfg(target_os="windows")]
                if let Some(host)=&d.browser_host{ui.small(host.origin());}
            }else{
                let conversation_height=(ui.available_height()-155.0).max(60.0);
                egui::ScrollArea::vertical().id_salt("assistant-transcript").max_height(conversation_height).min_scrolled_height(conversation_height).auto_shrink([false,false]).stick_to_bottom(!d.history.is_empty()).show(ui,|ui|{
                    let width=ui.available_width().min(820.0);
                    ui.vertical_centered(|ui|{
                        ui.set_width(width);
                        if d.history.is_empty(){
                            ui.add_space(24.0);ui.heading("A clearer view of your PDF");
                            ui.label("Summarize, ask questions or work through edits. You control what the Assistant can read.");ui.add_space(18.0);
                            ui.horizontal_wrapped(|ui|{
                                for (label,prompt) in [("Summarize","Summarize this PDF's main points with page references."),("Suggest edits","Suggest practical improvements to the reviewed text, with page references."),("Ask a question",""),("Translate","Translate the reviewed PDF text into ")]{
                                    if ui.add_sized([145.0,42.0],egui::Button::new(label).frame(true).fill(t.panel).stroke(egui::Stroke::new(0.5,t.border))).clicked(){d.task=Task::Ask;d.question=prompt.into();}
                                }
                            });
                            if !d.settings.enabled&&ui.button("Set up a connection").clicked(){d.setup=true;}
                        }
                        for (question,answer) in &d.history{
                            ui.with_layout(egui::Layout::top_down(egui::Align::Min),|ui|{
                                egui::Frame::new().fill(t.hover).inner_margin(12.0).corner_radius(8.0).show(ui,|ui|{ui.set_width(ui.available_width());ui.strong("You");ui.label(question);});
                                ui.add_space(10.0);ui.strong(provider_name(d.settings.provider));ui.label(answer);ui.add_space(18.0);
                            });
                        }
                    });
                });
                ui.add_space(8.0);
                egui::Frame::new().fill(t.field).stroke(egui::Stroke::new(1.0,t.border)).corner_radius(10.0).inner_margin(12.0).show(ui,|ui|{
                    let label=ui.label("Message");
                    ui.add(widgets::block(&mut d.question).hint_text("Ask about your PDF…").desired_width(ui.available_width()).desired_rows(2).char_limit(assistant::MAX_QUESTION_BYTES)).labelled_by(label.id);
                    ui.horizontal_wrapped(|ui|{
                        ui.label(egui::RichText::new(if d.context_enabled{if ready_context{"Reviewed PDF context attached"}else{"Review document context before sending"}}else{"PDF context is off"}).small().color(t.text_muted));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                            let account_ready=d.settings.provider!=Provider::ChatGpt||!d.settings.account.is_empty();
                            let ready=d.settings.enabled&&d.job.is_none()&&privacy_edit.is_none()&&ready_context&&account_ready&&(!d.settings.provider.requires_model()||pdfcraft_platform::assistant::check_model(&d.settings.model).is_ok())&&!d.question.trim().is_empty();
                            let send=ui.add_enabled_ui(ready,|ui|widgets::dialog_button(ui,"Send",widgets::DialogAction::Confirm)).inner.clicked();
                            let shortcut=ready&&ui.input_mut(|i|i.consume_key(egui::Modifiers::CTRL,egui::Key::Enter));
                            if send||shortcut{send_message(d,&ctx);}
                            if d.job.is_some()&&ui.button("Stop").clicked(){d.cancel();d.status="Stopped locally. The provider may already have processed a request that was sent.".into();}
                            if ui.button("New chat").clicked(){d.cancel();d.history.clear();d.answer.clear();}
                        });
                    });
                });
                if !d.answer.is_empty()&&ui.small_button("Copy last answer").clicked(){ctx.copy_text(d.answer.clone());}
            }
        });
    }
    if !d.status.is_empty() {
        ui.separator();
        ui.add(egui::Label::new(egui::RichText::new(&d.status).small().color(t.text_muted)).truncate()).on_hover_text(&d.status);
    }
    if d.job.is_some() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
    }
    if connection.2 != browser_mode(d) {
        forget_context(d);
        d.browser_open = false;
    }
    if let Some(url) = external {
        app.open_url(url);
    }
    if let Some(edit) = privacy_edit {
        app.assistant.cancel();
        app.apply_edit(edit);
    }
    false
}

#[cfg(not(target_arch = "wasm32"))]
fn context_pane(
    ui: &mut egui::Ui,
    d: &mut Draft,
    doc: Option<&pdfcraft_engine::Document>,
    pages: &[usize],
    allowed: &Result<(), String>,
) -> Option<pdfcraft_engine::Edit> {
    let ctx = ui.ctx().clone();
    let t = crate::theme::Tokens::get(&ctx);
    let mut edit = None;
    ui.strong("Document context");
    ui.add_space(6.0);
    if let Some(doc) = doc {
        ui.add(egui::Label::new(&doc.name).truncate()).on_hover_text(&doc.name);
    } else {
        ui.label("No PDF is open");
    }
    ui.add_space(10.0);
    ui.add(widgets::checkbox(&mut d.context_enabled, "Share PDF context"));
    ui.small("Off until you enable it. Review the text before sending or copying.");
    ui.add_space(10.0);
    egui::ComboBox::from_id_salt("assistant-scope")
        .selected_text(match d.scope {
            1 => "All pages",
            2 => "Selected pages",
            _ => "Current page",
        })
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            for (value, label) in [(0, "Current page"), (1, "All pages"), (2, "Selected pages")] {
                ui.selectable_value(&mut d.scope, value, label);
            }
        });
    if d.context_enabled {
        if let Err(error) = allowed {
            ui.label(egui::RichText::new(error).small().color(t.text_muted));
            d.text.clear();
            d.prepared_pages.clear();
        }
        if ui.add_enabled(d.job.is_none() && allowed.is_ok(), egui::Button::new("Review PDF context")).clicked()
            && let (Some(doc), Some(key)) = (doc, d.source)
        {
            match assistant::Source::new(doc, pages) {
                Ok(source) => {
                    d.prepared_pages = pages.to_vec();
                    d.start(&ctx, move |cancel, _| Ok(Update::Text(source.prepare(|| cancel.load(std::sync::atomic::Ordering::Relaxed))?, key)));
                }
                Err(error) => d.status = error,
            }
        }
        if !d.text.is_empty() {
            ui.add_space(8.0);
            ui.label("Reviewed text");
            if ui.add(widgets::block(&mut d.text).desired_width(ui.available_width()).desired_rows(8).char_limit(assistant::MAX_TEXT_BYTES)).changed()
            {
                d.history.clear();
                d.answer.clear();
            }
        }
    }
    ui.add_space(16.0);
    ui.separator();
    ui.add_space(8.0);
    ui.strong("Confidential");
    ui.small("Saved in the PDF. Blocks PdfCraft's AI sharing for this document or chosen pages.");
    if let Some(doc) = doc {
        match doc.confidentiality() {
            Ok(policy) => {
                let mut confidential = policy.document;
                if ui.add(widgets::checkbox(&mut confidential, "Confidential document")).changed() {
                    edit = Some(pdfcraft_engine::Edit::SetAiConfidential { pages: None, confidential });
                }
                ui.small(format!("{} page(s) individually marked", policy.pages.len()));
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Mark pages").clicked() {
                        edit = Some(pdfcraft_engine::Edit::SetAiConfidential { pages: Some(pages.to_vec()), confidential: true });
                    }
                    if ui.button("Allow AI for pages").clicked() {
                        edit = Some(pdfcraft_engine::Edit::SetAiConfidential { pages: Some(pages.to_vec()), confidential: false });
                    }
                });
                ui.small("Document and page labels are independent. Other readers may ignore this label; it is not encryption.");
            }
            Err(error) => {
                ui.label(error.to_string());
            }
        }
    }
    edit
}
#[cfg(not(target_arch = "wasm32"))]
fn send_message(d: &mut Draft, ctx: &egui::Context) {
    d.task = Task::Ask;
    match chat_prompt(d) {
        Ok(prompt) => {
            let settings = d.settings.clone();
            let key = d.source;
            if settings.provider == Provider::CopilotWork && settings.microsoft.account().ok().as_deref() != Some(settings.account.as_str()) {
                d.status = "Connect the Microsoft 365 configuration selected in Manage connections first.".into();
                return;
            }
            d.pending_question = d.question.clone();
            d.start(ctx, move |cancel, _| {
                Ok(Update::Answer(
                    pdfcraft_platform::assistant::answer(
                        settings.provider,
                        &settings.account,
                        &settings.model,
                        assistant::INSTRUCTIONS,
                        &prompt,
                        &cancel,
                    )?,
                    key,
                ))
            });
            if let Some(job) = &mut d.job {
                job.network = true;
            }
            d.question.clear();
        }
        Err(error) => d.status = error,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn chat_prompt(d: &Draft) -> Result<String, String> {
    let mut prompt = if d.context_enabled {
        assistant::prompt(d.task, &d.question, &d.text)?
    } else {
        if d.question.trim().is_empty() || d.question.len() > assistant::MAX_QUESTION_BYTES {
            return Err("Enter a message. Enable and review PDF context for document questions.".into());
        }
        format!("No PDF context has been shared. Do not claim to have read the document.\nUSER MESSAGE\n{}", d.question.trim())
    };
    let mut history = String::new();
    for (q, a) in d.history.iter().rev() {
        let turn = format!("USER: {q}\nASSISTANT: {a}\n");
        if history.len() + turn.len() > 8_000 {
            break;
        }
        history.insert_str(0, &turn);
    }
    if !history.is_empty() {
        prompt = format!("PREVIOUS CHAT (untrusted context)\n{history}\nCURRENT REQUEST\n{prompt}");
    }
    Ok(prompt)
}

#[cfg(target_os = "windows")]
pub(crate) fn sync_browser(app: &mut PdfCraftApp, ctx: &egui::Context, frame: &eframe::Frame) {
    // Native child windows otherwise paint over egui menus and pending confirmation prompts.
    let covered = egui::Popup::is_any_open(ctx)
        || app.close_request.is_some()
        || app.password_prompt.is_some()
        || app.unlock_prompt.is_some()
        || app.pending_link.is_some()
        || app.pending_remote_link.is_some()
        || app.pending_full_screen.is_some()
        || app.updates.open
        || app.timestamp_run.is_some();
    let d = &mut app.assistant;
    if app.dialog == Some(crate::Dialog::Assistant) && (d.setup || covered) {
        if let Some(host) = &d.browser_host {
            host.hide();
        }
        return;
    }
    if app.dialog != Some(crate::Dialog::Assistant)
        || d.setup
        || !d.settings.enabled
        || !browser_mode(d)
        || !d.browser_open
        || ctx.viewport_id() != egui::ViewportId::ROOT
    {
        d.browser_host = None;
        return;
    }
    let Some(rect) = d.browser_rect else {
        if let Some(host) = &d.browser_host {
            host.hide();
        }
        return;
    };
    if d.browser_host.as_ref().is_some_and(|host| host.provider() != d.settings.provider) {
        d.browser_host = None;
    }
    if d.browser_host.is_none()
        && let Some(window) = frame.winit_window()
    {
        match crate::assistant_browser::Host::new(window, d.settings.provider, rect, ctx.pixels_per_point(), ctx) {
            Ok(host) => d.browser_host = Some(host),
            Err(error) => {
                d.status = format!("Embedded chat could not start: {error}. Use Open in external browser.");
                d.browser_open = false;
                ctx.request_repaint();
            }
        }
    }
    if let Some(host) = &mut d.browser_host
        && let Err(error) = host.show(rect, ctx.pixels_per_point())
    {
        d.status = error;
        d.browser_open = false;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    #[test]
    fn assistant_popup_is_readable_with_pinned_composer_and_taskbar() {
        for size in [egui::vec2(1200.0, 800.0), egui::vec2(640.0, 480.0)] {
            for theme in ["light", "dark"] {
                for setup in [false, true] {
                    let mut h = Harness::builder().with_size(size).build_eframe(move |_| {
                        let mut app = PdfCraftApp::new();
                        app.set_option("language", "en").unwrap();
                        app.set_option("theme", theme).unwrap();
                        app.dialog = Some(crate::Dialog::Assistant);
                        app.assistant.setup = setup;
                        app.assistant.task = Task::Ask;
                        app.assistant.settings.model = "example-model".into();
                        app
                    });
                    h.run_steps(8);
                    assert!(!crate::dialog_windows::maximized(&h.ctx, "dialog"));
                    let card = h.ctx.data(|d| d.get_temp::<egui::Rect>(egui::Id::new("dialog").with("card"))).unwrap();
                    assert!(card.width() < size.x && card.height() < size.y, "chat stays inside a popup: {card:?}");
                    assert!(h.ctx.memory(|m| m.top_modal_layer().is_none()), "chat must not add a full-window dimming layer");
                    h.get_by_label("Minimize AI Assistant");
                    if !setup {
                        let message = h.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, "Message").rect();
                        assert!(message.width() > 300.0, "composer {message:?}");
                        assert!(message.bottom() < size.y - 30.0, "composer must stay above the taskbar: {message:?}");
                        h.get_by_label("Send");
                    } else {
                        h.get_by_label("Continue with ChatGPT");
                    }
                    if let Ok(dir) = std::env::var("PDFCRAFT_POPUP_SHOTS") {
                        h.render().unwrap().save(format!("{dir}/assistant-popup-{theme}-{}-{setup}.png", size.x)).unwrap();
                    }
                    assert!(h.state().assistant.job.is_none(), "layout cannot contact a provider");
                }
            }
        }
    }
    #[test]
    fn assistant_open_and_restore_keep_the_live_appearance() {
        for theme in ["light", "dark", "glass"] {
            let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).build_eframe(move |_| {
                let mut app = PdfCraftApp::new();
                app.set_option("language", "en").unwrap();
                app.set_option("theme", theme).unwrap();
                app.set_glass_settings(crate::theme::GlassSettings { tint: [48, 127, 154], opacity: 47, popup_opacity: 83, tool_opacity: 29 });
                let bytes = app.session.create_blank(612.0, 792.0, 1).unwrap();
                app.open_bytes("Behind-chat.pdf", None, bytes.as_ref().clone()).unwrap();
                app
            });
            h.run_steps(5);
            let preference = h.state().theme_preference;
            let glass = h.state().glass;
            let fill = h.ctx.global_style().visuals.window_fill;
            h.ctx.set_visuals(egui::Visuals::light());
            h.state_mut().execute("ai.ask");
            h.run_steps(4);
            assert_eq!(h.ctx.global_style().visuals.window_fill, fill);
            assert_eq!(h.state().theme_preference, preference);
            assert_eq!(h.state().glass, glass);
            assert!(h.state().views[0].page_screen_rect(0).is_some());
            h.state_mut().execute("app.dialog_minimize");
            h.run_steps(3);
            h.ctx.set_visuals(egui::Visuals::light());
            h.state_mut().execute("app.dialog_restore");
            h.run_steps(4);
            assert_eq!(h.ctx.global_style().visuals.window_fill, fill);
            assert_eq!(h.state().theme_preference, preference);
            assert_eq!(h.state().glass, glass);
            let mut restored = PdfCraftApp::new();
            restored.restore(&h.state().persist());
            assert_eq!(restored.theme_preference, preference);
            assert_eq!(restored.glass, glass);
        }
    }
    #[test]
    fn chat_shortcuts_do_not_close_or_edit_the_hidden_pdf() {
        let mut h = Harness::builder().build_eframe(|_| {
            let mut app = PdfCraftApp::new();
            let bytes = app.session.create_from_text("Synthetic", "Keep these words.").unwrap();
            app.open_bytes("synthetic.pdf", None, bytes.as_ref().clone()).unwrap();
            let id = app.views[0].id;
            app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
            app.dialog = Some(crate::Dialog::Assistant);
            app
        });
        h.run_steps(4);
        let id = h.state().views[0].id;
        let before = h.state().session.get(id).unwrap().bytes.clone();
        for key in [egui::Key::Z, egui::Key::W, egui::Key::L] {
            h.key_press_modifiers(egui::Modifiers::COMMAND, key);
            h.run_steps(2);
        }
        assert_eq!(h.state().views.len(), 1);
        assert_eq!(h.state().session.get(id).unwrap().bytes, before);
        assert!(h.state().close_request.is_none());
    }
    #[test]
    fn finished_jobs_keep_their_result_and_changed_documents_reject_stale_text() {
        let mut app = PdfCraftApp::new();
        let bytes = app.session.create_from_text("Synthetic", "Reviewed words.").unwrap();
        app.open_bytes("synthetic.pdf", None, bytes.as_ref().clone()).unwrap();
        let id = app.views[0].id;
        let key = (id, app.session.get(id).unwrap().edit_generation());
        app.dialog = Some(crate::Dialog::Assistant);
        app.assistant.source = Some(key);
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Message::Done(Ok(Update::Text("Reviewed words.".into(), key)))).unwrap();
        drop(tx);
        app.assistant.job = Some(Job { rx, cancel: Default::default(), network: false });
        poll(&mut app, &egui::Context::default());
        assert_eq!(app.assistant.text, "Reviewed words.");
        assert!(app.assistant.job.is_none());
        assert!(app.assistant.status.contains("Nothing has been sent"));
        app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Message::Done(Ok(Update::Answer("Stale answer".into(), Some(key))))).unwrap();
        drop(tx);
        app.assistant.job = Some(Job { rx, cancel: Default::default(), network: false });
        poll(&mut app, &egui::Context::default());
        assert!(app.assistant.text.is_empty() && app.assistant.answer.is_empty());
        assert!(app.assistant.status.contains("document changed"));
    }

    #[test]
    fn setup_has_no_bundled_account_and_never_persists_a_key_or_document_text() {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.assistant.api_key = "private-key-never-serialize".into();
        app.assistant.text = "private-pdf-never-serialize".into();
        let saved = app.persist();
        assert!(!saved.contains("private-key-never-serialize") && !saved.contains("private-pdf-never-serialize"));
        assert!(!app.assistant.settings.enabled && app.assistant.settings.account.is_empty());
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_| app);
        assert!(h.state_mut().execute("ai.setup"));
        h.run_steps(5);
        h.get_by_label("Continue with ChatGPT");
        assert!(h.state().assistant.job.is_none(), "opening setup makes no request");
        if let Ok(dir) = std::env::var("PDFCRAFT_ASSISTANT_SHOTS") {
            for theme in ["light", "dark"] {
                h.state_mut().set_option("theme", theme).unwrap();
                h.run_steps(6);
                h.render().unwrap().save(format!("{dir}/assistant-setup-{theme}.png")).unwrap();
            }
        }
    }
    #[test]
    fn confidential_changes_cancel_context_jobs_and_chat_never_persists_pdf_text() {
        let mut app = PdfCraftApp::new();
        let bytes = app.session.create_from_text("Synthetic", "Private source words.").unwrap();
        app.open_bytes("privacy-chat.pdf", None, bytes.as_ref().clone()).unwrap();
        let id = app.views[0].id;
        let key = (id, app.session.get(id).unwrap().edit_generation());
        app.dialog = Some(crate::Dialog::Assistant);
        app.assistant.source = Some(key);
        app.assistant.text = "Private source words.".into();
        app.assistant.prepared_pages = vec![0];
        app.assistant.history.push(("Explain".into(), "Private answer.".into()));
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (_tx, rx) = std::sync::mpsc::channel();
        app.assistant.job = Some(Job { rx, cancel: flag.clone(), network: false });
        app.session.apply(id, pdfcraft_engine::Edit::SetAiConfidential { pages: None, confidential: true }).unwrap();
        poll(&mut app, &egui::Context::default());
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(app.assistant.job.is_none() && app.assistant.text.is_empty() && app.assistant.history.is_empty());
        assert!(assistant::Source::new(app.session.get(id).unwrap(), &[0]).is_err());
        let saved = app.persist();
        assert!(!saved.contains("Private source words") && !saved.contains("Private answer"));
        assert!(!app.assistant.context_enabled);
    }
    #[test]
    fn chat_requests_are_bounded_and_do_not_silently_include_pdf_context() {
        let mut d = Draft { text: "A PRIVATE PDF".into(), question: "Hello".into(), ..Default::default() };
        let prompt = chat_prompt(&d).unwrap();
        assert!(!prompt.contains("A PRIVATE PDF"));
        d.context_enabled = true;
        d.task = Task::Ask;
        assert!(chat_prompt(&d).unwrap().contains("A PRIVATE PDF"));
        d.question = "x".repeat(assistant::MAX_QUESTION_BYTES + 1);
        assert!(chat_prompt(&d).is_err());
        d.question = "Hello".into();
        d.history = vec![("Old".into(), "x".repeat(9000))];
        assert!(!chat_prompt(&d).unwrap().contains("Old"));
    }
    #[test]
    fn browser_chat_is_explicit_and_confidential_context_cannot_be_copied() {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        let bytes = app.session.create_from_text("Synthetic", "Do not share these words.").unwrap();
        app.open_bytes("confidential-demo.pdf", None, bytes.as_ref().clone()).unwrap();
        let id = app.views[0].id;
        app.session.apply(id, pdfcraft_engine::Edit::SetAiConfidential { pages: None, confidential: true }).unwrap();
        app.dialog = Some(crate::Dialog::Assistant);
        app.assistant.settings.provider = Provider::CopilotPersonal;
        app.assistant.settings.enabled = true;
        app.assistant.context_enabled = true;
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).build_eframe(move |_| app);
        h.run_steps(8);
        h.get_by_label("Open browser chat");
        h.get_by_label("Copy reviewed PDF context");
        assert!(h.state().assistant.job.is_none());
        assert!(!h.state().assistant.browser_open);
        assert!(h.state().assistant.text.is_empty());
        assert!(h.ctx.output(|o| o.commands.is_empty()), "layout must not copy text or open a browser");
    }
    #[test]
    fn minimized_ai_restores_as_a_popup_without_old_pdf_context() {
        let mut h = Harness::builder().with_size(egui::vec2(1280.0, 900.0)).build_eframe(|_| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            let bytes = app.session.create_from_text("Synthetic", "Private source.").unwrap();
            app.open_bytes("source.pdf", None, bytes.as_ref().clone()).unwrap();
            app.execute("ai.ask");
            app
        });
        h.run_steps(6);
        h.state_mut().assistant.text = "Private source context".into();
        h.state_mut().execute("app.dialog_minimize");
        h.run_steps(3);
        assert!(crate::dialog_windows::describe(Some(&h.ctx)).as_array().unwrap().iter().any(|e| e["minimized"] == true));
        let bytes = h.state().session.create_from_text("Other", "Different source.").unwrap();
        h.state_mut().open_bytes("other.pdf", None, bytes.as_ref().clone()).unwrap();
        let current = h.state().active_ids().unwrap().1;
        h.state_mut().execute("app.dialog_restore");
        h.run_steps(6);
        assert_eq!(h.state().active_ids().unwrap().1, current);
        assert_eq!(h.state().dialog, Some(crate::Dialog::Assistant));
        assert!(!crate::dialog_windows::maximized(&h.ctx, "dialog"));
        assert!(h.state().assistant.text.is_empty());
        assert!(h.ctx.memory(|m| m.top_modal_layer().is_none()));
        h.get_by_label("Send");
    }
}
