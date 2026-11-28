//! Lion PDF: the few egui helpers Lion PDF's own dialogs draw with, inside the framework's dialogs.
//! Colours come from egui's visuals, which follow the Lion theme (`lion_theme::egui_visuals`).

use egui::{Response, RichText, Stroke, vec2};

/// The height of a field or button in these dialogs.
const FIELD_MIN_HEIGHT: f32 = 28.0;

/// What a footer button does, which decides how it looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogAction {
    /// The dialog's main action: filled in the accent.
    Confirm,
    /// Closes without acting.
    Cancel,
    /// A secondary action.
    Back,
    /// An action that loses work: filled in the error colour.
    Destructive,
}

/// Outlined controls at a shared height, so fields read against the dialog.
pub fn begin_dialog(ui: &mut egui::Ui) {
    let border = ui.visuals().widgets.noninteractive.bg_stroke.color;
    let w = &mut ui.visuals_mut().widgets;
    w.inactive.bg_stroke = Stroke::new(1.0, border);
    ui.spacing_mut().interact_size.y = FIELD_MIN_HEIGHT;
}

/// A single-line field.
pub fn line(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).min_size(vec2(0.0, FIELD_MIN_HEIGHT))
}

/// A multi-line field.
pub fn block(text: &mut dyn egui::TextBuffer) -> egui::TextEdit<'_> {
    egui::TextEdit::multiline(text)
}

/// A check box.
pub fn checkbox<'a>(checked: &'a mut bool, atoms: impl egui::IntoAtoms<'a>) -> impl egui::Widget + 'a {
    egui::Checkbox::new(checked, atoms)
}

/// A footer button styled for what it does.
pub(crate) fn dialog_button(ui: &mut egui::Ui, label: &str, action: DialogAction) -> Response {
    let (fill, ink) = match action {
        DialogAction::Confirm => (Some(ui.visuals().hyperlink_color), Some(egui::Color32::WHITE)),
        DialogAction::Destructive => (Some(ui.visuals().error_fg_color), Some(egui::Color32::WHITE)),
        DialogAction::Cancel | DialogAction::Back => (None, None),
    };
    let text = match ink {
        Some(ink) => RichText::new(label).color(ink),
        None => RichText::new(label),
    };
    let mut button = egui::Button::new(text).min_size(vec2(80.0, FIELD_MIN_HEIGHT));
    if let Some(fill) = fill {
        button = button.fill(fill);
    }
    ui.add(button)
}

/// A button with an icon's name before its label; `primary` fills it in the accent.
pub fn icon_pill(ui: &mut egui::Ui, _icon: &str, label: &str, primary: bool) -> Response {
    dialog_button(ui, label, if primary { DialogAction::Confirm } else { DialogAction::Back })
}

/// A heading inside a dialog's body.
pub(crate) fn dialog_heading(ui: &mut egui::Ui, title: impl Into<String>) -> Response {
    ui.label(RichText::new(title.into()).strong().size(16.0))
}

/// A section title inside a dialog's body.
pub fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong());
}

/// The buttons at the foot of a body, right-aligned.
pub(crate) fn dialog_footer<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<R> {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), content)
}

/// A body part that scrolls when it is taller than the room.
pub(crate) fn dialog_scroll<R>(ui: &mut egui::Ui, id: &str, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::vertical().id_salt(id).max_height((ui.ctx().content_rect().height() - 240.0).max(160.0)).show(ui, content).inner
}

/// A radio button.
pub fn radio<'a>(selected: bool, atoms: impl egui::IntoAtoms<'a>) -> impl egui::Widget + 'a {
    egui::RadioButton::new(selected, atoms)
}

/// A radio button that sets `current` to `alternative`.
pub fn radio_value<'a, V: PartialEq>(ui: &mut egui::Ui, current: &mut V, alternative: V, atoms: impl egui::IntoAtoms<'a>) -> Response {
    let mut response = ui.add(radio(*current == alternative, atoms));
    if response.clicked() && *current != alternative {
        *current = alternative;
        response.mark_changed();
    }
    response
}
