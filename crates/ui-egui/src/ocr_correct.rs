//! Review and correct invisible OCR paragraphs while preserving the scanned artwork.
use crate::theme::Tokens;
use crate::{PdfCraftApp, widgets};
use pdfcraft_engine::{BlockStyle, DocId, Edit};

#[derive(Default)]
pub struct Draft {
    pub page: usize,
    pub text: String,
    selected: Option<usize>,
    source: Option<(DocId, u64, usize)>,
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> bool {
    widgets::dialog_heading(ui, tl!("Correct recognized text"));
    let Some((_, id)) = app.active_ids() else { return true };
    let Some(doc) = app.session.get(id) else { return true };
    let generation = doc.edit_generation();
    let count = doc.info.pages.len();
    if count == 0 {
        ui.label(tl!("No pages"));
        return widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked();
    }
    let d = &mut app.ocr_correction;
    d.page = d.page.min(count.saturating_sub(1));
    let mut apply = None;
    widgets::dialog_scroll(ui, "ocr-correction-body", |ui| {
        ui.label(tl!("Corrections update searchable text. The scanned image stays unchanged."));
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let label = ui.label(tl!("Page"));
            let mut number = d.page.saturating_add(1);
            ui.add(egui::DragValue::new(&mut number).range(1..=count)).labelled_by(label.id);
            d.page = number.saturating_sub(1).min(count.saturating_sub(1));
        });
        let blocks: Vec<_> = doc.text_blocks(d.page).into_iter().enumerate().filter(|(_, b)| b.invisible).collect();
        let key = (id, generation, d.page);
        if d.source != Some(key) {
            d.selected = d.selected.filter(|selected| blocks.iter().any(|(index, _)| selected == index)).or_else(|| blocks.first().map(|(i, _)| *i));
            d.text =
                d.selected.and_then(|selected| blocks.iter().find(|(i, _)| *i == selected)).map(|(_, b)| b.editor_text.clone()).unwrap_or_default();
            d.source = Some(key);
        }
        let allowed = doc.allows_modification() && !doc.protected_pages().contains(&d.page);
        if blocks.is_empty() {
            ui.add_space(8.0);
            ui.label(tl!("No recognized text on this page. Run Recognize text first."));
            return;
        }
        ui.add_space(8.0);
        ui.label(tl!("Recognized text"));
        egui::ScrollArea::vertical().id_salt("ocr-paragraphs").max_height(150.0).show(ui, |ui| {
            for (index, block) in &blocks {
                let preview: String = block.editor_text.chars().take(120).collect();
                if ui.selectable_label(d.selected == Some(*index), preview).clicked() {
                    d.selected = Some(*index);
                    d.text = block.editor_text.clone();
                }
            }
        });
        ui.add_space(8.0);
        let label = ui.label(tl!("Corrected text"));
        ui.add_enabled(allowed, widgets::block(&mut d.text).desired_rows(4).desired_width(f32::INFINITY).char_limit(1_000_000)).labelled_by(label.id);
        if !allowed {
            ui.colored_label(t.text_muted, tl!("This page is locked or does not allow editing."));
        }
        if let Some(index) = d.selected
            && let Some((_, block)) = blocks.iter().find(|(i, _)| *i == index)
            && allowed
            && d.text != block.editor_text
        {
            apply = Some((d.page, index, d.text.clone()));
        }
    });
    let mut close = false;
    let mut save = false;
    ui.add_space(8.0);
    widgets::dialog_footer(ui, |ui| {
        save = ui.add_enabled_ui(apply.is_some(), |ui| widgets::dialog_button(ui, tl!("Apply"), widgets::DialogAction::Confirm)).inner.clicked();
        close = widgets::dialog_button(ui, tl!("Close"), widgets::DialogAction::Cancel).clicked();
    });
    if save && let Some((page, block, text)) = apply {
        let style = BlockStyle { keep_invisible: true, ..Default::default() };
        if app.apply_edit(Edit::Batch { label: "Correct recognized text".into(), edits: vec![Edit::EditTextBlock { page, block, text, style }] }) {
            app.ocr_correction.source = None;
        }
    }
    close
}
