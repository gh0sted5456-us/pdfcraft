//! Physical page dimensions, independent of raster resolution and viewing zoom.
use crate::{PdfCraftApp, units::Unit, widgets};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Paper {
    pub width: f64,
    pub height: f64,
}

impl Default for Paper {
    fn default() -> Self {
        Self { width: 612.0, height: 792.0 }
    }
}

impl Paper {
    pub fn valid(&self) -> bool {
        [self.width, self.height].into_iter().all(|n| n.is_finite() && (3.0..=14_400.0).contains(&n))
    }

    pub fn preset(name: &str) -> Option<Self> {
        let size = match name {
            "US business card" => (252.0, 144.0),
            "European business card" => (85.0 * 72.0 / 25.4, 55.0 * 72.0 / 25.4),
            "Japanese business card" => (91.0 * 72.0 / 25.4, 55.0 * 72.0 / 25.4),
            _ => pdfcraft_engine::print::PAPERS.iter().find(|(label, _)| *label == name)?.1,
        };
        Some(Self { width: size.0, height: size.1 })
    }

    pub fn label(&self) -> &str {
        Self::names().find(|name| Self::preset(name).as_ref() == Some(self)).unwrap_or("Custom")
    }

    fn names() -> impl Iterator<Item = &'static str> {
        pdfcraft_engine::print::PAPERS.iter().map(|(name, _)| *name).chain(["US business card", "European business card", "Japanese business card"])
    }
}

pub(crate) fn controls(ui: &mut egui::Ui, paper: &mut Paper, unit: &mut Unit) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Paper size");
        egui::ComboBox::from_id_salt("physical-paper").selected_text(tl!(paper.label())).show_ui(ui, |ui| {
            for name in Paper::names() {
                if ui.selectable_label(paper.label() == name, tl!(name)).clicked()
                    && let Some(value) = Paper::preset(name)
                {
                    *paper = value;
                }
            }
        });
        egui::ComboBox::from_id_salt("physical-units").selected_text(unit.label()).show_ui(ui, |ui| {
            for value in Unit::ALL {
                ui.selectable_value(unit, value, value.label());
            }
        });
    });
    egui::Grid::new("physical-dimensions").num_columns(2).show(ui, |ui| {
        for (label, side) in [("Width", &mut paper.width), ("Height", &mut paper.height)] {
            let l = ui.label(tl!(label));
            let k = unit.per_point();
            let mut value = *side * k;
            if ui
                .add(
                    egui::DragValue::new(&mut value)
                        .range(3.0 * k..=14_400.0 * k)
                        .speed(unit.speed())
                        .max_decimals(3)
                        .suffix(format!(" {}", unit.label())),
                )
                .labelled_by(l.id)
                .changed()
            {
                *side = value / k;
            }
            ui.end_row();
        }
    });
    if ui.button(tl!("Swap width and height")).clicked() {
        std::mem::swap(&mut paper.width, &mut paper.height);
    }
    ui.label(egui::RichText::new("US: Letter / Legal. Europe and international: ISO A sizes. Card presets are finished trim sizes.").small());
}

fn calibration(measured_mm: f32) -> Option<f32> {
    (measured_mm.is_finite() && (12.5..=200.0).contains(&measured_mm)).then(|| 50.0 / measured_mm)
}

pub(crate) fn preferences(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    let mut show_actual = false;
    widgets::section_title(ui, "New documents");
    controls(ui, &mut app.default_paper, &mut app.units);
    ui.label("Used for new blank PDFs. Designed templates keep their Letter/A4 layout; imported PDFs keep their own dimensions.");
    widgets::section_title(ui, "Printing");
    ui.horizontal_wrapped(|ui| {
        ui.label("Default print paper");
        let label = pdfcraft_engine::print::PAPERS.get(app.print_draft.paper).map_or("US Letter", |p| p.0);
        egui::ComboBox::from_id_salt("default-print-paper").selected_text(tl!(label)).show_ui(ui, |ui| {
            for (i, (name, _)) in pdfcraft_engine::print::PAPERS.iter().enumerate() {
                ui.selectable_value(&mut app.print_draft.paper, i, tl!(*name));
            }
        });
    });
    ui.label("Actual size in Print preserves physical dimensions. Fit and Shrink can change them; the printer must support the selected paper.");
    widgets::section_title(ui, "Calibrate actual-size viewing");
    ui.label("Measure the line below with a physical ruler. Enter its measured length, then choose Calibrate. Repeat after changing monitors or display scaling.");
    let (r, _) = ui.allocate_exact_size(egui::vec2(50.0 * 96.0 / 25.4, 16.0), egui::Sense::hover());
    let stroke = egui::Stroke::new(1.5, ui.visuals().text_color());
    ui.painter().line_segment([r.left_center(), r.right_center()], stroke);
    for x in [r.left(), r.right()] {
        ui.painter().line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], stroke);
    }
    ui.horizontal_wrapped(|ui| {
        let l = ui.label("Measured length");
        ui.add(egui::DragValue::new(&mut app.measured_ruler_mm).range(12.5..=200.0).suffix(" mm").speed(0.1)).labelled_by(l.id);
        let calibrated = calibration(app.measured_ruler_mm);
        if ui.add_enabled(calibrated.is_some(), egui::Button::new("Calibrate")).clicked()
            && let Some(zoom) = calibrated
        {
            app.actual_size_zoom = zoom;
        }
        if ui.button(tl!("Reset")).clicked() {
            app.actual_size_zoom = 1.0;
            app.measured_ruler_mm = 50.0;
        }
    });
    ui.label(format!("Actual-size display: {:.1}%", app.actual_size_zoom * 100.0));
    if ui.add_enabled_ui(app.active.is_some(), |ui| widgets::dialog_button(ui, "Show actual size", widgets::DialogAction::Back)).inner.clicked() {
        if let Some(view) = app.active.and_then(|i| app.views.get_mut(i)) {
            view.actual_size_zoom = app.actual_size_zoom;
            view.set_actual_size();
        }
        show_actual = true;
    }
    ui.label("Calibration changes viewing zoom only. PDF dimensions and print scale are unchanged.");
    show_actual
}

pub(crate) fn new_body(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> (bool, bool) {
    crate::widgets::dialog_heading(ui, "Custom PDF page");
    controls(ui, &mut app.new_paper, &mut app.units);
    ui.add_space(12.0);
    let (mut go, mut cancel) = (false, false);
    widgets::dialog_footer(ui, |ui| {
        go = ui.add_enabled_ui(app.new_paper.valid(), |ui| widgets::dialog_button(ui, tl!("Create"), widgets::DialogAction::Confirm)).inner.clicked();
        cancel = widgets::dialog_button(ui, tl!("Cancel"), widgets::DialogAction::Cancel).clicked();
    });
    (go, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_dimensions_and_calibration_reject_invalid_numbers() {
        for value in [0.0, -1.0, f32::NAN, f32::INFINITY, 1000.0] {
            assert!(calibration(value).is_none());
        }
        assert_eq!(calibration(40.0), Some(1.25));
        for value in [0.0, f64::NAN, f64::INFINITY, 14_401.0] {
            assert!(!Paper { width: value, height: 144.0 }.valid());
        }
        assert_eq!(Paper::preset("US business card"), Some(Paper { width: 252.0, height: 144.0 }));
        assert!(Paper::preset("unknown").is_none());
    }
}
