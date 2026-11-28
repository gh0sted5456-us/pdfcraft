//! Print Production: read-only preflight, source-colour inspection and a page-box map.
use craft::ui::contract::Bind;
use craft::ui::patterns::form_dialogs::{FormButtons, FormDialog};
use craft::ui::sections::{DialogRole, FooterButton};

use crate::PdfCraftApp;
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

/// What the dialog's controls choose.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct View {
    output_tab: bool,
    /// 1-based.
    page: usize,
}

/// The footer buttons after Close.
const COPY: usize = 2;
const REFRESH: usize = 3;
const BOXES: usize = 4;

impl PdfCraftApp {
    /// Lion PDF ▸ Print Production ▸ Preflight, or ▸ Output preview with `output`.
    pub(crate) fn open_production(&mut self, output: bool) {
        let Some((index, id)) = self.active_ids() else { return };
        let page = self.views[index].current();
        self.production = Some(Draft { output_tab: output, page, ..Default::default() });
        self.refresh_production(id);
    }

    fn refresh_production(&mut self, id: DocId) {
        let Some(doc) = self.session.get(id) else { return };
        let Some(production) = self.production.as_mut() else { return };
        let page = production.page.min(doc.info.pages.len().saturating_sub(1));
        let report = doc.output_inspection(page);
        production.id = Some(id);
        production.generation = doc.edit_generation();
        production.page = page;
        production.readiness = Some(doc.print_readiness());
        production.boxes = doc.page_boxes();
        match report {
            Ok(report) => production.output = Some(report),
            Err(error) => {
                production.output = None;
                self.notify_error(error);
            }
        }
    }

    /// One pass of the open Print Production dialog.
    pub(crate) fn production_pass(&mut self, ui: &mut egui::Ui) {
        let Some((index, id)) = self.active_ids() else {
            self.production = None;
            return;
        };
        if self.production.as_ref().is_some_and(|p| p.id != Some(id)) {
            self.refresh_production(id);
        }
        let pages = self.session.get(id).map_or(1, |d| d.info.pages.len()).max(1);
        let changed = self.session.get(id).map(|d| d.edit_generation());
        let Some(production) = self.production.as_ref() else { return };
        let stale = changed.is_some_and(|g| g != production.generation);
        let mut view = View { output_tab: production.output_tab, page: production.page + 1 };
        let mut show_page = None;
        let mut open = true;
        let out = FormDialog::new(tl!("Print Production"), Bind::bound(&mut view))
            .opened(Bind::bound(&mut open))
            .buttons(FormButtons::Close)
            .extra_buttons([
                FooterButton::new(tl!("Copy report"), DialogRole::Other),
                FooterButton::new(tl!("Refresh checks"), DialogRole::Other),
                FooterButton::new(tl!("Adjust page boxes and bleed…"), DialogRole::Other),
            ])
            .show(ui, |ui, _, v| {
                body(ui, production, v, pages, stale, &mut show_page);
                true
            });
        let text = self.production.as_ref().and_then(|p| if p.output_tab { p.output.as_ref().map(OutputInspection::text) } else { p.readiness.as_ref().map(PrintReadiness::text) });
        if out.closed.is_some() {
            self.production = None;
            return;
        }
        let page = view.page.clamp(1, pages) - 1;
        let refresh = out.picked == Some(REFRESH) || self.production.as_ref().is_some_and(|p| p.page != page);
        if let Some(p) = self.production.as_mut() {
            p.output_tab = view.output_tab;
            p.page = page;
        }
        if refresh {
            self.refresh_production(id);
        }
        match out.picked {
            Some(COPY) => {
                if let Some(text) = text {
                    ui.ctx().copy_text(text);
                    self.notify_tr("Copied");
                }
            }
            Some(BOXES) => self.run_command(ui.ctx(), "page.boxes"),
            _ => {}
        }
        if let Some(page) = show_page {
            self.views[index].go_to_page(page);
            self.production = None;
        }
    }
}

/// The dialog's body: the tab, the page, and the chosen report.
fn body(ui: &mut egui::Ui, production: &Draft, v: &mut View, pages: usize, stale: bool, show_page: &mut Option<usize>) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut v.output_tab, false, tl!("Preflight"));
        ui.selectable_value(&mut v.output_tab, true, tl!("Output inspection"));
    });
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Page"));
        ui.add(egui::DragValue::new(&mut v.page).range(1..=pages)).labelled_by(label.id);
        ui.label(crate::i18n::fmt(tl!("of {n}"), &[("n", &pages.to_string())]));
    });
    if stale {
        ui.label(tl!("The document changed: refresh the checks to inspect the latest edits."));
    }
    let height = (ui.ctx().content_rect().height() - 260.0).clamp(120.0, 380.0);
    egui::ScrollArea::vertical().id_salt("production-content").max_height(height).min_scrolled_height(height).auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if production.output_tab {
            ui.label(tl!("Source inventory of this page and its declared resources."));
            if let Some(report) = &production.output {
                let none = tl!("none declared").to_string();
                ui.label(format!("{}: {}", tl!("Colour spaces"), report.spaces.iter().cloned().collect::<Vec<_>>().join(", ")));
                ui.label(format!("{}: {}", tl!("Spot colour names"), if report.spots.is_empty() { none } else { report.spots.iter().cloned().collect::<Vec<_>>().join(", ") }));
                ui.label(format!("{}: {}", tl!("Overprint in resources"), if report.overprint_declared { tl!("declared") } else { tl!("not declared") }));
                ui.label(format!(
                    "{}: {}",
                    tl!("Largest explicit CMYK source sum"),
                    report.max_source_cmyk_percent.map(|n| format!("{n:.1}%")).unwrap_or_else(|| tl!("not measured").to_string())
                ));
                if !report.complete {
                    ui.colored_label(ui.visuals().warn_fg_color, tl!("Inspection is incomplete; see limitations below."));
                }
                for warning in &report.warnings {
                    ui.label(warning);
                }
            }
            ui.add(egui::Label::new(egui::RichText::new(tl!("Source sums are not composited total ink coverage. ICC soft proofs, plate separations and transparency flattening are not simulated.")).small().weak()).wrap());
            ui.separator();
            ui.label(tl!("Page box map (PDF coordinates)"));
            if let Some(boxes) = production.boxes.get(production.page) {
                box_map(ui, boxes);
            }
        } else if let Some(report) = &production.readiness {
            ui.label(crate::i18n::fmt(
                tl!("{pages} pages · {images} image placements · {findings} findings"),
                &[("pages", &report.pages.len().to_string()), ("images", &report.images.len().to_string()), ("findings", &report.findings.len().to_string())],
            ));
            ui.add(egui::Label::new(egui::RichText::new(tl!("Checks include effective image resolution, trim and bleed margins, output intents and font embedding. This is not a PDF/X certification.")).small().weak()).wrap());
            for finding in &report.findings {
                let label = format!("[{}] {}", finding.severity, finding.message);
                if let Some(page) = finding.page {
                    if ui.link(label).clicked() {
                        *show_page = Some(page.saturating_sub(1) as usize);
                    }
                } else {
                    ui.label(label);
                }
            }
            ui.collapsing(tl!("Page sizes and bleed"), |ui| {
                for p in &report.pages {
                    ui.label(format!("{} {}: {:.1} × {:.1} mm; bleed L/B/R/T: {:.1?} mm", tl!("Page"), p.page, p.media_mm[0], p.media_mm[1], p.bleed_margin_mm));
                }
            });
            ui.collapsing(tl!("Images"), |ui| {
                for image in &report.images {
                    ui.label(format!("{} {} · {} · {:.0} ppi", tl!("Page"), image.page, image.name, image.ppi));
                }
            });
            ui.collapsing(tl!("Fonts and output intents"), |ui| {
                ui.label(format!("{}: {}", tl!("Fonts not embedded"), report.fonts_not_embedded.join(", ")));
                ui.label(format!("{}: {}", tl!("Output intents"), report.output_intents.join(", ")));
            });
        }
    });
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
