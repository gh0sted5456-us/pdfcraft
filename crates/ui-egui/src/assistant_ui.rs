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
}
#[cfg(not(target_arch = "wasm32"))]
enum Update {
    Text(String, (DocId, u64)),
    Answer(String, (DocId, u64)),
    Models(Vec<pdfcraft_platform::assistant::Model>),
    Profiles(Vec<pdfcraft_platform::assistant::Profile>, Option<String>),
    Notice(String),
}
#[cfg(not(target_arch = "wasm32"))]
enum Message {
    Browser(String),
    Done(Result<Update, String>),
}
#[cfg(not(target_arch = "wasm32"))]
struct Job {
    rx: std::sync::mpsc::Receiver<Message>,
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
        self.job = Some(Job { rx, cancel });
        self.status = "Working…".into();
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn accept(&mut self, update: Update) {
        match update {
            Update::Text(text, key) if self.source == Some(key) => {
                self.text = text;
                self.answer.clear();
                self.status = "Review the text below. Nothing has been sent.".into();
            }
            Update::Answer(text, key) if self.source == Some(key) => {
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
}
pub(crate) fn poll(app: &mut PdfCraftApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if app.dialog != Some(crate::Dialog::Assistant) {
            app.assistant.cancel();
            return;
        }
        let key = app.active_ids().and_then(|(_, id)| app.session.get(id).map(|d| (id, d.edit_generation())));
        if app.assistant.source != key {
            app.assistant.text.clear();
            app.assistant.answer.clear();
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
    ui.heading("AI Assistant");
    #[cfg(target_arch = "wasm32")]
    {
        let _ = app;
        ui.label("Connect your own AI account in the desktop app.");
        widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        native_body(ui, app)
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn native_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    use pdfcraft_platform::assistant as platform;
    let active = app.active_ids();
    let context = active.and_then(|(i, id)| {
        app.session.get(id).map(|doc| (doc.id, doc.edit_generation(), doc.info.pages.len(), app.views[i].current, app.views[i].target_pages()))
    });
    let session = &app.session;
    let source = |scope: u8| match &context {
        Some((id, _, count, current, selected)) => {
            let pages = match scope {
                1 if *count <= assistant::MAX_PAGES => (0..*count).collect(),
                1 => Vec::new(),
                2 => selected.clone(),
                _ => vec![*current],
            };
            session.get(*id).ok_or_else(|| "The document was closed".to_string()).and_then(|doc| assistant::Source::new(doc, &pages))
        }
        None => Err("Open a PDF to review its text".into()),
    };
    let ctx = ui.ctx().clone();
    let d = &mut app.assistant;
    let busy = d.job.is_some();
    ui.label("Connect your own account. PdfCraft includes no shared AI login or API key.");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut d.setup, true, "Connections");
        ui.selectable_value(&mut d.setup, false, "Document assistant");
    });
    ui.separator();
    let height = (ui.ctx().content_rect().height() - 240.0).clamp(120.0, 610.0);
    egui::ScrollArea::vertical().id_salt("assistant-body").max_height(height).show(ui,|ui|{
        ui.add_enabled_ui(!busy,|ui|{
            ui.add(widgets::checkbox(&mut d.settings.enabled,"Enable Assistant requests (off by default)"));
            let before=d.settings.provider;
            let label=ui.label("AI connection");
            egui::ComboBox::from_id_salt("assistant-provider").selected_text(d.settings.provider.label()).width(390.0).show_ui(ui,|ui|{
                for p in [Provider::ChatGpt,Provider::ClaudeSubscription,Provider::OpenAi,Provider::Anthropic]{ui.selectable_value(&mut d.settings.provider,p,p.label());}
            }).response.labelled_by(label.id);
            if before!=d.settings.provider {d.models.clear();d.settings.model.clear();d.api_key.clear();d.status.clear();}
            if d.setup {
                match d.settings.provider {
                    Provider::ChatGpt=>{
                        ui.label("Eligible users can authorize their own ChatGPT plan. API billing is a separate choice; PdfCraft never switches to it automatically.");
                        if !d.profiles.is_empty(){
                            let selected=d.profiles.iter().find(|p|p.client_id==d.settings.account).map_or("Choose an account",|p|if p.email.is_empty(){p.subject.as_str()}else{p.email.as_str()});
                            let before=d.settings.account.clone();
                            egui::ComboBox::from_id_salt("assistant-account").selected_text(selected).show_ui(ui,|ui|{for p in &d.profiles{ui.selectable_value(&mut d.settings.account,p.client_id.clone(),if p.email.is_empty(){p.subject.as_str()}else{p.email.as_str()});}});
                            if before!=d.settings.account{d.models.clear();d.settings.model.clear();}
                        }
                        ui.horizontal_wrapped(|ui|{
                            if widgets::dialog_button(ui,"Load saved accounts",widgets::DialogAction::Back).clicked(){d.start(&ctx,|_,_|Ok(Update::Profiles(platform::accounts()?,None)));}
                            if widgets::dialog_button(ui,"Continue with ChatGPT",widgets::DialogAction::Confirm).clicked(){
                                let selected=(!d.settings.account.is_empty()).then(||d.settings.account.clone());
                                d.start(&ctx,move|cancel,tx|{
                                    let mut login=platform::Login::begin(selected.as_deref())?;
                                    tx.send(Message::Browser(login.url.clone())).map_err(|_|"Sign-in was closed")?;
                                    loop{
                                        if cancel.load(std::sync::atomic::Ordering::Relaxed){return Err("Sign-in canceled".into())}
                                        if let Some(profile)=login.poll()?{return Ok(Update::Profiles(platform::accounts()?,Some(profile.client_id)))}
                                        std::thread::sleep(std::time::Duration::from_millis(50));
                                    }
                                });
                            }
                            if widgets::dialog_button(ui,"Use another account",widgets::DialogAction::Back).clicked(){d.settings.account.clear();d.models.clear();d.settings.model.clear();d.status="Continue with ChatGPT to add a separate account.".into();}
                            if !d.settings.account.is_empty() && widgets::dialog_button(ui,"Disconnect ChatGPT",widgets::DialogAction::Destructive).clicked(){
                                let id=d.settings.account.clone();d.start(&ctx,move|_,_|{
                                    let revoked=platform::disconnect(&id,true)?;
                                    Ok(Update::Notice(if revoked{"Disconnected this ChatGPT session."}else{"Disconnected locally. Remote revocation was not confirmed; disconnect PdfCraft in ChatGPT settings too."}.into()))
                                });d.settings.account.clear();d.models.clear();d.settings.model.clear();
                            }
                        });
                        ui.hyperlink_to("Manage ChatGPT app usage and access","https://chatgpt.com/#settings/Usage");
                    }
                    Provider::ClaudeSubscription=>{
                        ui.label("Uses the official native Claude Code CLI with a separate PdfCraft login for this computer user. Install Claude Code first. Document requests disable tools, MCP, hooks and saved sessions.");
                        ui.horizontal_wrapped(|ui|{
                            if widgets::dialog_button(ui,"Sign in to Claude",widgets::DialogAction::Confirm).clicked(){d.start(&ctx,|cancel,_|{platform::claude_login(&cancel)?;Ok(Update::Notice("Claude sign-in finished. Check your connection before sending text.".into()))});}
                            if widgets::dialog_button(ui,"Check Claude connection",widgets::DialogAction::Back).clicked(){d.start(&ctx,|cancel,_|Ok(Update::Notice(if platform::claude_connected(&cancel)?{"Connected to your Claude subscription."}else{"No Claude subscription is connected."}.into())));}
                            if widgets::dialog_button(ui,"Disconnect Claude",widgets::DialogAction::Destructive).clicked(){d.start(&ctx,|cancel,_|{platform::claude_logout(&cancel)?;Ok(Update::Notice("Disconnected the PdfCraft Claude session.".into()))});}
                            ui.hyperlink_to("Get Claude Code","https://code.claude.com/docs/en/setup");
                        });
                    }
                    provider=>{
                        ui.label("Use your own API key. Requests use the provider's API billing, separately from a chat subscription. The key is saved only in this user's OS credential store.");
                        let label=ui.label("Personal API key");ui.add(widgets::line(&mut d.api_key).password(true).char_limit(8192).desired_width(480.0)).labelled_by(label.id);
                        ui.horizontal_wrapped(|ui|{
                            if widgets::dialog_button(ui,"Save personal API key",widgets::DialogAction::Confirm).clicked(){let key=std::mem::take(&mut d.api_key);d.start(&ctx,move|_,_|{platform::save_api_key(provider,&key)?;Ok(Update::Notice("Your personal API key was saved in the OS credential store.".into()))});}
                            if widgets::dialog_button(ui,"Check saved key",widgets::DialogAction::Back).clicked(){d.start(&ctx,move|_,_|Ok(Update::Notice(if platform::api_key_present(provider)?{"A personal API key is saved for this provider."}else{"No personal API key is saved."}.into())));}
                            if widgets::dialog_button(ui,"Remove saved key",widgets::DialogAction::Destructive).clicked(){d.start(&ctx,move|_,_|{platform::forget_api_key(provider)?;Ok(Update::Notice("The saved personal API key was removed.".into()))});}
                        });
                    }
                }
            }
            let label=ui.label("Model ID");ui.add(widgets::line(&mut d.settings.model).char_limit(160).desired_width(420.0)).labelled_by(label.id);
            if widgets::dialog_button(ui,"Load available models",widgets::DialogAction::Back).clicked(){let p=d.settings.provider;let account=d.settings.account.clone();d.start(&ctx,move|_,_|Ok(Update::Models(platform::models(p,&account)?)));}
            if !d.models.is_empty(){egui::ComboBox::from_id_salt("assistant-model").selected_text(if d.settings.model.is_empty(){"Choose a model"}else{d.settings.model.as_str()}).width(420.0).show_ui(ui,|ui|{for model in &d.models{ui.selectable_value(&mut d.settings.model,model.id.clone(),&model.label);}});}
            if !d.setup {
                ui.separator();
                ui.horizontal_wrapped(|ui|{for task in [Task::Summary,Task::Suggestions,Task::Ask,Task::Translate]{ui.selectable_value(&mut d.task,task,task.label());}});
                if matches!(d.task,Task::Ask|Task::Translate){let label=ui.label(if d.task==Task::Ask{"Your question"}else{"Target language"});ui.add(widgets::line(&mut d.question).char_limit(assistant::MAX_QUESTION_BYTES).desired_width(560.0)).labelled_by(label.id);}
                ui.horizontal_wrapped(|ui|{ui.label("Document pages");for (value,label) in [(0,"Current page"),(1,"All pages"),(2,"Selected pages")]{widgets::radio_value(ui,&mut d.scope,value,label);}});
                if widgets::dialog_button(ui,"Review PDF text",widgets::DialogAction::Back).clicked(){match source(d.scope){Ok(source)=>{if let Some(key)=d.source{d.start(&ctx,move|cancel,_|Ok(Update::Text(source.prepare(||cancel.load(std::sync::atomic::Ordering::Relaxed))?,key)));}},Err(error)=>d.status=error}}
                ui.label("Only the reviewed text and your request go to the selected provider when you press Send. You can remove sensitive passages here. Scanned pages need OCR first.");
                let label=ui.label("Text to send");ui.add(widgets::block(&mut d.text).desired_width(620.0).desired_rows(8).char_limit(assistant::MAX_TEXT_BYTES)).labelled_by(label.id);
                if !d.answer.is_empty(){ui.separator();ui.label("Assistant answer — review before editing");ui.add(widgets::block(&mut d.answer).desired_width(620.0).desired_rows(9).interactive(false));if widgets::dialog_button(ui,"Copy answer",widgets::DialogAction::Back).clicked(){ui.ctx().copy_text(d.answer.clone());}}
            }
        });
        if !d.status.is_empty(){ui.separator();ui.label(&d.status);}
    });
    ui.separator();
    let mut close = false;
    widgets::dialog_footer(ui, |ui| {
        let ready = !d.setup && !busy && d.settings.enabled && !d.text.trim().is_empty() && platform::check_model(&d.settings.model).is_ok();
        if ui.add_enabled_ui(ready, |ui| widgets::dialog_button(ui, "Send reviewed text", widgets::DialogAction::Confirm)).inner.clicked() {
            match assistant::prompt(d.task, &d.question, &d.text) {
                Ok(prompt) => {
                    if let Some(key) = d.source {
                        let settings = d.settings.clone();
                        d.start(&ctx, move |cancel, _| {
                            Ok(Update::Answer(
                                platform::answer(settings.provider, &settings.account, &settings.model, assistant::INSTRUCTIONS, &prompt, &cancel)?,
                                key,
                            ))
                        });
                    }
                }
                Err(error) => d.status = error,
            }
        }
        if widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked() {
            d.cancel();
            close = true;
        }
        if busy && widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked() {
            d.cancel();
            d.status = "Canceled locally. A request already sent may still count toward provider usage.".into();
        }
    });
    close
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
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
        app.assistant.job = Some(Job { rx, cancel: Default::default() });
        poll(&mut app, &egui::Context::default());
        assert_eq!(app.assistant.text, "Reviewed words.");
        assert!(app.assistant.job.is_none());
        assert!(app.assistant.status.contains("Nothing has been sent"));
        app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(Message::Done(Ok(Update::Answer("Stale answer".into(), key)))).unwrap();
        drop(tx);
        app.assistant.job = Some(Job { rx, cancel: Default::default() });
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
}
