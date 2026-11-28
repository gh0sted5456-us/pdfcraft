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

/// A check box (the framework's); whether it changed `checked` this pass.
pub fn check(ui: &mut egui::Ui, checked: &mut bool, label: &str) -> bool {
    use craft::ui::contract::{Props as _, Show as _};
    craft::ui::choice::Checkbox::new(craft::ui::contract::Bind::bound(checked)).label(label.to_string()).show(ui).value().is_some()
}

/// A drop-down list (the framework's) named `label` over `options`; whether it changed `value`.
pub fn select<T: Clone + PartialEq + Send + Sync + 'static>(ui: &mut egui::Ui, label: &str, value: &mut T, options: Vec<(T, String)>) -> bool {
    use craft::ui::contract::Show as _;
    let entries: Vec<craft::ui::contract::Entry<T>> = options.into_iter().map(|(v, l)| craft::ui::contract::Entry::new(v, l)).collect();
    let before = value.clone();
    craft::ui::pickers::Select::new(label.to_string(), craft::ui::contract::Bind::bound(value), &entries).show(ui);
    *value != before
}

/// Radio buttons (the framework's) named `label` over `options`.
pub fn radios<T: Clone + PartialEq + Send + Sync + 'static>(ui: &mut egui::Ui, label: &str, value: &mut T, options: Vec<(T, String)>) {
    use craft::ui::contract::{Props as _, Show as _, ShowContainer as _};
    craft::ui::choice::RadioGroup::new(label.to_string(), craft::ui::contract::Bind::bound(value)).show(ui, |ui, _| {
        for (v, l) in &options {
            craft::ui::choice::Radio::new(v.clone()).label(l.clone()).show(ui);
        }
    });
}

/// A footer button styled for what it does.
pub(crate) fn dialog_button(ui: &mut egui::Ui, label: &str, action: DialogAction) -> Response {
    let (fill, ink) = match action {
        DialogAction::Confirm => (Some(ui.visuals().hyperlink_color), Some(egui::Color32::WHITE)),
        DialogAction::Destructive => (Some(ui.visuals().error_fg_color), Some(egui::Color32::WHITE)),
        DialogAction::Back => (None, None),
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
