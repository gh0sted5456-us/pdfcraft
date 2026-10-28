//! The Settings dialog: a category rail, an independently scrolling pane and fixed actions.

use crate::{
    PdfCraftApp, icons,
    theme::{self, Tokens},
    widgets,
};
use egui::{Align, Layout, RichText, vec2};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreferencesCategory {
    #[default]
    Appearance,
    Documents,
    Paper,
    Language,
    Reading,
    Identity,
    Signing,
    JavaScript,
    Editing,
    Ocr,
    Pages,
    Security,
    Assistant,
    Updates,
}

impl PreferencesCategory {
    pub const ALL: [Self; 14] = [
        Self::Appearance,
        Self::Documents,
        Self::Paper,
        Self::Editing,
        Self::Pages,
        Self::Ocr,
        Self::Language,
        Self::Reading,
        Self::Identity,
        Self::Signing,
        Self::Security,
        Self::JavaScript,
        Self::Assistant,
        Self::Updates,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            Self::Appearance => "appearance",
            Self::Documents => "documents",
            Self::Paper => "paper",
            Self::Language => "language",
            Self::Reading => "reading",
            Self::Identity => "identity",
            Self::Signing => "signing",
            Self::JavaScript => "javascript",
            Self::Editing => "editing",
            Self::Ocr => "ocr",
            Self::Pages => "pages",
            Self::Security => "security",
            Self::Assistant => "assistant",
            Self::Updates => "updates",
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|category| category.key() == value)
            .ok_or_else(|| "preferences-category must be appearance, documents, paper, editing, pages, ocr, language, reading, identity, signing, security, javascript, assistant or updates".into())
    }

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Interface and appearance",
            Self::Documents => "Documents and view",
            Self::Paper => "Paper and print",
            Self::Language => "Language and spelling",
            Self::Reading => "Reading colours",
            Self::Identity => "Identity",
            Self::Signing => "Fill & Sign",
            Self::JavaScript => "JavaScript",
            Self::Editing => "Edit PDF",
            Self::Ocr => "Scan & OCR",
            Self::Pages => "Organize pages",
            Self::Security => "Security",
            Self::Assistant => "AI Assistant",
            Self::Updates => "Updates",
        }
    }

    fn navigation_label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Documents => "Documents",
            Self::Editing => "Editing",
            Self::Pages => "Pages",
            Self::Language => "Language",
            Self::Reading => "Accessibility",
            Self::Identity => "Comments",
            _ => self.label(),
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Appearance => "settings",
            Self::Documents => "files",
            Self::Paper => "file",
            Self::Language => "languages",
            Self::Reading => "eye",
            Self::Identity => "user-round",
            Self::Signing => "signature",
            Self::JavaScript => "file-code",
            Self::Editing => "pencil",
            Self::Ocr => "scan-text",
            Self::Pages => "files",
            Self::Security => "shield",
            Self::Assistant => "sparkles",
            Self::Updates => "cloud",
        }
    }
}

pub(crate) fn body(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) -> (bool, bool) {
    let mut connections = false;
    let mut close_requested = false;
    crate::widgets::dialog_heading(ui, tl!("Preferences"));
    ui.add_space(10.0);
    let height = (ui.ctx().content_rect().height() - 176.0).clamp(16.0, 380.0).min((ui.available_height() - 72.0).max(16.0));
    let width = ui.available_width();
    let rail = (width * 0.26).clamp(144.0, 180.0);
    let pane = (width - rail - 12.0).max(1.0);
    let padding = if height < 100.0 { 4 } else { 10 };
    let inside_height = (height - f32::from(padding) * 2.0).max(1.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.allocate_ui_with_layout(vec2(rail, height), Layout::top_down(Align::Min), |ui| {
            egui::Frame::new()
                .fill(if app.theme_preference == crate::theme::ThemePreference::Glass { t.panel } else { t.hover })
                .corner_radius(8)
                .inner_margin(egui::Margin::same(padding))
                .show(ui, |ui| {
                    ui.set_width((rail - f32::from(padding) * 2.0).max(1.0));
                    ui.spacing_mut().scroll.floating = false;
                    egui::ScrollArea::vertical()
                        .id_salt("preferences-categories")
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                        .max_height(inside_height)
                        .min_scrolled_height(inside_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for category in PreferencesCategory::ALL {
                                let selected = app.preferences_category == category;
                                let button = egui::Button::image_and_text(
                                    icons::image(category.icon(), 17.0, if selected { t.accent_text } else { t.icon }),
                                    tl!(category.navigation_label()),
                                )
                                .selected(selected)
                                .frame_when_inactive(selected)
                                .frame(true)
                                .min_size(vec2(ui.available_width(), 34.0))
                                .truncate();
                                if ui.add(button).on_hover_text(tl!(category.label())).clicked() {
                                    app.preferences_category = category;
                                }
                            }
                        });
                });
        });
        ui.allocate_ui_with_layout(vec2(pane, height), Layout::top_down(Align::Min), |ui| {
            egui::Frame::new().fill(t.card).stroke(egui::Stroke::new(1.0, t.border)).corner_radius(8).inner_margin(egui::Margin::same(padding)).show(
                ui,
                |ui| {
                    ui.set_width((pane - f32::from(padding) * 2.0).max(1.0));
                    ui.spacing_mut().scroll.floating = false;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    egui::ScrollArea::both()
                        .max_width(ui.available_width())
                        .min_scrolled_width(1.0)
                        .id_salt(("preferences-pane", app.preferences_category.key()))
                        .max_height(inside_height)
                        .min_scrolled_height(inside_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new(tl!(app.preferences_category.label())).font(theme::semibold(18.0)));
                            });
                            ui.separator();
                            ui.add_space(12.0);
                            match app.preferences_category {
                                PreferencesCategory::Appearance => appearance(ui, app, t),
                                PreferencesCategory::Documents => documents(ui, app, t),
                                PreferencesCategory::Paper => close_requested = crate::paper_ui::preferences(ui, app),
                                PreferencesCategory::Language => language(ui, app, t),
                                PreferencesCategory::Reading => reading(ui, app, t),
                                PreferencesCategory::Identity => identity(ui, app, t),
                                PreferencesCategory::Signing => signing(ui, app, t),
                                PreferencesCategory::JavaScript => javascript(ui, app, t),
                                PreferencesCategory::Editing => editing(ui, app, t),
                                PreferencesCategory::Ocr => crate::ocr_ui::preferences(ui, app),
                                PreferencesCategory::Pages => pages(ui, app),
                                PreferencesCategory::Security => security(ui, app),
                                PreferencesCategory::Assistant => connections = assistant(ui, app),
                                PreferencesCategory::Updates => crate::updates::preferences(ui, app),
                            }
                        });
                },
            );
        });
    });
    ui.add_space(8.0);
    ui.separator();
    let close = widgets::dialog_footer(ui, |ui| widgets::dialog_button(ui, tl!("OK"), widgets::DialogAction::Confirm).clicked()).inner;
    (close || close_requested, connections)
}

/// The label for a [`pdfcraft_render::ReadingColors`] mode name (`mode_name`).
fn reading_mode_label(mode: &str) -> &'static str {
    match mode {
        "black-on-white" => "Black on white",
        "white-on-black" => "White on black",
        "yellow-on-black" => "Yellow on black",
        "green-on-black" => "Green on black",
        "custom" => "Custom",
        _ => "Off",
    }
}

fn language(ui: &mut egui::Ui, app: &mut PdfCraftApp, _t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Interface language"));
        let shown = |language: crate::i18n::Lang| language.picker_name(|text| crate::js_ui::picker_text_is_drawable(ui, text));
        let selected = crate::i18n::Lang::from_code(&app.language).map_or(tl!("Auto"), shown);
        let before = app.language.clone();
        egui::ComboBox::from_id_salt("interface-language")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut app.language, crate::i18n::AUTO.to_string(), tl!("Auto")).clicked() {
                    ui.close();
                }
                for language in
                    crate::i18n::Lang::for_picker_by(|language| language.picker_name(|text| crate::js_ui::picker_text_is_drawable(ui, text)))
                {
                    let name = language.picker_name(|text| crate::js_ui::picker_text_is_drawable(ui, text));
                    if ui.selectable_value(&mut app.language, language.code().to_string(), name).clicked() {
                        ui.close();
                    }
                }
            })
            .response
            .accessible_name(tl!("Interface language"));
        // Relabel the rest of this dialog in the new language right away, not next frame.
        if app.language != before {
            crate::i18n::set_current(crate::i18n::Lang::from_pref(&app.language));
        }
    });
    crate::spelling_ui::preferences(ui, app);
}

fn appearance(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    ui.horizontal_wrapped(|ui| {
        use crate::ui_scale;
        ui.label(tl!("Interface size"));
        // Auto shows the size it draws at: the display's scale times the system's text scale.
        let native = ui.ctx().native_pixels_per_point().unwrap_or(1.0);
        let auto_percent = ui_scale::percent(ui_scale::drawn(None, app.system_text_scale, native));
        let auto = crate::i18n::fmt(tl!("Auto ({percent}%)"), &[("percent", &auto_percent.to_string())]);
        let selected = app.ui_scale.map_or_else(|| auto.clone(), |f| format!("{}%", ui_scale::percent(f)));
        egui::ComboBox::from_id_salt("interface-size")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if ui.selectable_value(&mut app.ui_scale, None, auto).clicked() {
                    ui.close();
                }
                for p in ui_scale::PRESETS {
                    let factor = f32::from(p) / 100.0;
                    if ui.selectable_label(app.ui_scale.is_some_and(|f| ui_scale::percent(f) == u32::from(p)), format!("{p}%")).clicked() {
                        app.ui_scale = Some(factor);
                        ui.close();
                    }
                }
            })
            .response
            .accessible_name(tl!("Interface size"));
    });
    ui.label(
        egui::RichText::new(tl!("Auto follows the system's text size. Pages always render at the display's full resolution."))
            .small()
            .color(t.text_muted),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Tools panel side"));
        let right = app.docking.tools_on_right();
        if ui.selectable_label(!right, tl!("Left")).clicked()
            && let Err(error) = app.set_option("tools-side", "left")
        {
            app.notify(error);
        }
        if ui.selectable_label(right, tl!("Right")).clicked()
            && let Err(error) = app.set_option("tools-side", "right")
        {
            app.notify(error);
        }
    });
    ui.add_space(8.0);
    ui.add(widgets::checkbox(&mut app.show_community, "Show community button in title bar"));
    crate::appearance::preferences_section(ui, app, t);
    ui.add_space(8.0);
    // Whether anything animates, the wheel's scrolling and zooming included; left alone it
    // follows the system's animation setting.
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Animation effects"));
        for choice in crate::motion::AnimationEffects::ALL {
            widgets::radio_value(ui, &mut app.animation_effects, choice, tl_ctx!("animation effects", choice.label()));
        }
    });
    // Only where the app draws the title bar (GNOME on Wayland); elsewhere the desktop draws it.
    if app.window_controls.is_some() {
        ui.add(widgets::checkbox(&mut app.system_title_bar, tl!("Use system title bar")))
            .on_hover_text(tl!("Show a basic title bar for the window buttons instead of the tab strip. GNOME doesn't draw title bars for apps, so this one doesn't follow its theme."));
    }
}

fn documents(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    widgets::section_title(ui, tl!("Opening documents"));
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Default workspace mode"));
        for (mode, label) in [
            (crate::Mode::AllTools, "All tools"),
            (crate::Mode::Read, "Read"),
            (crate::Mode::Edit, "Edit"),
            (crate::Mode::Convert, "Convert"),
            (crate::Mode::Sign, "E-Sign"),
        ] {
            widgets::radio_value(ui, &mut app.default_mode, mode, tl!(label));
        }
    });
    ui.label(
        egui::RichText::new(tl!("Used when opening PDFs. An explicit launch or control mode takes precedence for the session."))
            .small()
            .color(t.text_muted),
    );
    widgets::section_title(ui, tl!("Page display"));
    let defaults = &mut app.view_defaults;
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Default page display"));
        for l in crate::canvas::PageLayout::ORDER {
            widgets::radio_value(ui, &mut defaults.layout, l, tl!(l.label()));
        }
        ui.add(widgets::checkbox(&mut defaults.continuous, tl!("Continuous scrolling")));
    });
    ui.add(widgets::checkbox(&mut defaults.show_page_gaps, tl!("Show gaps between pages")));
    ui.label(egui::RichText::new(tl!("Hide gaps to join page edges. Margins inside the PDF remain unchanged.")).small().color(t.text_muted));
    ui.horizontal_wrapped(|ui| {
        use crate::canvas::Fit;
        ui.label(tl!("Default zoom"));
        widgets::radio_value(ui, &mut defaults.fit, Fit::Width, tl!("Fit to width"));
        widgets::radio_value(ui, &mut defaults.fit, Fit::Page, tl!("Zoom to page level"));
        widgets::radio_value(ui, &mut defaults.fit, Fit::None, tl!("Custom"));
        // Editing the percentage selects Custom.
        let mut percent = defaults.zoom * 100.0;
        if ui.add(egui::DragValue::new(&mut percent).range(8.0..=6400.0).max_decimals(0).suffix("%")).changed() {
            (defaults.fit, defaults.zoom) = (Fit::None, percent / 100.0);
        }
    });
    ui.label(
        egui::RichText::new(tl!("Used when a PDF doesn't ask for a layout or zoom. Continuous scrolling never snaps between pages."))
            .small()
            .color(t.text_muted),
    );
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Window resize behavior"));
        egui::ComboBox::from_id_salt("resize-zoom")
            .selected_text(tl!(defaults.resize_zoom.label()))
            .show_ui(ui, |ui| {
                for policy in crate::canvas::ResizeZoom::ALL {
                    ui.selectable_value(&mut defaults.resize_zoom, policy, tl!(policy.label()));
                }
            })
            .response
            .labelled_by(label.id);
    });
    ui.label(
        egui::RichText::new(tl!("Applies after manual zoom. Fit views always follow the window; resizing never changes the PDF itself."))
            .small()
            .color(t.text_muted),
    );
    ui.add(widgets::checkbox(&mut app.always_default_view, tl!("Always use my default zoom and page layout")));
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Units"));
        for u in crate::units::Unit::ALL {
            widgets::radio_value(ui, &mut app.units, u, u.label());
        }
    });
    ui.label(egui::RichText::new(tl!("For page sizes and margins.")).small().color(t.text_muted));
    widgets::section_title(ui, tl!("Files"));
    // The web build has no file paths to reopen.
    #[cfg(not(target_arch = "wasm32"))]
    ui.add(widgets::checkbox(&mut app.reopen_last_session, tl!("Reopen the files that were open when PdfCraft last closed")));
    // The web build has no files on disk to watch.
    #[cfg(not(target_arch = "wasm32"))]
    ui.add(widgets::checkbox(&mut app.reload_changed_files, tl!("Reload documents when their files change on disk")));
    // Viewer chrome only: validation and the Signatures panel are the same either way (#4221).
    ui.add(widgets::checkbox(&mut app.show_signature_badges, tl!("Show validation marks over signed signatures")));
    widgets::section_title(ui, tl!("Navigation"));
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Scroll speed (mouse wheel and arrow keys)"));
        for speed in crate::scroll_speed::ScrollSpeed::ALL {
            widgets::radio_value(ui, &mut app.scroll_speed, speed, tl_ctx!("scroll speed", speed.label()));
        }
    });
}

fn reading(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    // Reading colours (Acrobat: Replace Document Colors): solid text and line-art paints are
    // remapped while rendering, and the page clears to the background colour. The preference
    // is app-wide: every open document follows it, and documents open with it.
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Replace document colours"));
        let mode = app.reading_colors.mode_name();
        egui::ComboBox::from_id_salt("reading-colors")
            .selected_text(tl!(reading_mode_label(mode)))
            .show_ui(ui, |ui| {
                for (m, label) in [
                    ("off", "Off"),
                    ("black-on-white", "Black on white"),
                    ("white-on-black", "White on black"),
                    ("yellow-on-black", "Yellow on black"),
                    ("green-on-black", "Green on black"),
                    ("custom", "Custom"),
                ] {
                    if ui.selectable_label(mode == m, tl!(label)).clicked() {
                        match m {
                            "off" => app.set_reading_colors(pdfcraft_render::ReadingColors::default()),
                            "custom" => {
                                // Custom starts from the colours already chosen, or black on white.
                                let colors = app.reading_colors.params.map_or(pdfcraft_render::ReadingColors::BLACK_ON_WHITE, |params| {
                                    pdfcraft_render::ReadingColors { params: Some(params) }
                                });
                                app.set_reading_colors(colors);
                            }
                            // `parse` knows these names; a preset comes with its own flags.
                            preset => {
                                if let Ok(colors) = pdfcraft_render::ReadingColors::parse(preset, None, None, None, None) {
                                    app.set_reading_colors(colors);
                                }
                            }
                        }
                        ui.close();
                    }
                }
            })
            .response
            .accessible_name(tl!("Replace document colours"));
    });
    if app.reading_colors.params.is_some() {
        let mut changed = false;
        if let Some(params) = app.reading_colors.params.as_mut() {
            ui.horizontal_wrapped(|ui| {
                let text = ui.label(tl!("Text"));
                changed |= ui.color_edit_button_srgb(&mut params.text).labelled_by(text.id).changed();
                let background = ui.label(tl!("Page background"));
                changed |= ui.color_edit_button_srgb(&mut params.background).labelled_by(background.id).changed();
            });
            if ui.add(crate::widgets::checkbox(&mut params.only_black, tl!("Only change the colour of black text or line art"))).changed() {
                changed = true;
            }
            if ui.add(crate::widgets::checkbox(&mut params.line_art, tl!("Change the colour of line art as well as text"))).changed() {
                changed = true;
            }
        }
        if changed {
            app.set_reading_colors(app.reading_colors);
        }
    }
    ui.label(egui::RichText::new(tl!("Colours are remapped while rendering; the document itself never changes.")).small().color(t.text_muted));
}

fn identity(ui: &mut egui::Ui, app: &mut PdfCraftApp, _t: &Tokens) {
    // Identity: the author of new comments (Acrobat: Preferences ▸ Identity).
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Name on new comments"));
        ui.add(crate::widgets::line(&mut app.comment_prefs.author).desired_width(220.0).char_limit(crate::MAX_AUTHOR_CHARS)).labelled_by(label.id);
    });
    ui.add(widgets::checkbox(&mut app.comment_prefs.hide_empty_highlight_hover, tl!("Hide hover previews for highlights without comment text")));
}

fn signing(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    ui.add(widgets::checkbox(&mut app.flatten_fill_sign_on_save, tl!("Flatten Fill & Sign when saving")));
    ui.label(
        egui::RichText::new(tl!("Text, marks and signatures become part of the page. Other comments stay editable.")).small().color(t.text_muted),
    );
    ui.add_space(8.0);
    date_format(ui, app, t);
}

fn javascript(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    egui::Frame::new().fill(t.hover).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let mut on = app.session.javascript();
        if ui.add(widgets::checkbox(&mut on, tl!("Enable Acrobat JavaScript"))).changed() {
            app.session.set_javascript(on);
        }
        ui.label(
            egui::RichText::new(tl!("Scripts run in a sandbox without file or network access. A script that asks to open a web page needs your permission first. With JavaScript off, Acrobat's standard format, validate and calculate functions still work."))
                .small()
                .color(t.text_muted),
        );
    });
}

/// Preferences ▸ Date format: a preset, or any pattern typed in, for Fill & Sign dates, and the
/// language of its month and weekday names. A valid pattern applies as it's typed; an invalid
/// one stays in the box with the reason, and the last valid one stays in use.
fn date_format(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    use pdfcraft_engine::dates::{DATE_FORMATS, DATE_LANGUAGES, MAX_DATE_FORMAT_CHARS};
    ui.label(egui::RichText::new(tl!("Date format")).font(theme::semibold(13.0)));
    let sample = |app: &PdfCraftApp, f: &str| format!("{}   {f}", app.date_text(Some(f)).unwrap_or_default());
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Fill & Sign dates"));
        let current = app.session.date_format().to_string();
        let selected = if DATE_FORMATS.contains(&current.as_str()) { sample(app, &current) } else { tl!("Custom").to_string() };
        egui::ComboBox::from_id_salt("date-format")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                for f in DATE_FORMATS {
                    if ui.selectable_label(current == f, sample(app, f)).clicked() && app.session.set_date_format(f).is_ok() {
                        app.date_format_draft = None;
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label.id);
    });
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Pattern"));
        let mut text = app.date_format_draft.clone().unwrap_or_else(|| app.session.date_format().to_string());
        let edit = ui.add(egui::TextEdit::singleline(&mut text).desired_width(160.0).char_limit(MAX_DATE_FORMAT_CHARS)).labelled_by(label.id);
        // The box keeps the text as typed (a trailing space included) while it has focus; the
        // setting takes the trimmed pattern whenever it's valid.
        if edit.changed() {
            let _ = app.session.set_date_format(&text);
            app.date_format_draft = Some(text.clone());
        }
        if !edit.has_focus() && pdfcraft_engine::dates::check_date_format(&text).is_ok() {
            app.date_format_draft = None;
        }
        match app.date_text(Some(&text)) {
            Ok(today) => ui.label(egui::RichText::new(crate::i18n::fmt(tl!("Today: {date}"), &[("date", &today)])).color(t.text_muted)),
            // The engine's reason, in English.
            Err(e) => ui.label(egui::RichText::new(e).color(egui::Color32::from_rgb(0xD1, 0x3B, 0x3B))),
        };
    });
    // Fill & Sign refuses a date it would save as "?"; say so before anyone tries.
    let bad = app.date_text(None).map(|today| pdfcraft_engine::dates::unwritable(&today)).unwrap_or_default();
    if !bad.is_empty() {
        ui.label(
            egui::RichText::new(crate::i18n::fmt(
                tl!(
                    "{chars} can't be written into the PDF yet: Fill & Sign text is Western European only. Pick a numeric format such as dd.mm.yyyy."
                ),
                &[("chars", &bad)],
            ))
            .color(egui::Color32::from_rgb(0xD1, 0x3B, 0x3B)),
        );
    }
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Language"));
        let interface = crate::i18n::current().picker_name(|text| crate::js_ui::picker_text_is_drawable(ui, text));
        let follow = crate::i18n::fmt(tl!("Same as interface ({language})"), &[("language", interface)]);
        let current = app.session.date_language().map(str::to_string);
        let selected = DATE_LANGUAGES.iter().find(|l| current.as_deref() == Some(l.code)).map_or(follow.clone(), |l| {
            crate::js_ui::date_language_picker_name(l, |text| crate::js_ui::picker_text_is_drawable(ui, text)).to_string()
        });
        egui::ComboBox::from_id_salt("date-language")
            .selected_text(selected)
            .width(240.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current.is_none(), follow).clicked() && app.session.set_date_language(None).is_ok() {
                    ui.close();
                }
                for l in &DATE_LANGUAGES {
                    if ui
                        .selectable_label(
                            current.as_deref() == Some(l.code),
                            crate::js_ui::date_language_picker_name(l, |text| crate::js_ui::picker_text_is_drawable(ui, text)),
                        )
                        .clicked()
                        && app.session.set_date_language(Some(l.code)).is_ok()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label.id);
    });
    ui.label(
        egui::RichText::new(crate::i18n::fmt(
            tl!("yyyy or yy year · m or mm month · mmm or mmmm month name · d or dd day · ddd or dddd weekday. H, h, M, s and t are time letters and can't be used on their own: put {escape} before a letter to show it as is."),
            &[("escape", "\\")],
        ))
        .small()
        .color(t.text_muted),
    );
}

fn editing(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    ui.label(RichText::new("Style for newly added text in this session.").small().color(t.text_muted));
    ui.horizontal_wrapped(|ui| {
        let label = ui.label(tl!("Font size:"));
        ui.add(egui::DragValue::new(&mut app.text_style.size).range(1.0..=300.0).suffix(" pt")).labelled_by(label.id);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(tl!("Alignment"));
        for (value, label) in [
            (pdfcraft_engine::TextAlign::Left, "Left"),
            (pdfcraft_engine::TextAlign::Center, "Center"),
            (pdfcraft_engine::TextAlign::Right, "Right"),
            (pdfcraft_engine::TextAlign::Justify, "Justify"),
        ] {
            widgets::radio_value(ui, &mut app.text_style.align, value, tl!(label));
        }
    });
    ui.add(widgets::checkbox(&mut app.text_style.bold, tl!("Bold")));
    ui.add(widgets::checkbox(&mut app.text_style.italic, tl!("Italic")));
}

fn pages(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    ui.label("Preview sizes for the current document. Page contents are unchanged.");
    let index = app.active;
    ui.add_enabled_ui(index.is_some(), |ui| {
        if let Some(view) = index.and_then(|index| app.views.get_mut(index)) {
            let mut grid = view.grid_zoom();
            if ui.add(egui::Slider::new(&mut grid, crate::canvas::GRID_ZOOM_RANGE).text(tl!("Page grid"))).changed() {
                view.set_grid_zoom(grid);
            }
            let mut thumbs = view.thumbnail_zoom();
            if ui.add(egui::Slider::new(&mut thumbs, crate::canvas::THUMBNAIL_ZOOM_RANGE).text(tl!("Thumbnails"))).changed() {
                view.set_thumbnail_zoom(thumbs);
            }
        } else {
            ui.label("Open a PDF to adjust its page previews.");
        }
    });
}

fn security(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    ui.label("Trusted timestamp defaults. Choosing a service makes no network request.");
    crate::timestamp_ui::server_selector(ui, &mut app.timestamp_server);
    ui.add_space(12.0);
    ui.add(widgets::checkbox(&mut app.show_signature_badges, tl!("Show validation marks over signed signatures")));
    ui.add(widgets::checkbox(&mut app.show_secured_notice, tl!("Show a notice when a document is secured")));
}

fn assistant(ui: &mut egui::Ui, app: &mut PdfCraftApp) -> bool {
    ui.label("Connect your own account. PdfCraft includes no shared AI login or API key.");
    ui.add(widgets::checkbox(&mut app.assistant.settings.enabled, "Enable Assistant requests (off by default)"));
    let label = ui.label("Model ID");
    ui.add(widgets::line(&mut app.assistant.settings.model).char_limit(160).desired_width(260.0)).labelled_by(label.id);
    widgets::dialog_button(ui, "Manage AI connections", widgets::DialogAction::Back).clicked()
}
