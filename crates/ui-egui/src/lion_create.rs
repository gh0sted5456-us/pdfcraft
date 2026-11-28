//! Lion PDF ▸ File ▸ Create ▸ Custom page: a new document of blank pages at a paper size and
//! orientation the user picks, or at a size they type, in inches, millimetres or points.

use craft::ui::contract::Bind;
use craft::ui::dialog::{DialogResult, dialog_id};
use craft::ui::patterns::form_dialogs::{DialogFields, FieldValue, HostedParamDialog, Param, ParamDialog, ParamKind};
use craft::ui::runtime::env::Label;

use crate::PdfCraftApp;

const KEY: &str = "create.custom";
/// The custom size's entry in the paper list.
const CUSTOM: &str = "custom";
/// The page sizes PDF readers accept, in points.
const SIDE: std::ops::RangeInclusive<f64> = 3.0..=14_400.0;
/// The most pages one new document starts with.
const MAX_PAGES: i64 = 500;

/// Points in one of `unit`.
fn points_per(unit: &str) -> f64 {
    match unit {
        "mm" => 72.0 / 25.4,
        "pt" => 1.0,
        _ => 72.0,
    }
}

/// The paper sizes listed, portrait, in points.
fn papers() -> impl Iterator<Item = (&'static str, (f64, f64))> {
    pdfcraft_engine::print::PAPERS.iter().copied()
}

/// The page size the fields describe, in points, with the orientation applied; `None` when a typed
/// side is out of range.
fn size_of(fields: &DialogFields) -> Option<(f64, f64)> {
    let paper = fields.text("paper").unwrap_or(CUSTOM);
    let (w, h) = match papers().find(|(name, _)| *name == paper) {
        Some((_, size)) => size,
        None => {
            let per = points_per(fields.text("unit").unwrap_or("in"));
            (fields.number("width")? * per, fields.number("height")? * per)
        }
    };
    if !SIDE.contains(&w) || !SIDE.contains(&h) {
        return None;
    }
    let landscape = fields.text("orientation") == Some("landscape");
    Some(if landscape == (w < h) { (h, w) } else { (w, h) })
}

/// Opens Custom page, the size of a new blank document selected.
pub(crate) fn open_custom_page(app: &mut PdfCraftApp, ctx: &egui::Context) {
    let (w, h) = app.blank_paper.points();
    let paper = papers().find(|(_, size)| (size.0 - w).abs() < 0.5 && (size.1 - h).abs() < 0.5).map_or(CUSTOM, |(name, _)| name);
    let mut fields = DialogFields::new();
    fields.set("paper", paper.to_string());
    fields.set("unit", "in".to_string());
    fields.set("width", FieldValue::Number((w / 72.0 * 100.0).round() / 100.0));
    fields.set("height", FieldValue::Number((h / 72.0 * 100.0).round() / 100.0));
    fields.set("orientation", "portrait".to_string());
    fields.set("pages", FieldValue::Number(1.0));
    let dialog = HostedParamDialog::new(ctx, KEY, fields, |fields| {
        let usable = size_of(fields).is_some();
        let mut sizes: Vec<(String, Label)> = papers().map(|(name, _)| (name.to_string(), Label::from(tl!(name)))).collect();
        sizes.push((CUSTOM.to_string(), Label::from(tl!("Custom size"))));
        let units = [("in", tl!("Inches")), ("mm", tl!("Millimetres")), ("pt", tl!("Points"))];
        let rows = vec![
            Param::new("paper", tl!("Paper size"), ParamKind::choice(sizes, None)),
            Param::new("unit", tl!("Units"), ParamKind::choice(units.iter().map(|(id, l)| (id.to_string(), Label::from(*l))).collect(), None))
                .visible_when("paper", CUSTOM),
            Param::new("width", tl!("Width"), ParamKind::number(Some(0.01), Some(14_400.0), 8.5, 0.1, 2)).visible_when("paper", CUSTOM),
            Param::new("height", tl!("Height"), ParamKind::number(Some(0.01), Some(14_400.0), 11.0, 0.1, 2)).visible_when("paper", CUSTOM),
            Param::new(
                "orientation",
                tl!("Orientation"),
                ParamKind::radio(vec![("portrait".into(), Label::from(tl!("Portrait"))), ("landscape".into(), Label::from(tl!("Landscape")))], None),
            ),
            Param::new("pages", tl!("Pages"), ParamKind::int(Some(1), Some(MAX_PAGES), 1)),
        ];
        let mut dialog =
            ParamDialog::new(KEY, tl!("Custom page"), rows).values(Bind::bound(fields)).ok_label(tl!("Create").into()).ok_enabled(usable);
        if !usable {
            dialog = dialog.ok_reason(Label::from(tl!("Each side must be between 3 and 14,400 points (200 inches).")));
        }
        dialog
    });
    crate::dialogs::host::open(app, dialog);
}

impl PdfCraftApp {
    /// Custom page's OK: create the document and open it.
    pub(crate) fn custom_page_pass(&mut self) {
        let Some(DialogResult::Ok(result)) = self.dialogs.take_by_id::<HostedParamDialog>(dialog_id(KEY)) else { return };
        let Some((w, h)) = size_of(&result.fields) else { return };
        let pages = result.fields.number("pages").unwrap_or(1.0).clamp(1.0, MAX_PAGES as f64) as usize;
        let created = self.session.create_blank(w, h, pages).map_err(|e| e.to_string());
        if let Err(e) = created.and_then(|bytes| self.open_new_document("Untitled.pdf", bytes)) {
            self.notify_fmt("Couldn't create a PDF: {e}", &[("e", &e)]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, FieldValue)]) -> DialogFields {
        let mut f = DialogFields::new();
        for (k, v) in pairs {
            f.set(*k, v.clone());
        }
        f
    }

    #[test]
    fn landscape_puts_the_long_side_across() {
        let (w, h) = papers().next().map(|(_, s)| s).unwrap_or((612.0, 792.0));
        let name = papers().next().map(|(n, _)| n).unwrap_or_default();
        let portrait = size_of(&fields(&[("paper", name.to_string().into()), ("orientation", "portrait".to_string().into())]));
        let landscape = size_of(&fields(&[("paper", name.to_string().into()), ("orientation", "landscape".to_string().into())]));
        assert_eq!(portrait.map(|(a, b)| a <= b), Some(w.min(h) == w));
        assert!(landscape.is_some_and(|(a, b)| a >= b));
    }

    #[test]
    fn a_custom_size_is_converted_and_held_to_what_readers_accept() {
        let inch = |w: f64, h: f64, unit: &str| {
            size_of(&fields(&[
                ("paper", CUSTOM.to_string().into()),
                ("unit", unit.to_string().into()),
                ("width", FieldValue::Number(w)),
                ("height", FieldValue::Number(h)),
            ]))
        };
        assert!(inch(2.0, 3.0, "in").is_some_and(|(w, h)| w < h && w > 100.0));
        assert!(inch(210.0, 297.0, "mm").is_some_and(|(w, _)| (w - 595.0).abs() < 1.0));
        assert_eq!(inch(0.01, 3.0, "in"), None, "a side under 3 points is refused");
        assert_eq!(inch(300.0, 3.0, "in"), None, "a side over 200 inches is refused");
    }
}
