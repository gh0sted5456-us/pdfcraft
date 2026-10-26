//! New PDF gallery and portable template import/export. Previews run away from the UI thread.
use crate::{Dialog, PdfCraftApp, theme, widgets};
use egui::{Color32, Rect, Sense, Stroke, vec2};
use pdfcraft_engine::templates::{self, STARTERS, TemplateInfo};
use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

#[derive(Clone)]
enum Source {
    Starter(String, String),
    Blank(f64, f64),
    Working(TemplateInfo, Arc<Vec<u8>>),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
}
struct Preview {
    info: TemplateInfo,
    bytes: Arc<Vec<u8>>,
    image: egui::ColorImage,
    pages: usize,
    size: (f32, f32),
}
struct Ready {
    preview: Preview,
}
struct Loading {
    key: String,
    selected: bool,
    receiver: mpsc::Receiver<Result<Preview, String>>,
}
pub struct TemplateGallery {
    category: String,
    search: String,
    paper: String,
    default_size: (f64, f64),
    selected: String,
    ready: Option<Ready>,
    imported: Option<(TemplateInfo, Arc<Vec<u8>>)>,
    pending: Option<Source>,
    receiving: Option<Loading>,
    thumbnails: BTreeMap<String, Option<egui::TextureHandle>>,
    error: Option<String>,
}
pub struct TemplateExport {
    doc: pdfcraft_engine::DocId,
    info: TemplateInfo,
}
pub(crate) enum Action {
    None,
    Create,
    Import,
    Export,
    Cancel,
    Custom,
}

fn build(source: Source) -> Result<Preview, String> {
    let session = pdfcraft_engine::Session::new();
    let (info, bytes) = match source {
        Source::Starter(id, paper) => {
            let t = templates::find(&id).map_err(|e| e.to_string())?;
            (
                TemplateInfo { title: t.title.into(), description: t.description.into(), category: t.category.into() },
                session.create_starter(&id, &paper).map_err(|e| e.to_string())?,
            )
        }
        Source::Blank(w, h) => (
            TemplateInfo { title: "Blank page".into(), description: "Your default physical paper size.".into(), category: "Essentials".into() },
            session.create_blank(w, h, 1).map_err(|e| e.to_string())?,
        ),
        Source::Working(info, bytes) => (info, bytes),
        #[cfg(not(target_arch = "wasm32"))]
        Source::File(path) => {
            use std::io::Read;
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let mut bytes = Vec::new();
            file.take(templates::MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            session.import_template(Arc::new(bytes)).map_err(|e| e.to_string())?
        }
    };
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes.clone(), Default::default());
    let size = renderer.page_size(0).ok_or("The template has no readable page")?;
    if !size.0.is_finite() || !size.1.is_finite() || size.0 <= 0.0 || size.1 <= 0.0 {
        return Err("Invalid template page size".into());
    }
    let rendered = renderer.render(pdfcraft_render::RenderRequest {
        page: 0,
        scale: (440.0 / size.0.max(size.1)).min(1.0),
        preview: true,
        ..Default::default()
    });
    if let Some(error) = rendered.error {
        return Err(error);
    }
    let (w, h) = (rendered.width as usize, rendered.height as usize);
    if w == 0 || h == 0 || w > 1024 || h > 1024 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(rendered.rgba.len()) {
        return Err("Invalid template preview".into());
    }
    Ok(Preview { info, bytes, image: egui::ColorImage::from_rgba_premultiplied([w, h], &rendered.rgba), pages: renderer.page_count(), size })
}

impl TemplateGallery {
    fn new(paper: &crate::paper_ui::Paper) -> Self {
        Self {
            category: "All templates".into(),
            search: String::new(),
            paper: if paper.label().starts_with('A') { "a4".into() } else { "letter".into() },
            default_size: (paper.width, paper.height),
            selected: "blank".into(),
            ready: None,
            imported: None,
            pending: Some(Source::Blank(paper.width, paper.height)),
            receiving: None,
            thumbnails: BTreeMap::new(),
            error: None,
        }
    }
    fn choose(&mut self, id: &str) {
        self.selected = id.into();
        self.ready = None;
        self.error = None;
        self.pending = if id == "imported" {
            self.imported.as_ref().map(|(i, b)| Source::Working(i.clone(), b.clone()))
        } else if id == "blank" {
            Some(Source::Blank(self.default_size.0, self.default_size.1))
        } else {
            Some(Source::Starter(id.into(), self.paper.clone()))
        };
    }
    fn poll(&mut self, ctx: &egui::Context) {
        let result = self.receiving.as_ref().and_then(|job| match job.receiver.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => Some(Err("The template preview worker stopped".into())),
        });
        if let Some(result) = result {
            let Some(job) = self.receiving.take() else { return };
            match result {
                Ok(preview) => {
                    let texture = ctx.load_texture(format!("template-{}", job.key), preview.image.clone(), egui::TextureOptions::LINEAR);
                    self.thumbnails.insert(job.key, Some(texture.clone()));
                    // A newer selection supersedes a selected preview; thumbnails never change the document to create.
                    if job.selected && self.pending.is_none() {
                        if self.selected == "imported" {
                            self.imported = Some((preview.info.clone(), preview.bytes.clone()));
                        }
                        self.ready = Some(Ready { preview });
                    }
                }
                Err(error) => {
                    self.thumbnails.insert(job.key, None);
                    if job.selected && self.pending.is_none() {
                        self.error = Some(error);
                    }
                }
            }
        }
        if self.receiving.is_some() {
            return;
        }
        // One worker, selected template first; the cache has only built-in IDs in Letter/A4 and one imported template.
        let next = if let Some(source) = self.pending.take() {
            Some((format!("{}:{}", self.paper, self.selected), true, source))
        } else {
            STARTERS.iter().find_map(|item| {
                let key = format!("{}:{}", self.paper, item.id);
                if self.thumbnails.contains_key(&key) {
                    return None;
                }
                let source = if item.id == "blank" {
                    Source::Blank(self.default_size.0, self.default_size.1)
                } else {
                    Source::Starter(item.id.into(), self.paper.clone())
                };
                Some((key, false, source))
            })
        };
        if let Some((key, selected, source)) = next {
            let (tx, receiver) = mpsc::channel();
            self.receiving = Some(Loading { key, selected, receiver });
            let ctx = ctx.clone();
            let work = move || {
                let result = std::panic::catch_unwind(|| build(source)).unwrap_or_else(|_| Err("The template could not be previewed".into()));
                let _ = tx.send(result);
                ctx.request_repaint();
            };
            #[cfg(not(target_arch = "wasm32"))]
            if let Err(error) = std::thread::Builder::new().name("pdfcraft-template".into()).spawn(work)
                && let Some(job) = self.receiving.take()
            {
                self.thumbnails.insert(job.key, None);
                if job.selected {
                    self.error = Some(error.to_string());
                }
            }
            #[cfg(target_arch = "wasm32")]
            work();
        }
    }
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> Action {
    let draft = app.template_gallery.get_or_insert_with(|| TemplateGallery::new(&app.default_paper));
    draft.poll(ui.ctx());
    let t = theme::Tokens::get(ui.ctx());
    widgets::dialog_heading(ui, "New PDF");
    ui.add_space(6.0);
    let mut choose = None;
    let mut action = Action::None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let mut categories = vec!["All templates"];
        for item in STARTERS {
            if !categories.contains(&item.category) {
                categories.push(item.category);
            }
        }
        if draft.imported.is_some() {
            categories.push("Imported");
        }
        egui::ComboBox::from_id_salt("template-category").selected_text(&draft.category).width(160.0).show_ui(ui, |ui| {
            for category in categories {
                ui.selectable_value(&mut draft.category, category.into(), category);
            }
        });
        let search_width = (ui.available_width() - 80.0).max(50.0);
        ui.add(widgets::line(&mut draft.search).hint_text("Search templates").desired_width(search_width));
        if crate::icons::button(ui, "folder-open", 30.0, false, "Import template...").clicked() {
            action = Action::Import;
        }
        if ui.add_enabled_ui(draft.ready.is_some(), |ui| crate::icons::button(ui, "file-down", 30.0, false, "Export template...")).inner.clicked() {
            action = Action::Export;
        }
    });
    ui.add_space(6.0);
    let height = (ui.ctx().content_rect().height() - 264.0).clamp(100.0, 540.0).min((ui.available_height() - 120.0).max(16.0));
    let compact = ui.ctx().content_rect().height() < 650.0;
    let query = draft.search.to_lowercase();
    let mut shown = 0usize;
    ui.spacing_mut().scroll.floating = false;
    egui::ScrollArea::vertical()
        .id_salt("template-grid")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(height)
        .min_scrolled_height(height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let gap = 10.0;
            let columns = ((ui.available_width() + gap) / if compact { 150.0 } else { 180.0 }).floor().clamp(1.0, 6.0) as usize;
            let tile = vec2(((ui.available_width() - gap * (columns - 1) as f32) / columns as f32).max(1.0), if compact { 104.0 } else { 170.0 });
            egui::Grid::new("template-tiles").num_columns(columns).spacing([gap, gap]).show(ui, |ui| {
                for item in STARTERS {
                    if draft.category != "All templates" && draft.category != item.category {
                        continue;
                    }
                    if !format!("{} {} {}", item.title, item.category, item.description).to_lowercase().contains(&query) {
                        continue;
                    }
                    let key = format!("{}:{}", draft.paper, item.id);
                    let thumbnail = draft.thumbnails.get(&key).and_then(Option::as_ref);
                    if card(ui, item.title, item.category, draft.selected == item.id, thumbnail, tile, &t).clicked() {
                        choose = Some(item.id.to_string());
                    }
                    shown += 1;
                    if shown.is_multiple_of(columns) {
                        ui.end_row();
                    }
                }
                if let Some((info, _)) = &draft.imported
                    && (draft.category == "All templates" || draft.category == "Imported")
                    && info.title.to_lowercase().contains(&query)
                {
                    let key = format!("{}:imported", draft.paper);
                    let thumbnail = draft.thumbnails.get(&key).and_then(Option::as_ref);
                    if card(ui, &info.title, "Imported", draft.selected == "imported", thumbnail, tile, &t).clicked() {
                        choose = Some("imported".into());
                    }
                    shown += 1;
                }
            });
            if shown == 0 {
                ui.label("No matching templates.");
            }
        });
    ui.add_space(6.0);
    if let Some(ready) = &draft.ready {
        let p = &ready.preview;
        ui.add(
            egui::Label::new(format!(
                "{} · {} {} · {:.0} × {:.0} pt",
                p.info.title,
                p.pages,
                if p.pages == 1 { "page" } else { "pages" },
                p.size.0,
                p.size.1
            ))
            .truncate(),
        )
        .on_hover_text(&p.info.description);
    } else if let Some(error) = &draft.error {
        ui.add(egui::Label::new(error).truncate()).on_hover_text(error);
    } else {
        ui.label("Preparing selected template...");
    }
    ui.horizontal_wrapped(|ui| {
        ui.add_enabled_ui(draft.selected != "imported" && draft.selected != "blank", |ui| {
            ui.label("Paper");
            let old = draft.paper.clone();
            egui::ComboBox::from_id_salt("starter-paper")
                .selected_text(if draft.selected == "blank" {
                    "Settings default"
                } else if draft.selected == "imported" {
                    "Original size"
                } else if draft.paper == "a4" {
                    "A4"
                } else {
                    "US Letter"
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.paper, "letter".into(), "US Letter");
                    ui.selectable_value(&mut draft.paper, "a4".into(), "A4");
                });
            if old != draft.paper {
                choose = Some(draft.selected.clone());
            }
        });
        if widgets::dialog_button(ui, "Custom page...", widgets::DialogAction::Back).clicked() {
            action = Action::Custom;
        }
        ui.label(egui::RichText::new(format!("{shown} templates")).small().color(t.text_muted));
    });
    ui.add_space(8.0);
    widgets::dialog_footer(ui, |ui| {
        if ui.add_enabled_ui(draft.ready.is_some(), |ui| widgets::dialog_button(ui, tl!("Create"), widgets::DialogAction::Confirm)).inner.clicked() {
            action = Action::Create;
        }
        if widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked() {
            action = Action::Cancel;
        }
    });
    if let Some(id) = choose {
        draft.choose(&id);
    }
    action
}

fn card(
    ui: &mut egui::Ui,
    title: &str,
    category: &str,
    selected: bool,
    thumbnail: Option<&egui::TextureHandle>,
    size: egui::Vec2,
    t: &theme::Tokens,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, title));
    ui.painter().rect_filled(
        rect,
        6.0,
        if selected {
            t.accent_soft
        } else if response.hovered() {
            t.hover
        } else {
            t.field
        },
    );
    ui.painter().rect_stroke(
        rect,
        6.0,
        Stroke::new(if selected { 1.0 } else { 0.5 }, if selected { t.accent } else { t.border }),
        egui::StrokeKind::Inside,
    );
    if ui.is_rect_visible(rect) {
        let area = Rect::from_min_max(rect.min + vec2(8.0, 8.0), rect.max - vec2(8.0, 42.0));
        if let Some(texture) = thumbnail {
            let image = texture.size_vec2();
            let scale = (area.width() / image.x).min(area.height() / image.y);
            let paper = Rect::from_center_size(area.center(), image * scale);
            ui.painter().image(texture.id(), paper, Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), Color32::WHITE);
        } else {
            crate::icons::paint(ui, Rect::from_center_size(area.center(), vec2(24.0, 24.0)), "file-text", 24.0, t.text_faint);
        }
        let font = theme::semibold(12.5);
        let title = widgets::fit_line(title, (rect.width() - 16.0).max(1.0), |s| widgets::text_width(ui, s, &font));
        ui.painter().text(egui::pos2(rect.center().x, rect.bottom() - 27.0), egui::Align2::CENTER_CENTER, title, font, t.text);
        ui.painter().text(
            egui::pos2(rect.center().x, rect.bottom() - 11.0),
            egui::Align2::CENTER_CENTER,
            category,
            theme::regular(10.5),
            t.text_muted,
        );
    }
    theme::hand(response).on_hover_text(format!("{title} · {category}"))
}

impl PdfCraftApp {
    pub(crate) fn begin_template_gallery(&mut self) {
        self.template_gallery = Some(TemplateGallery::new(&self.default_paper));
        self.dialog = Some(Dialog::Templates);
    }
    pub(crate) fn template_action(&mut self, action: Action) {
        match action {
            Action::Custom => {
                self.execute("create.custom");
            }
            Action::Create => {
                let bytes = self.template_gallery.as_ref().and_then(|d| d.ready.as_ref()).map(|r| r.preview.bytes.clone());
                if let Some(bytes) = bytes {
                    match self.open_created_bytes("Untitled.pdf", Ok(bytes)) {
                        Ok(()) => {
                            self.dialog = None;
                            self.template_gallery = None;
                            self.execute("edit.edit_text");
                        }
                        Err(error) => self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error)]),
                    }
                }
            }
            Action::Import => {
                #[cfg(not(target_arch = "wasm32"))]
                self.ask_one(
                    crate::pickers::Ask::File(rfd::AsyncFileDialog::new().add_filter("PdfCraft template", &[templates::EXTENSION])),
                    None,
                    |app, path| {
                        if let Some(draft) = app.template_gallery.as_mut() {
                            draft.selected = "imported".into();
                            draft.category = "Imported".into();
                            draft.search.clear();
                            draft.ready = None;
                            draft.error = None;
                            draft.pending = Some(Source::File(path));
                        }
                    },
                );
                #[cfg(target_arch = "wasm32")]
                self.notify("Open or drop a .pdfcraft-template file to create a working copy.");
            }
            Action::Export => {
                let chosen = self.template_gallery.as_ref().and_then(|d| d.ready.as_ref()).map(|r| (r.preview.info.clone(), r.preview.bytes.clone()));
                if let Some((info, bytes)) = chosen {
                    let mut session = pdfcraft_engine::Session::new();
                    let exported = session
                        .open_new("Template.pdf", bytes)
                        .map_err(|e| e.to_string())
                        .and_then(|id| session.export_template(id, &info).map_err(|e| e.to_string()));
                    match exported {
                        Ok(bytes) => self.write_template(bytes, &info.title),
                        Err(error) => self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error.to_string())]),
                    }
                }
            }
            Action::Cancel => {
                self.dialog = None;
                self.template_gallery = None;
            }
            Action::None => {}
        }
    }
    pub(crate) fn begin_template_export(&mut self) {
        let Some((_, doc)) = self.active_ids() else { return };
        if !self.commit_typing_in(doc) {
            return;
        }
        let name = self.session.get(doc).map(|d| d.name.trim_end_matches(".pdf").to_string()).unwrap_or_else(|| "My template".into());
        self.template_export =
            Some(TemplateExport { doc, info: TemplateInfo { title: name, description: String::new(), category: "Custom".into() } });
        self.dialog = Some(Dialog::SaveTemplate);
    }
    pub(crate) fn finish_template_export(&mut self) {
        let Some(draft) = self.template_export.as_ref() else { return };
        let title = draft.info.title.clone();
        match self.session.export_template(draft.doc, &draft.info) {
            Ok(bytes) => {
                self.dialog = None;
                self.template_export = None;
                self.write_template(bytes, &title);
            }
            Err(error) => self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error.to_string())]),
        }
    }
    fn write_template(&mut self, bytes: Arc<Vec<u8>>, title: &str) {
        let stem: String = title.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(80).collect();
        let name = format!("{}-template.{}", if stem.trim().is_empty() { "My" } else { stem.trim() }, templates::EXTENSION);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let originals: Vec<_> = self.session.docs().iter().filter_map(|d| d.path.clone()).collect();
            let write = move |app: &mut Self, path: String| {
                if originals.iter().any(|p| crate::same_file(p, &path))
                    || app.session.docs().iter().filter_map(|d| d.path.as_deref()).any(|p| crate::same_file(p, &path))
                {
                    app.notify("Choose a different file for the reusable template; the original document is open.");
                    return;
                }
                match crate::editing::write_atomically(&path, &bytes) {
                    Ok(()) => app.notify("Template exported. Open it to create a new working copy."),
                    Err(error) => app.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error.to_string())]),
                }
            };
            match self.save_override.clone() {
                Some(path) => write(self, path),
                None => {
                    self.ask_one(
                        crate::pickers::Ask::Save(
                            rfd::AsyncFileDialog::new().add_filter("PdfCraft template", &[templates::EXTENSION]).set_file_name(&name),
                        ),
                        None,
                        move |app, path| write(app, path.to_string_lossy().into_owned()),
                    );
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Err(error) = crate::editing::download(&name, &bytes) {
            self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error)]);
        }
    }
}

pub(crate) fn export_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    crate::widgets::dialog_heading(ui, "Save as template");
    let Some(draft) = app.template_export.as_mut() else { return (false, true) };
    ui.label("Includes current contents and form values. The source document stays unchanged.");
    ui.add_space(12.0);
    for (label, value, limit) in
        [("Name", &mut draft.info.title, 120), ("Description", &mut draft.info.description, 1000), ("Category", &mut draft.info.category, 64)]
    {
        ui.label(label);
        if label == "Description" {
            ui.add(egui::TextEdit::multiline(value).char_limit(limit).desired_rows(3).desired_width(ui.available_width()));
        } else {
            ui.add(egui::TextEdit::singleline(value).char_limit(limit).desired_width(ui.available_width()));
        }
    }
    ui.add_space(12.0);
    let (mut save, mut cancel) = (false, false);
    widgets::dialog_footer(ui, |ui| {
        save = widgets::dialog_button(ui, "Save template", widgets::DialogAction::Confirm).clicked();
        cancel = widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked();
    });
    (save, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thumbnails_finish_and_superseded_selections_never_become_the_created_pdf() {
        let app = PdfCraftApp::new();
        let mut gallery = TemplateGallery::new(&app.default_paper);
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while gallery.thumbnails.len() < STARTERS.len() && std::time::Instant::now() < deadline {
            gallery.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(gallery.thumbnails.values().filter(|v| v.is_some()).count(), STARTERS.len());
        let first = STARTERS.iter().find(|s| s.id != "blank").unwrap();
        let last = STARTERS.iter().find(|s| s.id != "blank" && s.id != first.id).unwrap();
        gallery.choose(first.id);
        gallery.poll(&ctx);
        gallery.choose(last.id);
        while gallery.ready.is_none() && std::time::Instant::now() < deadline {
            gallery.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(gallery.ready.as_ref().unwrap().preview.info.title, last.title);
        gallery.paper = "a4".into();
        gallery.choose(first.id);
        while gallery.ready.is_none() && std::time::Instant::now() < deadline {
            gallery.poll(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(gallery.ready.as_ref().unwrap().preview.info.title, first.title);
        assert!(gallery.thumbnails.contains_key(&format!("a4:{}", first.id)));
        assert!(gallery.thumbnails.len() <= STARTERS.len() * 2 + 2);
    }
}
