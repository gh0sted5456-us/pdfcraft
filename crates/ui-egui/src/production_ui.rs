//! Print Production: read-only preflight, source-colour inspection and a page-box map.
use crate::{Dialog, PdfCraftApp, widgets};
use pdfcraft_engine::{DocId, outputcheck::OutputInspection, printcheck::PrintReadiness};

#[derive(Default)]
pub(crate) struct Draft {
    id: Option<DocId>,
    generation: u64,
    output_tab: bool,
    page: usize,
    readiness: Option<PrintReadiness>,
    output: Option<OutputInspection>,
    boxes: Vec<[[f64; 4]; 5]>,
}

impl PdfCraftApp {
    pub(crate) fn open_production(&mut self, output: bool) {
        let Some((index, id)) = self.active_ids() else { return };
        let page = self.views[index].current;
        self.production = Draft { output_tab: output, page, ..Default::default() };
        self.refresh_production(id);
        self.dialog = Some(Dialog::PrintProduction);
    }

    fn refresh_production(&mut self, id: DocId) {
        let Some(doc) = self.session.get(id) else { return };
        let page = self.production.page.min(doc.info.pages.len().saturating_sub(1));
        let report = doc.output_inspection(page);
        self.production.id = Some(id);
        self.production.generation = doc.edit_generation();
        self.production.page = page;
        self.production.readiness = Some(doc.print_readiness());
        self.production.boxes = doc.page_boxes();
        match report {
            Ok(report) => self.production.output = Some(report),
            Err(error) => {
                self.production.output = None;
                self.notify_error(error);
            }
        }
    }
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    let Some((index, id)) = app.active_ids() else { return true };
    if app.production.id != Some(id) {
        app.refresh_production(id);
    }
    crate::widgets::dialog_heading(ui, "Print Production");
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.production.output_tab, false, "Preflight");
        ui.selectable_value(&mut app.production.output_tab, true, "Output inspection");
    });
    ui.separator();
    let mut page = app.production.page + 1;
    let pages = app.session.get(id).map_or(1, |d| d.info.pages.len()).max(1);
    let mut refresh = false;
    ui.horizontal_wrapped(|ui| {
        ui.label("Page");
        refresh |= ui.add(egui::DragValue::new(&mut page).range(1..=pages)).changed();
        ui.label(format!("of {pages}"));
        refresh |= widgets::dialog_button(ui, "Refresh checks", widgets::DialogAction::Back).clicked();
    });
    if refresh {
        app.production.page = page.saturating_sub(1);
        app.refresh_production(id);
    }
    if app.session.get(id).is_some_and(|d| d.edit_generation() != app.production.generation) {
        ui.label("Document changed — refresh checks to inspect the latest edits.");
    }
    let mut show_page = None;
    let mut page_boxes = false;
    let height = (ui.available_height() - 64.0).clamp(16.0, 380.0);
    egui::ScrollArea::vertical().id_salt("production-content").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible).max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if app.production.output_tab {
            ui.label("Source inventory of this page and its declared resources.");
            if let Some(report) = &app.production.output {
                ui.label(format!("Colour spaces: {}", report.spaces.iter().cloned().collect::<Vec<_>>().join(", ")));
                ui.label(format!("Spot colour names: {}", if report.spots.is_empty() { "none declared".into() } else { report.spots.iter().cloned().collect::<Vec<_>>().join(", ") }));
                ui.label(format!("Overprint in resources: {}", if report.overprint_declared { "declared" } else { "not declared" }));
                ui.label(format!("Largest explicit CMYK source sum: {}", report.max_source_cmyk_percent.map(|n| format!("{n:.1}%")).unwrap_or_else(|| "not measured".into())));
                if !report.complete { ui.colored_label(egui::Color32::from_rgb(190, 130, 50), "Inspection is incomplete; see limitations below."); }
                for warning in &report.warnings { ui.label(warning); }
            }
            ui.label("Source sums are not composited total ink coverage. ICC soft proofs, plate separations and transparency flattening are not simulated.");
            ui.separator();
            ui.label("Page box map (PDF coordinates)");
            if let Some(boxes) = app.production.boxes.get(app.production.page) { box_map(ui, boxes); }
            page_boxes = widgets::dialog_button(ui, "Adjust page boxes and bleed…", widgets::DialogAction::Back).clicked();
        } else if let Some(report) = &app.production.readiness {
            ui.label(format!("{} pages · {} image placements · {} findings", report.pages.len(), report.images.len(), report.findings.len()));
            ui.label("Checks include effective image resolution, trim/bleed margins, output intents and font embedding. This is not a PDF/X certification.");
            for finding in &report.findings {
                let label = format!("[{}] {}", finding.severity, finding.message);
                if let Some(page) = finding.page {
                    if ui.link(label).clicked() { show_page = Some(page.saturating_sub(1) as usize); }
                } else { ui.label(label); }
            }
            ui.collapsing("Page sizes and bleed", |ui| {
                for p in &report.pages { ui.label(format!("Page {}: {:.1} × {:.1} mm; bleed L/B/R/T: {:.1?} mm", p.page, p.media_mm[0], p.media_mm[1], p.bleed_margin_mm)); }
            });
            ui.collapsing("Images", |ui| { for image in &report.images { ui.label(format!("Page {} · {} · {:.0} ppi", image.page, image.name, image.ppi)); } });
            ui.collapsing("Fonts and output intents", |ui| {
                ui.label(format!("Fonts not embedded: {}", report.fonts_not_embedded.join(", ")));
                ui.label(format!("Output intents: {}", report.output_intents.join(", ")));
            });
            page_boxes = widgets::dialog_button(ui, "Adjust page boxes and bleed…", widgets::DialogAction::Back).clicked();
        }
    });
    ui.separator();
    let text = if app.production.output_tab {
        app.production.output.as_ref().map(OutputInspection::text)
    } else {
        app.production.readiness.as_ref().map(PrintReadiness::text)
    };
    let mut close = false;
    widgets::dialog_footer(ui, |ui| {
        close = widgets::dialog_button(ui, "Close", widgets::DialogAction::Cancel).clicked();
        if widgets::dialog_button(ui, "Copy report", widgets::DialogAction::Back).clicked()
            && let Some(text) = text
        {
            ui.ctx().copy_text(text);
        }
    });
    if let Some(page) = show_page {
        app.views[index].go_to_page(page);
        close = true;
    }
    if page_boxes {
        app.run_command("page.boxes");
    }
    close
}

fn box_map(ui: &mut egui::Ui, boxes: &[[f64; 4]; 5]) {
    let width = ui.available_width().min(440.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 180.0), egui::Sense::hover());
    let media = boxes[0];
    let w = media[2] - media[0];
    let h = media[3] - media[1];
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return;
    }
    let scale = ((rect.width() - 12.0) as f64 / w).min((rect.height() - 12.0) as f64 / h);
    let origin = rect.center() - egui::vec2((w * scale) as f32, (h * scale) as f32) * 0.5;
    for (index, (name, color)) in [
        ("Media", egui::Color32::GRAY),
        ("Crop", egui::Color32::from_rgb(80, 140, 220)),
        ("Bleed", egui::Color32::from_rgb(210, 100, 100)),
        ("Trim", egui::Color32::from_rgb(70, 170, 110)),
    ]
    .into_iter()
    .enumerate()
    {
        let b = boxes[index];
        let mapped = egui::Rect::from_two_pos(
            origin + egui::vec2(((b[0] - media[0]) * scale) as f32, ((media[3] - b[3]) * scale) as f32),
            origin + egui::vec2(((b[2] - media[0]) * scale) as f32, ((media[3] - b[1]) * scale) as f32),
        );
        if mapped.is_finite() {
            ui.painter().with_clip_rect(rect).rect_stroke(mapped, 0.0, egui::Stroke::new(1.5, color), egui::StrokeKind::Inside);
        }
        ui.painter().text(
            rect.left_top() + egui::vec2(4.0, index as f32 * 18.0),
            egui::Align2::LEFT_TOP,
            name,
            egui::FontId::proportional(12.0),
            color,
        );
    }
}
