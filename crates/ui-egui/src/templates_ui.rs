//! Lion PDF ▸ File ▸ Create a PDF: the template gallery (built-in starters, a blank page and an
//! imported `.pdfcraft-template`), and File ▸ Save as template. Previews are drawn on a worker, one
//! at a time with the selected template first, so the gallery never holds the frame.

use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};

use craft::host::{PickRequest, Picked, WriteIntent};
use craft::ui::contract::Bind;
use craft::ui::dialog::{DialogResult, dialog_id};
use craft::ui::patterns::form_dialogs::{DialogFields, FormDialog, HostedParamDialog, Param, ParamDialog, ParamKind};
use craft::ui::sections::{DialogRole, FooterButton};
use egui::{Rect, Sense, Stroke, vec2};
use pdfcraft_engine::templates::{self, STARTERS, TemplateInfo};

use crate::PdfCraftApp;

/// The gallery's "every category" entry.
const ALL: &str = "All templates";
/// The category an imported template is listed under.
const IMPORTED: &str = "Imported";

#[derive(Clone)]
enum Source {
    Starter(String, String),
    Blank(f64, f64),
    Working(TemplateInfo, Arc<Vec<u8>>),
    #[cfg(not(target_arch = "wasm32"))]
    File(std::path::PathBuf),
    Bytes(Arc<Vec<u8>>),
}

struct Preview {
    info: TemplateInfo,
    bytes: Arc<Vec<u8>>,
    image: egui::ColorImage,
    pages: usize,
    size: (f32, f32),
}

struct Loading {
    key: String,
    selected: bool,
    receiver: mpsc::Receiver<Result<Preview, String>>,
}

/// What the gallery's controls choose: the dialog's value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Choice {
    category: String,
    search: String,
    /// `letter` or `a4`, for the starters.
    paper: String,
    selected: String,
}

impl Default for Choice {
    fn default() -> Self {
        Self { category: ALL.into(), search: String::new(), paper: "letter".into(), selected: "blank".into() }
    }
}

/// The open gallery: its choices and the previews drawn so far.
pub struct TemplateGallery {
    choice: Choice,
    default_size: (f64, f64),
    ready: Option<Preview>,
    imported: Option<(TemplateInfo, Arc<Vec<u8>>)>,
    pending: Option<Source>,
    receiving: Option<Loading>,
    thumbnails: BTreeMap<String, Option<egui::TextureHandle>>,
    error: Option<String>,
}

/// Draws a template's first page for the gallery.
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
            TemplateInfo { title: "Blank page".into(), description: "The paper size of a new blank document.".into(), category: "Essentials".into() },
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
        Source::Bytes(bytes) => session.import_template(bytes).map_err(|e| e.to_string())?,
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
    let rgba: &[u8] = rendered.rgba.as_ref();
    if w == 0 || h == 0 || w > 1024 || h > 1024 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(rgba.len()) {
        return Err("Invalid template preview".into());
    }
    Ok(Preview { info, bytes, image: egui::ColorImage::from_rgba_premultiplied([w, h], rgba), pages: renderer.page_count(), size })
}

impl TemplateGallery {
    fn new(blank: (f64, f64), a4: bool) -> Self {
        let choice = Choice { paper: if a4 { "a4".into() } else { "letter".into() }, ..Choice::default() };
        Self {
            choice,
            default_size: blank,
            ready: None,
            imported: None,
            pending: Some(Source::Blank(blank.0, blank.1)),
            receiving: None,
            thumbnails: BTreeMap::new(),
            error: None,
        }
    }

    /// The source a selection previews.
    fn source_of(&self, id: &str) -> Option<Source> {
        match id {
            "imported" => self.imported.as_ref().map(|(i, b)| Source::Working(i.clone(), b.clone())),
            "blank" => Some(Source::Blank(self.default_size.0, self.default_size.1)),
            id => Some(Source::Starter(id.into(), self.choice.paper.clone())),
        }
    }

    /// Takes the dialog's choices: a new selection or paper draws the selected preview again.
    fn take(&mut self, choice: Choice) {
        let reselect = choice.selected != self.choice.selected || choice.paper != self.choice.paper;
        self.choice = choice;
        if reselect {
            self.ready = None;
            self.error = None;
            self.pending = self.source_of(&self.choice.selected.clone());
        }
    }

    /// Picks up a finished preview and starts the next one.
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
                    self.thumbnails.insert(job.key, Some(texture));
                    // A newer selection supersedes a selected preview; thumbnails never change the document to create.
                    if job.selected && self.pending.is_none() {
                        if self.choice.selected == "imported" {
                            self.imported = Some((preview.info.clone(), preview.bytes.clone()));
                        }
                        self.ready = Some(preview);
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
        // One worker, the selected template first; the cache holds the built-in IDs in Letter and A4 and one imported template.
        let next = if let Some(source) = self.pending.take() {
            Some((format!("{}:{}", self.choice.paper, self.choice.selected), true, source))
        } else {
            STARTERS.iter().find_map(|item| {
                let key = format!("{}:{}", self.choice.paper, item.id);
                if self.thumbnails.contains_key(&key) {
                    return None;
                }
                Some((key, false, self.source_of(item.id)?))
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
            if cfg!(test) {
                work();
            } else if let Err(error) = std::thread::Builder::new().name("lion-template".into()).spawn(work)
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

/// The gallery's footer buttons after Create and Cancel.
const IMPORT: usize = 2;
const EXPORT: usize = 3;
const CUSTOM: usize = 4;

/// One pass of the open gallery.
pub(crate) fn show(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let Some(gallery) = app.template_gallery.as_mut() else { return };
    gallery.poll(ui.ctx());
    let mut choice = gallery.choice.clone();
    let mut open = true;
    let ready = gallery.ready.is_some();
    let out = FormDialog::new(tl!("Create a PDF"), Bind::bound(&mut choice))
        .opened(Bind::bound(&mut open))
        .ok_label(tl!("Create"))
        .ok_enabled(ready)
        .extra_buttons([
            FooterButton::new(tl!("Import template…"), DialogRole::Other),
            FooterButton::new(tl!("Export template…"), DialogRole::Other).enabled(ready),
            FooterButton::new(tl!("Custom page…"), DialogRole::Other),
        ])
        .show(ui, |ui, _, c| {
            body(ui, gallery, c);
            true
        });
    gallery.take(choice);
    if out.closed.is_some() && out.accepted.is_none() {
        app.template_gallery = None;
        return;
    }
    if out.accepted.is_some() {
        app.create_from_template(ui.ctx());
        return;
    }
    match out.picked {
        Some(IMPORT) => app.import_template(),
        Some(EXPORT) => app.export_gallery_template(),
        Some(CUSTOM) => {
            app.template_gallery = None;
            crate::lion_create::open_custom_page(app, ui.ctx());
        }
        _ => {}
    }
}

/// The gallery's body: category and search, the tiles, the paper and the selection's line.
fn body(ui: &mut egui::Ui, gallery: &TemplateGallery, c: &mut Choice) {
    let visuals = ui.visuals().clone();
    ui.horizontal(|ui| {
        let mut categories = vec![ALL];
        for item in STARTERS {
            if !categories.contains(&item.category) {
                categories.push(item.category);
            }
        }
        if gallery.imported.is_some() {
            categories.push(IMPORTED);
        }
        let label = ui.label(tl!("Category"));
        egui::ComboBox::from_id_salt("template-category")
            .selected_text(tl!(&c.category))
            .width(160.0)
            .show_ui(ui, |ui| {
                for category in categories {
                    ui.selectable_value(&mut c.category, category.into(), tl!(category));
                }
            })
            .response
            .labelled_by(label.id);
        ui.add(egui::TextEdit::singleline(&mut c.search).hint_text(tl!("Search templates")).desired_width(ui.available_width().max(60.0)));
    });
    ui.add_space(6.0);
    let height = (ui.ctx().content_rect().height() - 300.0).clamp(120.0, 520.0);
    let compact = ui.ctx().content_rect().height() < 650.0;
    let query = c.search.to_lowercase();
    let mut shown = 0usize;
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("template-grid").max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
        let gap = 10.0;
        let columns = ((ui.available_width() + gap) / if compact { 150.0 } else { 180.0 }).floor().clamp(1.0, 6.0) as usize;
        let tile = vec2(((ui.available_width() - gap * (columns - 1) as f32) / columns as f32).max(1.0), if compact { 104.0 } else { 170.0 });
        egui::Grid::new("template-tiles").num_columns(columns).spacing([gap, gap]).show(ui, |ui| {
            let mut tile_for = |ui: &mut egui::Ui, id: &str, title: &str, category: &str| {
                let key = format!("{}:{}", c.paper, id);
                let thumbnail = gallery.thumbnails.get(&key).and_then(Option::as_ref);
                if card(ui, &visuals, title, category, c.selected == id, thumbnail, tile).clicked() {
                    chosen = Some(id.to_string());
                }
                shown += 1;
                if shown.is_multiple_of(columns) {
                    ui.end_row();
                }
            };
            for item in STARTERS {
                if c.category != ALL && c.category != item.category {
                    continue;
                }
                if !format!("{} {} {}", item.title, item.category, item.description).to_lowercase().contains(&query) {
                    continue;
                }
                tile_for(ui, item.id, tl!(item.title), tl!(item.category));
            }
            if let Some((info, _)) = &gallery.imported
                && (c.category == ALL || c.category == IMPORTED)
                && info.title.to_lowercase().contains(&query)
            {
                tile_for(ui, "imported", &info.title, tl!(IMPORTED));
            }
        });
        if shown == 0 {
            ui.label(tl!("No matching templates."));
        }
    });
    if let Some(id) = chosen {
        c.selected = id;
    }
    ui.add_space(6.0);
    match (&gallery.ready, &gallery.error) {
        (Some(p), _) => {
            let pages = crate::i18n::trn(crate::i18n::current(), p.pages as u64, "{n} page", "{n} pages");
            ui.add(egui::Label::new(format!("{} · {} · {:.0} × {:.0} pt", p.info.title, pages, p.size.0, p.size.1)).truncate())
                .on_hover_text(&p.info.description);
        }
        (None, Some(error)) => {
            ui.add(egui::Label::new(error).truncate()).on_hover_text(error);
        }
        (None, None) => {
            ui.label(tl!("Preparing the selected template…"));
        }
    }
    ui.horizontal(|ui| {
        let starter = c.selected != "imported" && c.selected != "blank";
        ui.add_enabled_ui(starter, |ui| {
            let label = ui.label(tl!("Paper"));
            egui::ComboBox::from_id_salt("starter-paper")
                .selected_text(if !starter {
                    tl!("Original size")
                } else if c.paper == "a4" {
                    "A4"
                } else {
                    tl!("US Letter")
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.paper, "letter".into(), tl!("US Letter"));
                    ui.selectable_value(&mut c.paper, "a4".into(), "A4");
                })
                .response
                .labelled_by(label.id);
        });
        ui.label(egui::RichText::new(crate::i18n::trn(crate::i18n::current(), shown as u64, "{n} template", "{n} templates")).small().weak());
    });
}

/// One tile: the first page's preview, the title and the category.
fn card(
    ui: &mut egui::Ui,
    visuals: &egui::Visuals,
    title: &str,
    category: &str,
    selected: bool,
    thumbnail: Option<&egui::TextureHandle>,
    size: egui::Vec2,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, title));
    let fill = if selected {
        visuals.selection.bg_fill
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.extreme_bg_color
    };
    let radius = visuals.widgets.inactive.corner_radius;
    ui.painter().rect_filled(rect, radius, fill);
    let stroke = if selected { visuals.selection.stroke } else { Stroke::new(0.5, visuals.widgets.noninteractive.bg_stroke.color) };
    ui.painter().rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);
    if ui.is_rect_visible(rect) {
        let area = Rect::from_min_max(rect.min + vec2(8.0, 8.0), rect.max - vec2(8.0, 42.0));
        if let Some(texture) = thumbnail {
            let image = texture.size_vec2();
            let scale = (area.width() / image.x).min(area.height() / image.y);
            let paper = Rect::from_center_size(area.center(), image * scale);
            ui.painter().image(texture.id(), paper, Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
        } else {
            ui.painter().rect_stroke(
                Rect::from_center_size(area.center(), vec2(area.height() * 0.7, area.height())),
                2.0,
                Stroke::new(1.0, visuals.weak_text_color()),
                egui::StrokeKind::Inside,
            );
        }
        let text = visuals.text_color();
        let body = egui::TextStyle::Body.resolve(ui.style());
        let small = egui::TextStyle::Small.resolve(ui.style());
        let painter = ui.painter_at(rect.shrink(6.0));
        painter.text(egui::pos2(rect.center().x, rect.bottom() - 27.0), egui::Align2::CENTER_CENTER, title, body, text);
        painter.text(egui::pos2(rect.center().x, rect.bottom() - 11.0), egui::Align2::CENTER_CENTER, category, small, visuals.weak_text_color());
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("{title} · {category}"))
}

/// The file name a template titled `title` is saved under.
fn template_file_name(title: &str) -> String {
    let stem: String = title.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(80).collect();
    format!("{}-template.{}", if stem.trim().is_empty() { "My" } else { stem.trim() }, templates::EXTENSION)
}

impl PdfCraftApp {
    /// File ▸ Create a PDF: open the gallery, the blank page selected.
    pub(crate) fn begin_template_gallery(&mut self) {
        let a4 = self.blank_paper == crate::paper::Paper::A4;
        self.template_gallery = Some(TemplateGallery::new(self.blank_paper.points(), a4));
    }

    /// Create: a new document from the selected template, ready for Edit text.
    fn create_from_template(&mut self, ctx: &egui::Context) {
        let Some(bytes) = self.template_gallery.as_ref().and_then(|g| g.ready.as_ref()).map(|p| p.bytes.clone()) else { return };
        match self.open_new_document("Untitled.pdf", bytes) {
            Ok(()) => {
                self.template_gallery = None;
                self.execute(ctx, "edit.edit_text");
            }
            Err(error) => self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &error)]),
        }
    }

    /// Import template…: a `.pdfcraft-template` file joins the gallery, selected.
    fn import_template(&mut self) {
        let request = PickRequest::open("template.import").title(tl!("Import template")).filter(tl!("Lion PDF template"), &[templates::EXTENSION]);
        self.pick_file(request, None, |app, _, picked| {
            let source = match picked {
                #[cfg(not(target_arch = "wasm32"))]
                Picked::Paths(paths) => paths.into_iter().next().map(Source::File),
                Picked::Contents(files) => files.into_iter().next().map(|(_, bytes)| Source::Bytes(Arc::new(bytes))),
                _ => None,
            };
            if let (Some(gallery), Some(source)) = (app.template_gallery.as_mut(), source) {
                gallery.choice.selected = "imported".into();
                gallery.choice.category = IMPORTED.into();
                gallery.choice.search.clear();
                gallery.ready = None;
                gallery.error = None;
                gallery.pending = Some(source);
            }
        });
    }

    /// Export template…: the selected template as a `.pdfcraft-template` file.
    fn export_gallery_template(&mut self) {
        let Some((info, bytes)) = self.template_gallery.as_ref().and_then(|g| g.ready.as_ref()).map(|p| (p.info.clone(), p.bytes.clone())) else {
            return;
        };
        let mut session = pdfcraft_engine::Session::new();
        let exported =
            session.open_new("Template.pdf", bytes).map_err(|e| e.to_string()).and_then(|id| session.export_template(id, &info).map_err(|e| e.to_string()));
        match exported {
            Ok(bytes) => self.write_template(bytes, &info.title),
            Err(error) => self.notify_fmt("Couldn't save the template: {e}", &[("e", &error)]),
        }
    }

    /// File ▸ Save as template: ask the template's name, description and category.
    pub(crate) fn begin_template_export(&mut self, ctx: &egui::Context) {
        let Some((_, doc)) = self.active_ids() else { return };
        if !self.commit_form_typing(ctx) {
            return;
        }
        let name = self.session.get(doc).map(|d| d.name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string()).unwrap_or_default();
        let mut fields = DialogFields::new();
        fields.set("title", if name.is_empty() { tl!("My template").to_string() } else { name });
        fields.set("description", String::new());
        fields.set("category", tl!("Custom").to_string());
        let dialog = HostedParamDialog::new(ctx, "create.save_template", fields, |fields| {
            let usable = fields.text("title").is_some_and(|t| !t.trim().is_empty());
            let rows = vec![
                Param::new("title", tl!("Name"), ParamKind::text("")),
                Param::new("description", tl!("Description"), ParamKind::text("")).multiline(3),
                Param::new("category", tl!("Category"), ParamKind::text("")),
            ];
            ParamDialog::new("create.save_template", tl!("Save as template"), rows)
                .values(Bind::bound(fields))
                .ok_label(tl!("Save template").into())
                .ok_enabled(usable)
                .footer_note(tl!("Includes the current contents and form values. The document itself stays unchanged."))
        });
        crate::dialogs::host::open(self, dialog);
    }

    /// Save as template's OK: write the template where the user chooses.
    pub(crate) fn template_export_pass(&mut self) {
        let Some(DialogResult::Ok(result)) = self.dialogs.take_by_id::<HostedParamDialog>(dialog_id("create.save_template")) else { return };
        let Some((_, doc)) = self.active_ids() else { return };
        let text = |key: &str, limit: usize| result.fields.text(key).unwrap_or_default().trim().chars().take(limit).collect::<String>();
        let info = TemplateInfo { title: text("title", 120), description: text("description", 1000), category: text("category", 64) };
        match self.session.export_template(doc, &info) {
            Ok(bytes) => self.write_template(bytes, &info.title),
            Err(error) => self.notify_fmt("Couldn't save the template: {e}", &[("e", &error.to_string())]),
        }
    }

    /// Writes a template where the user chooses, never over an open document.
    fn write_template(&mut self, bytes: Arc<Vec<u8>>, title: &str) {
        let request = PickRequest::save("template.export")
            .title(tl!("Save template"))
            .filter(tl!("Lion PDF template"), &[templates::EXTENSION])
            .file_name(template_file_name(title));
        self.pick_file(request, None, move |app, _, picked| {
            let Some(path) = picked.first_path() else { return };
            let shown = path.display().to_string();
            if app.session.docs().iter().filter_map(|d| d.path.as_deref()).any(|p| crate::same_file(p, &shown)) {
                app.notify_tr("Choose a different file for the reusable template; the original document is open.");
                return;
            }
            match app.host.files().write(&path, &bytes, WriteIntent::SaveAs) {
                Ok(_) => app.notify_tr("Template saved. Open it from Create a PDF to make a new working copy."),
                Err(e) => app.notify_fmt("Couldn't write {name}: {e}", &[("name", &shown), ("e", &e.to_string())]),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_names_keep_only_safe_characters() {
        assert_eq!(template_file_name("Invoice / 2026"), format!("Invoice  2026-template.{}", templates::EXTENSION));
        assert_eq!(template_file_name("../.."), format!("My-template.{}", templates::EXTENSION));
    }

    #[test]
    fn the_blank_page_previews_first_and_a_new_selection_supersedes_it() {
        let ctx = egui::Context::default();
        let mut gallery = TemplateGallery::new((612.0, 792.0), false);
        gallery.poll(&ctx);
        gallery.poll(&ctx);
        assert!(gallery.ready.as_ref().is_some_and(|p| p.pages == 1), "{:?}", gallery.error);
        gallery.take(Choice { selected: STARTERS[1].id.into(), ..gallery.choice.clone() });
        assert!(gallery.ready.is_none(), "a new selection waits for its own preview");
        gallery.poll(&ctx);
        gallery.poll(&ctx);
        assert!(gallery.ready.is_some(), "{:?}", gallery.error);
    }
}
