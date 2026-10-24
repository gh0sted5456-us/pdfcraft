//! Spelling marks are editor chrome. They never alter or enter a PDF's appearance.
use egui::{Color32, Pos2, Stroke};
use pdfcraft_engine::spelling::{self, Checker};
use std::{cell::RefCell, ops::Range};

#[derive(Clone)]
pub(crate) struct Settings {
    pub enabled: bool,
    pub language: String,
    pub words: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true, language: "auto".into(), words: Vec::new() }
    }
}

#[derive(Default)]
struct Cache {
    language: String,
    checker: Option<Result<Checker, String>>,
    text: String,
    ranges: Vec<Range<usize>>,
}
thread_local! { static CACHE: RefCell<Cache> = RefCell::new(Cache::default()); }
fn key() -> egui::Id {
    egui::Id::new("spelling-settings")
}

pub(crate) fn begin_frame(ctx: &egui::Context, settings: &mut Settings, interface: &str) {
    // Words added through an editor are collected before persisting the next app frame.
    if let Some(previous) = ctx.data(|d| d.get_temp::<Settings>(key())) {
        for word in previous.words {
            if settings.words.len() < 1000 && !settings.words.contains(&word) {
                settings.words.push(word);
            }
        }
    }
    let mut active = settings.clone();
    if active.language == "auto" {
        active.language = spelling::for_interface(interface);
    }
    ctx.data_mut(|d| d.insert_temp(key(), active));
}

fn with_checker<T>(language: &str, f: impl FnOnce(&Checker) -> T) -> Result<T, String> {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.language != language || cache.checker.is_none() {
            cache.language = language.into();
            cache.checker = Some(Checker::new(language));
            cache.text.clear();
            cache.ranges.clear();
        }
        match cache.checker.as_ref() {
            Some(Ok(checker)) => Ok(f(checker)),
            Some(Err(e)) => Err(e.clone()),
            None => Err("Spelling unavailable".into()),
        }
    })
}

pub(crate) fn ranges(ctx: &egui::Context, text: &str) -> Vec<Range<usize>> {
    let Some(settings) = ctx.data(|d| d.get_temp::<Settings>(key())) else { return Vec::new() };
    if !settings.enabled || text.is_empty() || text.len() > spelling::MAX_TEXT_BYTES {
        return Vec::new();
    }
    let _ = with_checker(&settings.language, |_| ());
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.text != text {
            cache.ranges = match cache.checker.as_ref() {
                Some(Ok(checker)) => checker.check(text).unwrap_or_default(),
                _ => Vec::new(),
            };
            cache.text = text.to_string();
        }
        cache.ranges.iter().filter(|r| text.get((*r).clone()).is_some_and(|w| !settings.words.contains(&w.to_lowercase()))).cloned().collect()
    })
}

/// Paint underlines with the editor's actual galley, preserving rich text, wrapping and alignment.
pub(crate) fn annotate(ui: &mut egui::Ui, out: &mut egui::text_edit::TextEditOutput, text: &mut String, max_chars: Option<usize>) {
    let ranges = ranges(ui.ctx(), text);
    let painter = ui.painter().with_clip_rect(out.text_clip_rect.intersect(ui.clip_rect()));
    for range in &ranges {
        let Some(before) = text.get(..range.start) else { continue };
        let Some(word) = text.get(range.clone()) else { continue };
        let start = before.chars().count();
        for index in start..start.saturating_add(word.chars().count()) {
            let a = out.galley.pos_from_cursor(egui::text::CCursor::new(index));
            let b = out.galley.pos_from_cursor(egui::text::CCursor::new(index.saturating_add(1)));
            let width = if (a.top() - b.top()).abs() < 1.0 { (b.left() - a.left()).max(2.0) } else { 4.0 };
            let p = out.galley_pos + egui::vec2(a.left(), a.bottom() - 1.0);
            let points: Vec<Pos2> =
                (0..=(width.ceil() as usize).min(128)).map(|x| p + egui::vec2(x as f32, if x % 4 < 2 { 0.0 } else { 1.5 })).collect();
            painter.add(egui::Shape::line(points, Stroke::new(1.0, Color32::from_rgb(201, 65, 75))));
        }
    }
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let at = pointer.map(|p| usize::from(out.galley.cursor_from_pos(p - out.galley_pos).index));
    let range = at
        .and_then(|at| {
            ranges.iter().find(|r| {
                let start = text.get(..r.start).map_or(0, |s| s.chars().count());
                let end = text.get(..r.end).map_or(0, |s| s.chars().count());
                (start..end).contains(&at)
            })
        })
        .cloned();
    // Keep the word captured when the context menu opened, even after the pointer enters it.
    let menu_key = out.response.id.with("spelling-word");
    if out.response.secondary_clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(menu_key, range));
    }
    let chosen = ui.ctx().data(|d| d.get_temp::<Option<Range<usize>>>(menu_key)).flatten();
    let Some(range) = chosen else { return };
    let Some(word) = text.get(range.clone()).map(str::to_owned) else { return };
    let response = out.response.response.clone();
    response.context_menu(|ui| {
        let Some(settings) = ui.ctx().data(|d| d.get_temp::<Settings>(key())) else { return };
        ui.label(tl!("Spelling suggestions"));
        let suggestion_key = out.response.id.with("spelling-suggestions");
        let cached = ui.ctx().data(|d| d.get_temp::<(String, String, Vec<String>)>(suggestion_key));
        let suggestions = match cached {
            Some((lang, previous, suggestions)) if lang == settings.language && previous == word => suggestions,
            _ => {
                let suggestions = with_checker(&settings.language, |c| c.suggest(&word)).ok().and_then(Result::ok).unwrap_or_default();
                ui.ctx().data_mut(|d| d.insert_temp(suggestion_key, (settings.language.clone(), word.clone(), suggestions.clone())));
                suggestions
            }
        };
        if suggestions.is_empty() {
            ui.label(tl!("No suggestions"));
        }
        for suggestion in suggestions.iter().take(10) {
            let fits = max_chars
                .is_none_or(|max| text.chars().count().saturating_sub(word.chars().count()).saturating_add(suggestion.chars().count()) <= max);
            if ui.add_enabled(fits, egui::Button::new(suggestion)).clicked() && text.get(range.clone()) == Some(word.as_str()) {
                text.replace_range(range.clone(), suggestion);
                out.response.response.mark_changed();
                out.response.request_focus();
                ui.close();
            }
        }
        ui.separator();
        if ui.add_enabled(settings.words.len() < 1000 && word.len() <= 128, egui::Button::new(tl!("Add to dictionary"))).clicked() {
            ui.ctx().data_mut(|d| {
                let mut settings = settings;
                settings.words.push(word.to_lowercase());
                d.insert_temp(key(), settings);
            });
            out.response.request_focus();
            ui.close();
        }
    });
}

pub(crate) fn preferences(ui: &mut egui::Ui, app: &mut crate::PdfCraftApp) {
    ui.add(crate::widgets::checkbox(&mut app.spelling.enabled, tl!("Check spelling while typing")));
    ui.horizontal(|ui| {
        let label = ui.label(tl!("Spelling language"));
        let selected = spelling::LANGUAGES.iter().find(|(tag, _)| *tag == app.spelling.language).map_or_else(
            || {
                if app.spelling.language == "auto" { tl!("Same as interface").to_owned() } else { app.spelling.language.clone() }
            },
            |(_, name)| (*name).to_owned(),
        );
        egui::ComboBox::from_id_salt("spelling-language")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut app.spelling.language, "auto".into(), tl!("Same as interface"));
                for (tag, name) in spelling::LANGUAGES {
                    ui.selectable_value(&mut app.spelling.language, tag.into(), name);
                }
                for tag in Checker::available_languages().into_iter().filter(|tag| !spelling::LANGUAGES.iter().any(|(l, _)| l == tag)) {
                    ui.selectable_value(&mut app.spelling.language, tag.clone(), tag);
                }
            })
            .response
            .labelled_by(label.id);
    });
    let language = if app.spelling.language == "auto" {
        spelling::for_interface(crate::i18n::Lang::from_pref(&app.language).code())
    } else {
        app.spelling.language.clone()
    };
    if app.spelling.enabled {
        if let Err(error) = with_checker(&language, |_| ()) {
            ui.label(format!("{}: {error}", tl!("Spelling dictionary unavailable")));
        } else {
            ui.label(tl!("Spelling checks stay on this device."));
        }
    }
    ui.label(tl!("Spelling marks do not change the document. Right-click an underlined word for suggestions."));
    if !app.spelling.words.is_empty() && ui.button(tl!("Clear personal dictionary")).clicked() {
        app.spelling.words.clear();
        ui.ctx().data_mut(|d| d.remove::<Settings>(key()));
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn a_spelling_suggestion_changes_only_the_chosen_word() {
        let mut fonts = false;
        let mut h = egui_kittest::Harness::new_ui_state(
            move |ui, state: &mut (String, egui::Pos2)| {
                crate::i18n::set_current(crate::i18n::Lang::EN);
                if !fonts {
                    ui.ctx().set_fonts(crate::theme::font_definitions());
                    fonts = true;
                    return;
                }
                let mut settings = Settings { language: "en-US".into(), ..Default::default() };
                begin_frame(ui.ctx(), &mut settings, "en");
                let mut out = egui::TextEdit::multiline(&mut state.0).desired_width(300.0).show(ui);
                state.1 = out.galley_pos + out.galley.pos_from_cursor(egui::text::CCursor::new(3)).center().to_vec2();
                annotate(ui, &mut out, &mut state.0, None);
            },
            ("beleeve the document".into(), egui::Pos2::ZERO),
        );
        h.run_steps(3);
        let pos = h.state().1;
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE });
            h.run_steps(1);
        }
        h.run_steps(2);
        assert_eq!(h.state().0, "beleeve the document", "opening the menu cannot change text");
        h.get_by_label("believe").click();
        h.run_steps(2);
        assert_eq!(h.state().0, "believe the document");
    }
    #[test]
    fn spelling_marks_respect_language_personal_words_and_off_switch() {
        let ctx = egui::Context::default();
        let mut settings = Settings { language: "en-US".into(), ..Default::default() };
        begin_frame(&ctx, &mut settings, "en");
        let text = "café 😀 beleeve";
        assert!(ranges(&ctx, text).iter().any(|r| text.get(r.clone()) == Some("beleeve")));
        settings.words.push("beleeve".into());
        begin_frame(&ctx, &mut settings, "en");
        assert!(!ranges(&ctx, text).iter().any(|r| text.get(r.clone()) == Some("beleeve")));
        settings.enabled = false;
        begin_frame(&ctx, &mut settings, "en");
        assert!(ranges(&ctx, "nothaword").is_empty());
    }
}
