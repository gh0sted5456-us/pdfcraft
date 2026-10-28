//! Application-owned dialog windows and their taskbar. Drafts never leave this process.
use crate::{Dialog, PdfCraftApp, icons, theme, widgets};
use egui::{Align, Align2, Id, Layout, Rect, Vec2, vec2};
use pdfcraft_engine::DocId;
use std::sync::atomic::{AtomicU64, Ordering};

const MAX_PARKED: usize = 16;
static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Default)]
struct Registry {
    entries: Vec<Entry>,
}
#[derive(Clone)]
struct Entry {
    key: String,
    title: String,
    minimized: bool,
    maximized: bool,
    parked: bool,
    close: bool,
    seen: u64,
    offset: Vec2,
    size: Vec2,
    requested_size: Option<Vec2>,
}
#[derive(Clone)]
struct Slot {
    key: String,
    dialog: Dialog,
}
struct Parked {
    slot: Slot,
    title: String,
    doc: Option<DocId>,
    generation: Option<u64>,
    drafts: Box<PdfCraftApp>,
    maximized: bool,
}
#[derive(Default)]
pub(crate) struct Shelf {
    active: Option<Slot>,
    parked: Vec<Parked>,
}
impl Shelf {
    pub(crate) fn binds(&self, doc: DocId) -> bool {
        self.parked.iter().any(|p| p.doc == Some(doc))
    }
    pub(crate) fn holds_document_drafts(&self) -> bool {
        self.parked.iter().any(|p| p.doc.is_some())
    }
}
fn registry_id(ctx: &egui::Context) -> Id {
    Id::unique(("dialog-windows", ctx.viewport_id()))
}
fn registry<R>(ctx: &egui::Context, change: impl FnOnce(&mut Registry) -> R) -> R {
    let id = registry_id(ctx);
    let mut r = ctx.data(|d| d.get_temp::<Registry>(id)).unwrap_or_default();
    let out = change(&mut r);
    ctx.data_mut(|d| d.insert_temp(id, r));
    out
}
fn effective(ctx: &egui::Context, id: &str) -> String {
    if matches!(id, "dialog" | "field_table_window") {
        let alias = registry_id(ctx).with("main");
        ctx.data(|d| d.get_temp::<String>(alias)).unwrap_or_else(|| id.into())
    } else {
        id.into()
    }
}
fn touch(ctx: &egui::Context, key: &str, title: &str, full: bool) {
    let frame = ctx.cumulative_frame_nr();
    registry(ctx, |r| {
        if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
            e.seen = frame;
        } else {
            r.entries.push(Entry {
                key: key.into(),
                title: title.chars().take(160).collect(),
                minimized: false,
                maximized: full,
                parked: false,
                close: false,
                seen: frame,
                offset: Vec2::ZERO,
                size: vec2(400.0, 300.0),
                requested_size: None,
            });
        }
    });
}
fn remove(ctx: &egui::Context, key: &str) {
    registry(ctx, |r| r.entries.retain(|e| e.key != key));
}
pub(crate) fn minimized(ctx: &egui::Context, id: &str) -> bool {
    let key = effective(ctx, id);
    registry(ctx, |r| r.entries.iter().any(|e| e.key == key && e.minimized))
}
pub(crate) fn maximized(ctx: &egui::Context, id: &str) -> bool {
    let key = effective(ctx, id);
    registry(ctx, |r| r.entries.iter().any(|e| e.key == key && e.maximized))
}
fn set_minimized(ctx: &egui::Context, key: &str, value: bool) {
    registry(ctx, |r| {
        if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
            e.minimized = value;
        }
    });
    if value {
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
    }
    ctx.request_repaint();
}
pub(crate) fn title(dialog: Dialog) -> String {
    use Dialog::*;
    match dialog {
        Assistant => "AI Assistant",
        Preferences => "Preferences",
        Properties(_) => "Document Properties",
        Templates => "New PDF",
        SaveTemplate => "Export template",
        NewPaper => "New PDF",
        ExportTo => "Export to",
        About => "About Lion PDF",
        Shortcuts => "Keyboard shortcuts",
        Sign => "Digital signature",
        RemoveSignature => "Remove signature",
        Timestamp => "Timestamp",
        CertificateViewer => "Certificate",
        EncryptCertificate => "Encrypt with Certificate",
        Protect => "Protect PDF",
        Print => "Print",
        FieldTable => "Text Field Table",
        WordCount => "Word count",
        CreateImages => "Create PDF from images",
        Split => "Split PDF",
        NumberPages => "Page labels",
        PageBoxes => "Page boxes",
        PageProps => "Page Properties",
        PrinterMarks => "Add printer marks",
        FixHairlines => "Fix hairlines",
        Marks(_) => "Page marks",
        Export(_) => "Export PDF",
        Signature => "Create signature",
        CommentProps => "Comment Properties",
        ReplacePages => "Replace pages",
        FieldProps => "Field Properties",
        BulkFieldProps => "Field Properties",
        RedactPages => "Redact pages",
        RedactSearch => "Find and redact",
        RedactProps => "Redaction Properties",
        RedactApply => "Apply redactions",
        Revert => "Revert document",
        OverwriteChanged => "File changed on disk",
        LinkProps => "Link Properties",
        Extract => "Extract pages",
        RotatePages => "Rotate pages",
        RemoveHidden => "Remove hidden information",
        Sanitize => "Sanitize document",
        Recovery => "Document recovery",
        SummarizeComments => "Summarize comments",
        Optimize => "Optimize PDF",
        DuplicateField => "Duplicate field",
        AccessibilityOptions => "Accessibility options",
        RecognizeText => "Recognize text",
        CorrectOcrText => "Correct recognized text",
        RecognizeScan { .. } => "Recognize scanned text",
        JsConsole => "JavaScript console",
        DocumentJs => "Document JavaScripts",
        DocumentActions => "Document actions",
        PrintProduction => "Print Production",
        CompareFiles => "Compare files",
        ActionWizard => "Action Wizard",
        PdfA => "PDF/A",
        CreateStamp => "Create stamp",
        AltText => "Alternate text",
        AuditSpace => "Space audit",
    }
    .into()
}
fn default_title(id: &str) -> &str {
    match id {
        "save_prompt" => "Unsaved changes",
        "certificate-password" => "Certificate password",
        "link_prompt" => "Open link",
        "remote_link_prompt" => "Open linked document",
        "full_screen_prompt" => "Full screen",
        "timestamp-progress" => "Timestamp",
        "updates" => "Updates",
        "combine-preview" => "Preview",
        "combine-skip" => "Combine files",
        _ => crate::APP_NAME,
    }
}

pub(crate) struct Response {
    close: bool,
}
impl Response {
    pub(crate) fn should_close(&self) -> bool {
        self.close
    }
}
/// Content titles become window titles rather than a second heading inside the form.
pub(crate) fn heading(ui: &mut egui::Ui, title: &str) -> Option<egui::Response> {
    let drawing = registry_id(ui.ctx()).with("drawing");
    let key = ui.ctx().data(|d| d.get_temp::<String>(drawing))?;
    registry(ui.ctx(), |r| {
        if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
            e.title = title.chars().take(160).collect();
        }
    });
    Some(ui.allocate_response(Vec2::ZERO, egui::Sense::hover()))
}
pub(crate) fn chrome(ui: &mut egui::Ui, rect: Rect, key: &str) {
    let ctx = ui.ctx().clone();
    let Some(entry) = registry(&ctx, |r| r.entries.iter().find(|e| e.key == key).cloned()) else { return };
    let t = theme::Tokens::get(&ctx);
    let drag = ui.interact(
        Rect::from_min_max(rect.min, egui::pos2((rect.right() - 92.0).max(rect.left()), rect.bottom())),
        Id::unique((key, "move")),
        egui::Sense::click_and_drag(),
    );
    if drag.double_clicked() {
        registry(&ctx, |r| {
            if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                e.maximized = !e.maximized;
            }
        });
        ctx.request_repaint();
    }
    if drag.dragged() && !entry.maximized {
        let delta = drag.drag_delta();
        if delta.is_finite() {
            registry(&ctx, |r| {
                if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                    let bounds = ((ctx.content_rect().size() - e.size - vec2(0.0, 44.0)) * 0.5).max(Vec2::ZERO);
                    e.offset = (e.offset + delta).clamp(-bounds, bounds);
                }
            });
            ctx.request_repaint();
        }
    }
    let controls = Rect::from_min_max(egui::pos2((rect.right() - 90.0).max(rect.left()), rect.top()), rect.right_bottom());
    let mut buttons = ui.new_child(egui::UiBuilder::new().id_salt((key, "controls")).max_rect(controls).layout(Layout::left_to_right(Align::Center)));
    buttons.spacing_mut().item_spacing.x = 2.0;
    if icons::button(&mut buttons, "minus", 26.0, false, &format!("Minimize {}", entry.title)).clicked() {
        set_minimized(&ctx, key, true);
    }
    if icons::button(
        &mut buttons,
        if entry.maximized { "minimize-2" } else { "maximize-2" },
        26.0,
        false,
        &format!("{} {}", if entry.maximized { "Restore" } else { "Maximize" }, entry.title),
    )
    .clicked()
    {
        registry(&ctx, |r| {
            if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                e.maximized = !e.maximized;
            }
        });
        ctx.request_repaint();
    }
    if icons::button(&mut buttons, "x", 26.0, false, &format!("Close {}", entry.title)).clicked() {
        registry(&ctx, |r| {
            if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                e.close = true;
            }
        });
        ctx.request_repaint();
    }
    let title_rect = Rect::from_min_max(rect.min + vec2(90.0, 0.0), egui::pos2((rect.right() - 90.0).max(rect.left() + 91.0), rect.bottom()));
    let font = theme::semibold(16.0);
    let shown = widgets::fit_line(&entry.title, title_rect.width(), |text| widgets::text_width(ui, text, &font));
    ui.painter().with_clip_rect(title_rect).text(title_rect.center(), Align2::CENTER_CENTER, shown, font, t.text);
    let title_response = ui.interact(title_rect, Id::unique((key, "title")), egui::Sense::hover()).on_hover_text(&entry.title);
    title_response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, format!("Title: {}", entry.title)));
}
pub(crate) fn show<R>(ctx: &egui::Context, id: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> Response {
    render(ctx, id, true, content)
}
pub(crate) fn window<R>(ctx: &egui::Context, id: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> Response {
    render(ctx, id, false, content)
}
fn render<R>(ctx: &egui::Context, id: &str, blocking: bool, content: impl FnOnce(&mut egui::Ui) -> R) -> Response {
    let key = effective(ctx, id);
    touch(ctx, &key, default_title(id), false);
    let state = registry(ctx, |r| r.entries.iter().find(|e| e.key == key).cloned());
    if state.as_ref().is_some_and(|e| e.close) {
        remove(ctx, &key);
        return Response { close: true };
    }
    if state.as_ref().is_some_and(|e| e.minimized) {
        return Response { close: false };
    }
    let full = state.as_ref().is_some_and(|e| e.maximized);
    let shelf = registry(ctx, |r| r.entries.iter().any(|e| e.minimized));
    let usable = ctx.content_rect().shrink2(if full { vec2(12.0, 12.0) } else { vec2(36.0, 32.0) });
    let limit = (usable.size() - vec2(32.0, 60.0)).max(vec2(1.0, 1.0));
    let requested = state.as_ref().and_then(|e| e.requested_size).filter(|size| size.is_finite());
    let available = if full { limit } else { requested.map_or(limit, |size| (size - vec2(32.0, 24.0)).clamp(vec2(1.0, 1.0), limit)) };
    let mut modal = widgets::modal(ctx, Id::unique(&key));
    if shelf {
        modal = modal.area(egui::Modal::default_area(Id::unique(&key)).anchor(Align2::CENTER_CENTER, vec2(0.0, -18.0)));
    }
    let offset = if full {
        Vec2::ZERO
    } else {
        let size = requested.unwrap_or_else(|| state.as_ref().map_or(Vec2::ZERO, |e| e.size)).min(limit + vec2(32.0, 24.0));
        let bounds = ((ctx.content_rect().size() - size - vec2(0.0, 44.0)) * 0.5).max(Vec2::ZERO);
        state.as_ref().map_or(Vec2::ZERO, |e| e.offset).clamp(-bounds, bounds)
    };
    let offset = offset + vec2(0.0, if shelf { -18.0 } else { 0.0 });
    modal = modal.area(egui::Modal::default_area(Id::unique(&key)).anchor(Align2::CENTER_CENTER, offset));
    let draw = |ui: &mut egui::Ui| {
        widgets::begin_dialog(ui);
        ui.set_max_size(available);
        if full || requested.is_some() {
            ui.set_min_size(available);
        }
        let origin = ui.cursor().min;
        ui.allocate_space(vec2(0.0, 34.0));
        let drawing = registry_id(ctx).with("drawing");
        let previous = ctx.data_mut(|d| {
            let id = drawing;
            let old = d.get_temp::<String>(id);
            d.insert_temp(id, key.clone());
            old
        });
        egui::ScrollArea::both()
            .id_salt((&key, "content"))
            .max_width(available.x)
            .max_height((available.y - 34.0).max(1.0))
            .auto_shrink([requested.is_none() && !full; 2])
            .show(ui, |ui| {
                ui.set_max_height((available.y - 34.0).max(1.0));
                content(ui)
            });
        ctx.data_mut(|d| {
            let id = drawing;
            if let Some(old) = previous {
                d.insert_temp(id, old);
            } else {
                d.remove::<String>(id);
            }
        });
        let width = ui.min_rect().width().max(300.0).min(available.x);
        ui.set_min_width(width);
        chrome(ui, Rect::from_min_size(origin, vec2(width, 28.0)), &key);
        let card = ui.min_rect().expand2(vec2(16.0, 12.0));
        ctx.data_mut(|d| d.insert_temp(Id::unique(id).with("card"), card));
        registry(ctx, |r| {
            if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                e.size = card.size();
            }
        });
        if !full {
            resize_edges(ui, &key, card, limit + vec2(32.0, 24.0));
        }
    };
    let (response, dismissed) = if blocking {
        let out = modal.show(ctx, draw);
        let dismissed = out.should_close();
        (out.response, dismissed)
    } else {
        let response = egui::Area::new(Id::unique(&key))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, offset)
            .show(ctx, |ui| {
                widgets::dialog_frame(ctx).show(ui, draw);
            })
            .response;
        let dismissed = response.should_close();
        (response, dismissed)
    };
    let caption = registry(ctx, |r| r.entries.iter().find(|e| e.key == key).map(|e| e.title.clone())).unwrap_or_else(|| default_title(id).into());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Window, true, &caption));
    let close = dismissed || registry(ctx, |r| r.entries.iter().any(|e| e.key == key && e.close));
    if close {
        remove(ctx, &key);
    }
    Response { close }
}

/// Resize the shared popup shell; content scrolls inside its chosen bounds.
fn resize_edges(ui: &mut egui::Ui, key: &str, card: Rect, maximum: Vec2) {
    let directions =
        [vec2(-1.0, -1.0), vec2(0.0, -1.0), vec2(1.0, -1.0), vec2(-1.0, 0.0), vec2(1.0, 0.0), vec2(-1.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 1.0)];
    for (index, direction) in directions.into_iter().enumerate() {
        let corner = direction.x != 0.0 && direction.y != 0.0;
        let size = if corner {
            vec2(14.0, 14.0)
        } else if direction.x == 0.0 {
            vec2((card.width() - 28.0).max(1.0), 6.0)
        } else {
            vec2(6.0, (card.height() - 28.0).max(1.0))
        };
        let center = card.center() + direction * (card.size() - size) * 0.5;
        let cursor = if direction.x == 0.0 {
            egui::CursorIcon::ResizeVertical
        } else if direction.y == 0.0 {
            egui::CursorIcon::ResizeHorizontal
        } else if direction.x == direction.y {
            egui::CursorIcon::ResizeNwSe
        } else {
            egui::CursorIcon::ResizeNeSw
        };
        let response =
            ui.interact(Rect::from_center_size(center, size), Id::unique((key, "resize", index)), egui::Sense::drag()).on_hover_cursor(cursor);
        let title = registry(ui.ctx(), |r| r.entries.iter().find(|e| e.key == key).map(|e| e.title.clone())).unwrap_or_default();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Resize {title} ({index})")));
        if response.dragged() && response.drag_delta().is_finite() {
            let delta = response.drag_delta() * direction;
            let viewport = ui.ctx().content_rect().size();
            registry(ui.ctx(), |r| {
                if let Some(entry) = r.entries.iter_mut().find(|e| e.key == key) {
                    let old = card.size();
                    let size = (old + delta).clamp(vec2(300.0, 180.0).min(maximum), maximum);
                    entry.requested_size = Some(size);
                    let bounds = ((viewport - size - vec2(0.0, 44.0)) * 0.5).max(Vec2::ZERO);
                    entry.offset = (entry.offset + (size - old) * direction * 0.5).clamp(-bounds, bounds);
                }
            });
            ui.ctx().request_repaint();
        }
    }
}

// Move only the active dialog's drafts; live documents, Undo, global preferences and jobs stay in the host.
fn swap_drafts(app: &mut PdfCraftApp, holder: &mut PdfCraftApp, dialog: Dialog) {
    macro_rules! swap{($($field:ident),+)=>{{$(std::mem::swap(&mut app.$field,&mut holder.$field);)+}};}
    use Dialog::*;
    match dialog {
        Assistant => swap!(assistant),
        Templates => swap!(template_gallery),
        SaveTemplate => swap!(template_export),
        WordCount => swap!(word_count_draft),
        CreateImages => swap!(image_import),
        Properties(_) => swap!(props_draft, view_draft, custom_draft),
        Split => swap!(split_draft),
        NumberPages => swap!(number_draft),
        Protect => swap!(protect_draft),
        EncryptCertificate => swap!(certificate_draft),
        PageBoxes => swap!(boxes_draft),
        PageProps => swap!(page_props_draft),
        PrinterMarks => swap!(printer_marks_draft),
        FixHairlines => swap!(hairlines_draft),
        Marks(_) => swap!(marks_draft),
        Export(_) => swap!(export_draft),
        Signature => swap!(signature_draft, signature_preview, signature_photo),
        CommentProps => swap!(comment_props),
        ReplacePages => swap!(replace_draft),
        FieldProps => swap!(field_props),
        BulkFieldProps => swap!(bulk_field_props),
        RedactPages => swap!(redact_pages_draft),
        RedactSearch => swap!(redact_search),
        RedactProps => swap!(redact_prefs, redact_before),
        Print => swap!(print_draft),
        LinkProps => swap!(link_draft),
        Extract => swap!(extract_draft),
        RotatePages => swap!(rotate_draft),
        RemoveHidden => swap!(hidden_draft),
        SummarizeComments => swap!(summary_sort),
        Sign => swap!(sign_draft),
        Timestamp => swap!(timestamp_error),
        Optimize => swap!(optimize_draft),
        DuplicateField => swap!(duplicate_draft),
        FieldTable => swap!(field_table_draft),
        RecognizeText | RecognizeScan { .. } => swap!(ocr_draft),
        CorrectOcrText => swap!(ocr_correction),
        JsConsole => swap!(js_console),
        DocumentJs | DocumentActions => swap!(doc_js),
        PrintProduction => swap!(production),
        Preferences => swap!(preferences_category),
        NewPaper => swap!(new_paper),
        CompareFiles => swap!(compare_old),
        ActionWizard => swap!(wizard),
        PdfA => swap!(pdfa),
        CreateStamp => swap!(stamp_draft),
        AltText => swap!(alt_draft),
        AuditSpace => swap!(space_audit, space_audit_of),
        CertificateViewer => swap!(cert_viewer),
        RemoveSignature => swap!(remove_sig),
        OverwriteChanged => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                swap!(pending_overwrite);
            }
        }
        _ => {}
    }
}
fn park(app: &mut PdfCraftApp, ctx: &egui::Context, slot: Slot) -> bool {
    if app.dialog_shelf.parked.len() >= MAX_PARKED {
        set_minimized(ctx, &slot.key, false);
        app.notify("Close a dialog before minimizing another (16 are already open).");
        return false;
    }
    let settings = app.assistant.settings.clone();
    if slot.dialog == Dialog::Assistant {
        crate::assistant_ui::suspend(&mut app.assistant);
    }
    let doc = if matches!(
        slot.dialog,
        Dialog::Assistant | Dialog::Preferences | Dialog::About | Dialog::Shortcuts | Dialog::Templates | Dialog::NewPaper | Dialog::CreateImages
    ) {
        None
    } else {
        app.active_ids().map(|(_, id)| id)
    };
    let generation = doc.and_then(|id| app.session.get(id).map(|d| d.edit_generation()));
    let mut holder = Box::new(PdfCraftApp::new());
    swap_drafts(app, &mut holder, slot.dialog);
    if slot.dialog == Dialog::Assistant {
        app.assistant.settings = settings;
    }
    if slot.dialog == Dialog::Print {
        app.print_draft = holder.print_draft.clone();
    }
    if matches!(slot.dialog, Dialog::RecognizeText | Dialog::RecognizeScan { .. }) {
        app.ocr_draft = holder.ocr_draft.clone();
    }
    if slot.dialog == Dialog::RedactProps {
        app.redact_prefs = holder.redact_before.clone().unwrap_or_else(|| app.redact_defaults.clone());
    }
    let entry = registry(ctx, |r| {
        r.entries.iter_mut().find(|e| e.key == slot.key).map(|e| {
            e.parked = true;
            e.minimized = true;
            e.clone()
        })
    });
    let caption = entry.as_ref().map_or_else(|| title(slot.dialog), |e| e.title.clone());
    let full = entry.is_some_and(|e| e.maximized);
    app.dialog_shelf.parked.push(Parked { slot, title: caption, doc, generation, drafts: holder, maximized: full });
    true
}
pub(crate) fn prepare(app: &mut PdfCraftApp, ctx: &egui::Context) {
    if let Some(slot) = app.dialog_shelf.active.take() {
        if app.dialog.is_some_and(|d| std::mem::discriminant(&d) == std::mem::discriminant(&slot.dialog)) {
            app.dialog_shelf.active = Some(Slot { dialog: app.dialog.unwrap_or(slot.dialog), ..slot });
        } else if slot.dialog == Dialog::Assistant && app.dialog.is_some() {
            park(app, ctx, slot);
        } else {
            remove(ctx, &slot.key);
        }
    }
    if let Some(dialog) = app.dialog
        && app.dialog_shelf.active.is_none()
    {
        let key = format!("dialog-window-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        touch(ctx, &key, &title(dialog), false);
        app.dialog_shelf.active = Some(Slot { key, dialog });
    }
    if let Some(slot) = &app.dialog_shelf.active {
        let alias = registry_id(ctx).with("main");
        ctx.data_mut(|d| d.insert_temp(alias, slot.key.clone()));
        touch(ctx, &slot.key, &title(slot.dialog), false);
    }
    if app.dialog_shelf.active.is_none() {
        let alias = registry_id(ctx).with("main");
        ctx.data_mut(|d| d.remove::<String>(alias));
    }
    for p in &app.dialog_shelf.parked {
        touch(ctx, &p.slot.key, &p.title, p.maximized);
        registry(ctx, |r| {
            if let Some(e) = r.entries.iter_mut().find(|e| e.key == p.slot.key) {
                e.minimized = true;
                e.parked = true;
            }
        });
    }
}
pub(crate) fn capture(app: &mut PdfCraftApp, ctx: &egui::Context) {
    let Some(slot) = app.dialog_shelf.active.clone() else { return };
    if minimized(ctx, &slot.key) && app.dialog.is_some() && park(app, ctx, slot) {
        app.dialog = None;
        app.dialog_shelf.active = None;
    }
}
fn restore(app: &mut PdfCraftApp, ctx: &egui::Context, key: &str) {
    let Some(index) = app.dialog_shelf.parked.iter().position(|p| p.slot.key == key) else {
        set_minimized(ctx, key, false);
        return;
    };
    let Some(parked) = app.dialog_shelf.parked.get(index) else { return };
    if let Some(doc) = parked.doc
        && app.session.get(doc).is_none()
    {
        app.notify("Bring the PDF back to this window before restoring its dialog, or close the dialog from the taskbar.");
        return;
    }
    if matches!(parked.slot.dialog, Dialog::OverwriteChanged | Dialog::Sign | Dialog::RemoveSignature | Dialog::PageProps)
        && parked.doc.and_then(|id| app.session.get(id).map(|d| d.edit_generation())) != parked.generation
    {
        let parked = app.dialog_shelf.parked.remove(index);
        remove(ctx, &parked.slot.key);
        app.notify("The PDF changed while this dialog was minimized. Open the dialog again; your document was kept.");
        return;
    }
    if let Some(active) = app.dialog_shelf.active.take()
        && !park(app, ctx, active.clone())
    {
        app.dialog_shelf.active = Some(active);
        return;
    }
    let mut parked = app.dialog_shelf.parked.remove(index);
    if let Some(doc) = parked.doc {
        app.active = app.views.iter().position(|v| v.id == doc);
    }
    swap_drafts(app, &mut parked.drafts, parked.slot.dialog);
    app.dialog = Some(parked.slot.dialog);
    if parked.slot.dialog == Dialog::Assistant {
        app.apply_theme(ctx);
    }
    registry(ctx, |r| {
        if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
            e.minimized = false;
            e.parked = false;
            e.maximized = parked.maximized;
        }
    });
    app.dialog_shelf.active = Some(parked.slot);
    ctx.request_repaint();
}
pub(crate) fn taskbar(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    prepare(app, &ctx);
    let frame = ctx.cumulative_frame_nr();
    let parked: Vec<_> = app.dialog_shelf.parked.iter().map(|p| p.slot.key.clone()).collect();
    registry(&ctx, |r| r.entries.retain(|e| if e.parked { parked.contains(&e.key) } else { e.seen >= frame.saturating_sub(1) }));
    registry(&ctx, |r| {
        r.entries.retain(|e| match e.key.as_str() {
            "save_prompt" => app.close_request.is_some(),
            "password" => app.password_prompt.is_some(),
            "unlock" => app.unlock_prompt.is_some(),
            "updates" => app.updates.open,
            "timestamp-progress" => app.timestamp_run.is_some(),
            "background-progress" => app.progress_notice.is_some(),
            "link_prompt" => app.pending_link.is_some(),
            "remote_link_prompt" => app.pending_remote_link.is_some(),
            "full_screen_prompt" => app.pending_full_screen.is_some(),
            _ => true,
        })
    });
    let entries = registry(&ctx, |r| r.entries.clone());
    if entries.is_empty() {
        return;
    }
    let t = theme::Tokens::get(&ctx);
    let mut action = None;
    let mut close = None;
    let mut all = false;
    let panel = egui::Panel::bottom("dialog-taskbar").frame(egui::Frame::NONE).exact_size(42.0).show(ui, |ui| {
        ui.allocate_space(vec2(ui.available_width(), 42.0));
    });
    // The shelf stays clickable above a modal backdrop without exposing the document beneath it.
    egui::Area::new(registry_id(&ctx).with("taskbar")).order(egui::Order::Tooltip).fixed_pos(panel.response.rect.min).show(&ctx, |ui| {
        ui.set_width(panel.response.rect.width());
        egui::Frame::new().fill(t.glass_toolbar_shade().unwrap_or(t.panel)).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let logo = widgets::app_icon(ui, 24.0).on_hover_text(crate::APP_NAME);
                logo.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, "Lion PDF taskbar"));
                let width = (ui.available_width() - 34.0).max(1.0);
                egui::ScrollArea::horizontal().id_salt("dialog-taskbar-items").max_width(width).auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for entry in &entries {
                            ui.scope(|ui| {
                                ui.horizontal(|ui| {
                                    let label = format!("Taskbar: {} {}", if entry.minimized { "Restore" } else { "Minimize" }, entry.title);
                                    let response = ui.add_sized([180.0, 24.0], egui::Button::new(&entry.title).selected(!entry.minimized).truncate());
                                    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                                    if response.on_hover_text(&label).clicked() {
                                        action = Some((entry.key.clone(), entry.minimized));
                                    }
                                    if icons::button(ui, "x", 22.0, false, &format!("Close taskbar window {}", entry.title)).clicked() {
                                        close = Some(entry.key.clone());
                                    }
                                });
                            });
                        }
                    });
                });
                if icons::button(ui, "minimize-2", 24.0, false, "Minimize all windows").clicked() {
                    all = true;
                }
            });
        });
    });
    if all {
        for e in &entries {
            set_minimized(&ctx, &e.key, true);
        }
        capture(app, &ctx);
    }
    if let Some((key, was_minimized)) = action {
        if was_minimized {
            restore(app, &ctx, &key);
        } else {
            set_minimized(&ctx, &key, true);
            capture(app, &ctx);
        }
    }
    if let Some(key) = close {
        if let Some(index) = app.dialog_shelf.parked.iter().position(|p| p.slot.key == key) {
            app.dialog_shelf.parked.remove(index);
            remove(&ctx, &key);
        } else {
            registry(&ctx, |r| {
                if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                    e.close = true;
                }
            });
        }
    }
}
pub(crate) fn command(app: &mut PdfCraftApp, id: &str) {
    let Some(ctx) = app.ctx.clone() else { return };
    prepare(app, &ctx);
    match id {
        "app.dialog_restore" => {
            if let Some(key) = registry(&ctx, |r| r.entries.iter().rev().find(|e| e.minimized).map(|e| e.key.clone())) {
                restore(app, &ctx, &key);
            }
        }
        "app.dialog_minimize_all" => {
            let keys = registry(&ctx, |r| r.entries.iter().map(|e| e.key.clone()).collect::<Vec<_>>());
            for key in keys {
                set_minimized(&ctx, &key, true);
            }
            capture(app, &ctx);
        }
        _ => {
            if let Some(key) = registry(&ctx, |r| r.entries.iter().rev().find(|e| !e.minimized).map(|e| e.key.clone())) {
                if id == "app.dialog_minimize" {
                    set_minimized(&ctx, &key, true);
                    capture(app, &ctx);
                } else {
                    registry(&ctx, |r| {
                        if let Some(e) = r.entries.iter_mut().find(|e| e.key == key) {
                            e.maximized = !e.maximized;
                        }
                    });
                }
            }
        }
    }
    ctx.request_repaint();
}
pub(crate) fn describe(ctx: Option<&egui::Context>) -> serde_json::Value {
    ctx.map_or_else(
        || serde_json::json!([]),
        |ctx| {
            registry(ctx, |r| {
                serde_json::json!(
                    r.entries
                        .iter()
                        .map(|e| serde_json::json!({"title":e.title,"minimized":e.minimized,"maximized":e.maximized}))
                        .collect::<Vec<_>>()
                )
            })
        },
    )
}
pub(crate) fn minimize_notice(ctx: &egui::Context, id: &str, title: &str) {
    let key = effective(ctx, id);
    touch(ctx, &key, title, false);
    set_minimized(ctx, &key, true);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};
    fn app() -> Result<PdfCraftApp, String> {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en")?;
        let bytes = app.session.create_from_text("Synthetic", "A synthetic document.").map_err(|e| e.to_string())?;
        app.open_bytes("window-source.pdf", None, bytes.as_ref().clone())?;
        Ok(app)
    }
    #[test]
    fn taskbar_preserves_separate_dialog_drafts_and_restores_their_source() {
        let mut h = Harness::builder().with_size(vec2(1200.0, 850.0)).build_eframe(|_| {
            let mut app = app().unwrap();
            app.execute("file.properties");
            app
        });
        h.run_steps(6);
        let doc = h.state().views[0].id;
        h.state_mut().props_draft.as_mut().unwrap().1[0] = "An unfinished title".into();
        assert!(h.state_mut().execute("app.dialog_minimize"));
        h.run_steps(3);
        assert!(h.state().dialog.is_none() && !h.state().modal_open());
        assert_eq!(h.state().dialog_shelf.parked.len(), 1);
        assert!(h.state().props_draft.is_none());
        h.state_mut().execute("app.preferences");
        h.run_steps(4);
        assert_eq!(h.state().dialog_shelf.parked.len(), 1);
        // The shelf remains interactive above Preferences' modal backdrop.
        h.get_by_label("Taskbar: Restore Document Properties").click();
        h.run_steps(5);
        assert_eq!(h.state().active_ids().unwrap().1, doc);
        assert_eq!(h.state().props_draft.as_ref().unwrap().1[0], "An unfinished title");
        assert!(!h.state().session.get(doc).unwrap().dirty, "minimizing cannot apply the draft");
        h.get_by_label("Maximize Document Properties").click();
        h.run_steps(3);
        assert!(maximized(&h.ctx, "dialog"));
        h.get_by_label("Close Document Properties").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_none());
        assert!(!h.state().session.get(doc).unwrap().dirty);
    }
    #[test]
    fn minimizing_a_save_confirmation_never_discards_or_approves_changes() {
        let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_eframe(|_| {
            let mut app = app().unwrap();
            let id = app.views[0].id;
            app.session.apply(id, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
            app.request_close_tab(0);
            app
        });
        h.run_steps(5);
        let doc = h.state().views[0].id;
        h.state_mut().execute("app.dialog_minimize");
        h.run_steps(3);
        assert!(h.state().close_request.is_some());
        assert!(h.state().session.get(doc).unwrap().dirty);
        h.state_mut().execute("app.dialog_restore");
        h.run_steps(3);
        h.get_by_label("Close Unsaved changes").click();
        h.run_steps(3);
        assert!(h.state().close_request.is_none());
        assert!(h.state().session.get(doc).unwrap().dirty);
    }
    #[test]
    fn stale_signing_and_save_requests_do_not_resume_against_changed_documents() {
        let mut app = app().unwrap();
        let ctx = egui::Context::default();
        let doc = app.views[0].id;
        app.ctx = Some(ctx.clone());
        app.dialog = Some(Dialog::Sign);
        prepare(&mut app, &ctx);
        command(&mut app, "app.dialog_minimize");
        assert_eq!(app.dialog_shelf.parked.len(), 1);
        app.session.apply(doc, pdfcraft_engine::Edit::RotatePages { pages: vec![0], degrees: 90 }).unwrap();
        command(&mut app, "app.dialog_restore");
        assert!(app.dialog.is_none() && app.dialog_shelf.parked.is_empty());
        assert!(app.session.get(doc).unwrap().dirty);
    }
    #[test]
    fn popup_edges_resize_without_losing_settings_and_restore_the_chosen_size() {
        let mut h = Harness::builder().with_size(vec2(1400.0, 1000.0)).build_eframe(|_| {
            let mut app = app().unwrap();
            app.execute("app.preferences");
            app
        });
        h.run_steps(8);
        let key = effective(&h.ctx, "dialog");
        let before = registry(&h.ctx, |r| r.entries.iter().find(|e| e.key == key).unwrap().size);
        let a = h.get_by_label("Resize Preferences (7)").rect().center();
        let b = a + vec2(90.0, 60.0);
        h.hover_at(a);
        h.run_steps(2);
        h.drag_at(a);
        h.run_steps(2);
        h.hover_at(b);
        h.run_steps(2);
        h.drop_at(b);
        h.run_steps(6);
        let after = registry(&h.ctx, |r| r.entries.iter().find(|e| e.key == key).unwrap().size);
        assert!(after.x > before.x + 40.0 && after.y > before.y + 20.0, "{before:?} -> {after:?}");
        h.get_by_label("Show community button in title bar").click();
        h.run_steps(3);
        assert!(h.state().show_community);
        let saved = h.state().persist();
        let mut restored = PdfCraftApp::new();
        restored.restore(&saved);
        assert!(restored.show_community);
        h.state_mut().execute("app.dialog_minimize");
        h.run_steps(3);
        h.state_mut().execute("app.dialog_restore");
        h.run_steps(6);
        let resumed = registry(&h.ctx, |r| r.entries.iter().find(|e| e.key == key).unwrap().size);
        assert!((resumed - after).length() < 2.0, "{after:?} -> {resumed:?}");
        assert!(h.state().show_community);
        h.input_mut().screen_rect = Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(640.0, 480.0)));
        h.run_steps(8);
        let card = h.ctx.data(|data| data.get_temp::<Rect>(Id::unique("dialog").with("card"))).unwrap();
        assert!(card.left() >= 0.0 && card.top() >= 0.0 && card.right() <= 640.0 && card.bottom() <= 480.0, "{card:?}");
    }

    #[test]
    fn community_shortcut_is_opt_in_and_old_settings_keep_it_hidden() {
        let mut h = Harness::builder().with_size(vec2(1200.0, 850.0)).build_eframe(|_| app().unwrap());
        h.run_steps(5);
        assert!(!h.state().show_community);
        assert!(h.query_by_label("ArtCraft Community").is_none());
        h.state_mut().set_option("show-community", "true").unwrap();
        h.run_steps(3);
        h.get_by_label("ArtCraft Community");
        let mut restored = PdfCraftApp::new();
        restored.restore(&h.state().persist());
        assert!(restored.show_community);
        restored.restore("{}");
        assert!(!restored.show_community);
    }
}
