//! Scan & OCR ▸ Recognize text in the real shell (egui_kittest): the dialog, the run and its
//! result (skipped without the OCR models: `cargo xtask models`).

use egui_kittest::Harness;

use egui_kittest::kittest::Queryable;

use pdfcraft_engine::{AddedText, Edit, Session, export};

use pdfcraft_ui_egui::PdfCraftApp;

/// A one-page PDF that is only a picture of a sentence.
fn scan() -> Vec<u8> {
    scan_pages(1)
}

fn scan_pages(count: usize) -> Vec<u8> {
    let mut s = Session::new();
    let text = s.create_from_text("t", "Recognize these scanned words").unwrap();
    let id = s.open("text.pdf", None, text, None).unwrap();
    let png = export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap();
    let images = (0..count).map(|i| (format!("scan-{i}.png"), png.clone())).collect::<Vec<_>>();
    s.create_from_images(&images).unwrap().to_vec()
}

fn scan_harness(bytes: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("scan.pdf", None, bytes).unwrap();
        app.run_inline = true;
        app
    })
}

#[test]
fn scan_text_prompt_waits_for_a_page_choice() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut h = scan_harness(scan());
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    h.get_by_label("Scanned text detected");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Current page");
    h.get_by_label("All pages");
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state().session.get(id).unwrap().text_blocks(0).is_empty());
    assert!(!h.state().session.get(id).unwrap().dirty);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none(), "cancel does not reopen the prompt");
    assert!(!h.state().session.get(id).unwrap().dirty);
}

#[test]
fn scan_text_prompt_respects_current_and_all_pages_and_undo() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    for all in [false, true] {
        let mut h = scan_harness(scan_pages(2));
        h.run_steps(4);
        h.state_mut().execute("edit.edit_text");
        h.run_steps(4);
        h.get_by_role_and_label(egui::accesskit::Role::Button, if all { "All pages" } else { "Current page" }).click();
        h.run_steps(4);
        let id = h.state().active_ids().unwrap().1;
        let doc = h.state().session.get(id).unwrap();
        assert!(!doc.text_blocks(0).is_empty());
        assert_eq!(!doc.text_blocks(1).is_empty(), all, "only the chosen pages are recognized");
        assert_eq!(doc.can_undo(), Some("Recognize text"));
        let saved = h.state_mut().session.save_bytes(id).unwrap();
        let mut reopened = Session::new();
        let copy = reopened.open("recognized.pdf", None, saved, None).unwrap();
        assert!(!reopened.get(copy).unwrap().text_blocks(0).is_empty());
        assert_eq!(!reopened.get(copy).unwrap().text_blocks(1).is_empty(), all);
        h.state_mut().execute("edit.undo");
        h.run_steps(4);
        assert!(h.state().dialog.is_none(), "undo does not immediately offer OCR again");
        let doc = h.state().session.get(id).unwrap();
        assert!(doc.text_blocks(0).is_empty() && doc.text_blocks(1).is_empty());
    }
}

#[test]
fn scan_text_toggle_is_persisted_and_reenabling_offers_the_prompt() {
    let mut app = PdfCraftApp::new();
    app.set_option("auto_ocr", "off").unwrap();
    let mut restored = PdfCraftApp::new();
    restored.restore(&app.persist());
    assert!(!restored.auto_ocr);
    assert!(restored.set_option("auto_ocr", "sometimes").is_err());
    assert!(!restored.auto_ocr);
    if !pdfcraft_engine::ocr::available() {
        return;
    }
    let mut h = scan_harness(scan());
    h.state_mut().set_option("auto_ocr", "off").unwrap();
    h.state_mut().execute("edit.edit_text");
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    h.get_by_label("Recognize scanned text automatically").click();
    h.run_steps(4);
    h.get_by_label("Scanned text detected");
}

#[test]
fn scan_text_prompt_refuses_a_document_changed_before_the_page_choice() {
    if !pdfcraft_engine::ocr::available() {
        return;
    }
    let mut h = scan_harness(scan());
    h.state_mut().execute("edit.edit_text");
    h.run_steps(4);
    let id = h.state().active_ids().unwrap().1;
    let note = AddedText { rect: [20.0, 20.0, 220.0, 60.0], text: "Keep this note".into(), ..Default::default() };
    h.state_mut().session.apply(id, Edit::AddText { page: 0, text: note }).unwrap();
    let generation = h.state().session.get(id).unwrap().edit_generation();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Current page").click();
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation);
    assert!(h.state().toast.as_ref().unwrap().0.contains("result was discarded"));
}

#[test]
fn scan_text_prompt_defers_for_dialogs_and_ignores_native_text_and_read_mode() {
    if !pdfcraft_engine::ocr::available() {
        return;
    }
    let mut h = scan_harness(scan());
    h.state_mut().set_option("dialog", "about").unwrap();
    h.state_mut().execute("edit.edit_text");
    h.run_steps(4);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::About));
    h.state_mut().dialog = None;
    h.run_steps(4);
    h.get_by_label("Scanned text detected");

    let native = Session::new().create_from_text("text", "This text is already searchable").unwrap();
    let mut h = scan_harness(native.to_vec());
    h.state_mut().execute("edit.edit_text");
    h.run_steps(4);
    assert!(h.state().dialog.is_none());

    let mut h = scan_harness(scan());
    let id = h.state().active_ids().unwrap().1;
    h.state_mut().execute("edit.edit_text");
    h.state_mut().set_option("mode", "read").unwrap();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert!(!h.state().session.get(id).unwrap().dirty);
}

#[test]
fn locked_scans_do_not_offer_edit_mode_ocr_until_unlocked() {
    if !pdfcraft_engine::ocr::available() {
        return;
    }
    let mut h = scan_harness(scan());
    h.state_mut().apply_edit(Edit::SetPageLocks { pages: vec![0], locked: true });
    h.state_mut().execute("edit.edit_text");
    h.run_steps(5);
    assert!(h.state().dialog.is_none(), "locked scans must not offer recognition before editing");
    h.state_mut().apply_edit(Edit::SetPageLocks { pages: vec![0], locked: false });
    h.run_steps(5);
    h.get_by_label("Scanned text detected");
}

#[test]
fn recognize_text_dialog_adds_searchable_text() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("scan.pdf", None, scan()).unwrap();
        app.run_inline = true;
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("ocr.recognize"));
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Document language");
    h.get_by_label("Output");
    h.get_by_label("Automatically rotate sideways and upside-down pages");
    h.get_by_label("Straighten skewed scans");
    assert!(h.state().ocr_draft.auto_rotate && h.state().ocr_draft.deskew);
    let expected = pdfcraft_engine::ocr::language_for_locale(sys_locale::get_locale().as_deref());
    assert_eq!(h.state().ocr_draft.language, expected, "OCR follows the system even when the interface is English");
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Document language").click();
    h.run_steps(2);
    for label in ["Nederlands", "Dansk", "Norsk bokmål", "Svenska", "Suomi", "Português", "Català"] {
        h.get_by_label(label);
    }
    let traditional = if h.query_by_label("繁體中文").is_some() { "繁體中文" } else { "Traditional Chinese" };
    let simplified = if h.query_by_label("简体中文").is_some() { "简体中文" } else { "Simplified Chinese" };
    h.get_by_label(traditional);
    h.get_by_label(simplified);
    // The expanded language list scrolls when the popup reaches the window edge.
    h.get_by_label(simplified).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(simplified).click();
    h.run_steps(2);
    assert_eq!(h.state().ocr_draft.language, "zh-Hans", "the last language in the list can be chosen");
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Document language").click();
    h.run_steps(2);
    h.get_by_label("Català").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Català").click();
    h.run_steps(2);
    assert_eq!(h.state().ocr_draft.language, "ca");
    h.get_by_value("Català").click();
    h.run_steps(2);
    h.get_by_label("Italiano").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Italiano").click();
    h.run_steps(2);
    assert_eq!(h.state().ocr_draft.language, "it");
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    h.get_by_label("Recognize text").click();
    h.run_steps(3);
    assert!(h.state().ocr_run.is_none(), "finished");
    let toast = h.state().toast.clone().unwrap().0;
    assert!(toast.starts_with("Recognized ") && toast.contains("on 1 page"), "{toast}");
    let app = h.state();
    let id = app.active_ids().unwrap().1;
    assert_eq!(app.session.get(id).unwrap().can_undo(), Some("Recognize text"));
}

#[test]
fn scan_settings_preserve_corrections_and_existing_text_choice() {
    let mut draft = PdfCraftApp::new().ocr_draft;
    assert!(draft.settings().skip_text_pages);
    draft.text_pages = true;
    draft.language = "fr".into();
    draft.auto_rotate = false;
    draft.deskew = false;
    draft.auto_crop = true;
    draft.crop_margin = 7.0;
    let settings = draft.settings();
    assert!(!settings.skip_text_pages);
    assert_eq!(settings.language, "fr");
    assert!(!settings.auto_rotate && !settings.deskew);
    assert!(settings.auto_crop);
    assert_eq!(settings.crop_margin, 7.0);
    draft.text_pages = false;
    draft.recognize = false;
    assert!(!draft.settings().skip_text_pages, "enhancement retains existing OCR pages");
    assert!(!draft.settings().recognize);
}

#[test]
fn enhance_scan_dialog_offers_correction_without_recognition() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("scan.pdf", None, scan()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("ocr.enhance"));
    h.run_steps(2);
    h.get_by_label("Correct scans");
    assert!(!h.state().ocr_draft.recognize);
    assert!(h.state().ocr_draft.auto_crop);
    assert_eq!(h.state().ocr_draft.language, pdfcraft_engine::ocr::language_for_locale(sys_locale::get_locale().as_deref()));
    h.get_by_label("Safety margin (pt)");
    h.get_by_label("Crop scanner borders to the paper edge").click();
    h.run_steps(2);
    assert!(!h.state().ocr_draft.auto_crop);
    h.get_by_label("Add searchable text").click();
    h.run_steps(2);
    assert!(h.state().ocr_draft.recognize);
}

#[test]
fn recognize_text_in_multiple_files_writes_searchable_copies() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let dir = std::env::temp_dir().join(format!("pdfcraft-ocr-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.run_inline = true;
    app.export_dir_override = Some(dir.to_string_lossy().into_owned());
    app.use_files(pdfcraft_ui_egui::FilePurpose::Ocr, vec![("one.pdf".into(), scan()), ("two.pdf".into(), scan())]);
    assert!(app.ocr_batch.is_none(), "finished");
    let toast = app.toast.clone().unwrap().0;
    assert!(toast.starts_with("Recognized ") && toast.ends_with("in 2 files"), "{toast}");
    for name in ["one.pdf", "two.pdf"] {
        let bytes = std::fs::read(dir.join(name)).unwrap();
        assert!(bytes.len() > scan().len(), "an incremental update was appended");
    }
}

/// #1028: "Also recognize pages that already have text" reads a picture on a page that has its own
/// text, without adding that text a second time.
#[test]
fn recognize_text_can_read_pictures_on_pages_with_text() {
    let hybrid = {
        let mut s = Session::new();
        let id = s.open("scan.pdf", None, std::sync::Arc::new(scan()), None).unwrap();
        let (w, h) = {
            let p = &s.get(id).unwrap().info.pages[0];
            (p.width as f64, p.height as f64)
        };
        let note =
            AddedText { rect: [w * 0.1, h * 0.75, w * 0.9, h * 0.85], text: "Invoice reference 4711".into(), size: 18.0, ..Default::default() };
        s.apply(id, Edit::AddText { page: 0, text: note }).unwrap();
        s.save_bytes(id).unwrap().to_vec()
    };
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("hybrid.pdf", None, hybrid).unwrap();
        app.run_inline = true;
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("ocr.recognize"));
    h.run_steps(2);
    assert!(h.state().ocr_draft.settings().skip_text_pages, "pages with text are skipped by default");
    h.get_by_label("Also recognize pages that already have text").click();
    h.run_steps(2);
    assert!(!h.state().ocr_draft.settings().skip_text_pages);
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    h.get_by_label("Recognize text").click();
    h.run_steps(3);
    assert!(h.state().ocr_run.is_none(), "finished");
    let toast = h.state().toast.clone().unwrap().0;
    assert!(toast.starts_with("Recognized ") && toast.ends_with("on 1 page"), "{toast}");
    let app = h.state();
    let id = app.active_ids().unwrap().1;
    let doc = app.session.get(id).unwrap();
    assert_eq!(doc.can_undo(), Some("Recognize text"));
    let mut r = pdfcraft_render::PageRenderer::new(doc.bytes.clone(), Default::default());
    let text = r.render(pdfcraft_render::RenderRequest { page: 0, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() });
    let text = text.text.unwrap().plain_text().to_lowercase();
    assert_eq!((text.matches("invoice").count(), text.matches("scanned").count()), (1, 1), "{text}");
}

/// Edit text & images on a scanned page: an OCR word (invisible text over the scan) opens for
/// editing on a click, stays open when the pointer leaves it, and its editor hides the scanned
/// word instead of showing it through.
#[test]
fn an_ocr_word_opens_for_editing_and_stays_open() {
    use pdfcraft_engine::ocr::PlacedWord;
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("scan.pdf", None, scan()).unwrap();
        let id = app.views[0].id;
        let ph = app.session.get(id).unwrap().info.pages[0].height as f64;
        let words = vec![PlacedWord { text: "Recognize".into(), origin: [72.0, ph - 100.0], across: [80.0, 0.0], up: [0.0, 14.0] }];
        app.session.apply(id, Edit::AddOcrText { page: 0, words }).unwrap();
        app.set_option("zoom", "100").unwrap();
        assert!(app.execute("edit.edit_text"));
        app
    });
    crate::support::settle(&mut h);
    let (r, k) = {
        let app = h.state();
        let d = app.session.get(app.views[0].id).unwrap();
        assert!(d.text_blocks(0).first().is_some_and(|b| b.invisible));
        let r = app.views[0].page_screen_rect(0).unwrap();
        (r, r.width() / d.info.pages[0].width)
    };
    // The word, 100 pt from the top of the page.
    let p = r.min + egui::vec2(100.0, 93.0) * k;
    h.hover_at(p);
    h.run_steps(2);
    for pressed in [true, false] {
        h.event(egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
    }
    h.run_steps(4);
    let opened = |h: &Harness<'_, PdfCraftApp>| h.state().views[0].line_editor.as_ref().map(|e| e.text.clone());
    assert_eq!(opened(&h).as_deref(), Some("Recognize"));
    assert!(h.state().views[0].image_selection.is_none(), "the scan under the word isn't selected");
    h.hover_at(r.min + egui::vec2(300.0, 400.0) * k);
    h.run_steps(4);
    assert_eq!(opened(&h).as_deref(), Some("Recognize"), "moving the pointer away keeps the word open");
}

#[test]
fn ocr_preferences_survive_restart_and_reject_damaged_values() {
    let mut app = PdfCraftApp::new();
    app.ocr_draft.language = "fr".into();
    app.ocr_draft.dpi = 600;
    app.ocr_draft.text_pages = true;
    let mut restored = PdfCraftApp::new();
    restored.restore(&app.persist());
    assert_eq!(restored.ocr_draft.settings(), app.ocr_draft.settings());
    restored.restore(r#"{"ocr":{"language":"invalid","dpi":18446744073709551615,"text_pages":"yes"}}"#);
    assert_eq!(restored.ocr_draft.settings(), app.ocr_draft.settings());
}

/// A page of real text, not a picture.
fn text_page() -> Vec<u8> {
    let s = Session::new();
    s.create_from_text("t", "This page already has its own text").unwrap().to_vec()
}

fn edit_harness(pdf: Vec<u8>) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, pdf).unwrap();
        app.run_inline = true;
        app
    });
    h.run_steps(4);
    h
}

fn accept_current_scan(h: &mut Harness<'static, PdfCraftApp>) {
    assert!(matches!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::RecognizeScan { .. })), "recognition waits for the user's page choice");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Current page").click();
    h.run_steps(4);
}

/// Like Acrobat's "Converting scanned page to Editable Text and Images": turning Edit on for a
/// scanned page offers a page choice, then recognizes its words in one undoable step, once. Undoing it
/// is not undone again by the next frame.
#[test]
fn editing_a_scanned_page_recognizes_its_text_first_and_only_once() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut h = edit_harness(scan());
    let id = h.state().active_ids().unwrap().1;
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None, "nothing is recognized before Edit");
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    accept_current_scan(&mut h);
    assert!(h.state().ocr_run.is_none(), "finished");
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Recognize text"));
    // Undo takes the text layer away and Edit does not put it back.
    h.state_mut().undo();
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None);
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None, "recognized only once per page");
}

/// The Edit tab (its tool panel) turns Edit on without picking Edit text & images, and Acrobat's
/// conversion happens there too.
#[test]
fn the_edit_panel_recognizes_a_scanned_page_too() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut h = edit_harness(scan());
    let id = h.state().active_ids().unwrap().1;
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None);
    h.state_mut().set_option("tool", "edit").unwrap();
    h.run_steps(4);
    accept_current_scan(&mut h);
    assert!(h.state().ocr_run.is_none(), "finished");
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Recognize text"));
}

/// A scan with only a comment (a text box) over it is still a scan: Edit reads the picture. The
/// comment's own words are not read into the layer, and it counts as no page text.
#[test]
fn editing_a_scan_with_a_comment_over_it_recognizes_the_picture() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let with_comment = {
        let mut s = Session::new();
        let id = s.open("scan.pdf", None, std::sync::Arc::new(scan()), None).unwrap();
        let (w, h) = {
            let p = &s.get(id).unwrap().info.pages[0];
            (p.width as f64, p.height as f64)
        };
        let shape = pdfcraft_engine::Shape::TextBox { rect: [w * 0.1, h * 0.75, w * 0.9, h * 0.85], font_size: 18.0 };
        let style = pdfcraft_engine::Style::default_for(&shape);
        let note = pdfcraft_engine::NewAnnotation { page: 0, shape, style, contents: "Invoice reference 4711".into(), author: "Test".into() };
        s.apply(id, Edit::AddAnnotation(note)).unwrap();
        s.save_bytes(id).unwrap().to_vec()
    };
    let mut h = edit_harness(with_comment);
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    accept_current_scan(&mut h);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Recognize text"));
}

/// A born-digital page with a full-page background picture and real text (a flyer, a slide
/// export) must not change just because Edit is turned on (maintainer review of #1609).
#[test]
fn editing_a_page_with_text_over_a_background_picture_changes_nothing() {
    let hybrid = {
        let mut s = Session::new();
        let id = s.open("scan.pdf", None, std::sync::Arc::new(scan()), None).unwrap();
        let (w, h) = {
            let p = &s.get(id).unwrap().info.pages[0];
            (p.width as f64, p.height as f64)
        };
        let note =
            AddedText { rect: [w * 0.1, h * 0.75, w * 0.9, h * 0.85], text: "Invoice reference 4711".into(), size: 18.0, ..Default::default() };
        s.apply(id, Edit::AddText { page: 0, text: note }).unwrap();
        s.save_bytes(id).unwrap().to_vec()
    };
    let mut h = edit_harness(hybrid);
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(6);
    assert!(h.state().ocr_run.is_none());
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None, "nothing was recognized or marked modified");
    assert!(!h.state().session.get(id).unwrap().dirty, "the file is not marked modified");
    // And Edit says nothing about recognizing: no progress, no "0 words ... already had text".
    let toast = h.state().toast.as_ref().map(|t| t.0.clone()).unwrap_or_default();
    assert!(!toast.contains("ecogniz") && !toast.contains("already had text"), "{toast:?}");
}

/// A page that is not a scan when Edit looks at it is looked at again after the document changes:
/// adding a picture of text that covers it turns it into a scan, and Edit then reads it (and does
/// so once).
#[test]
fn a_page_that_becomes_a_scan_while_edit_is_on_is_recognized() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let png = {
        let mut s = Session::new();
        let text = s.create_from_text("t", "Recognize these scanned words").unwrap();
        let id = s.open("text.pdf", None, text, None).unwrap();
        export::Exporter::new(s.get(id).unwrap()).png(0, 150.0).unwrap()
    };
    let blank = Session::new().create_blank(595.0, 842.0, 1).unwrap().to_vec();
    let mut h = edit_harness(blank);
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(4);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None, "a blank page is no scan: nothing happened");
    // The same page, now with a picture of text over all of it.
    h.state_mut()
        .session
        .apply(id, Edit::AddImage { page: 0, rect: Some([0.0, 0.0, 595.0, 842.0]), name: "scan.png".into(), bytes: std::sync::Arc::new(png) })
        .unwrap();
    h.run_steps(6);
    accept_current_scan(&mut h);
    assert!(h.state().ocr_run.is_none(), "finished");
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Recognize text"));
}

/// Edit changes the document only while the user is editing: not in Read mode (which leaves the Edit
/// tool picked), and not beneath a dialog. The page is recognized once those are gone.
#[test]
fn edit_recognizes_a_scan_only_outside_read_mode_and_beneath_no_dialog() {
    if !pdfcraft_engine::ocr::available() {
        eprintln!("skipped: OCR models not installed");
        return;
    }
    let mut h = edit_harness(scan());
    let id = h.state().active_ids().unwrap().1;
    let undo = |h: &Harness<'static, PdfCraftApp>| -> Option<String> { h.state().session.get(id).unwrap().can_undo().map(String::from) };
    // Read mode with the Edit tool still picked.
    assert!(h.state_mut().execute("edit.edit_text"));
    h.state_mut().set_option("mode", "read").unwrap();
    h.run_steps(5);
    assert_eq!(undo(&h), None, "not in Read mode");
    // A dialog over the window.
    h.state_mut().set_option("mode", "edit").unwrap();
    h.state_mut().dialog = Some(pdfcraft_ui_egui::Dialog::JsConsole);
    h.run_steps(5);
    assert_eq!(undo(&h), None, "not beneath a dialog");
    // Both gone: it is read now.
    h.state_mut().dialog = None;
    h.run_steps(5);
    accept_current_scan(&mut h);
    assert_eq!(undo(&h).as_deref(), Some("Recognize text"));
}

#[test]
fn editing_a_page_that_has_text_does_not_recognize_anything() {
    let mut h = edit_harness(text_page());
    let id = h.state().active_ids().unwrap().1;
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(6);
    assert!(h.state().ocr_run.is_none());
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None);
}

/// Only the Edit tool recognizes: viewing a scan, or any other tool, leaves the file as it is.
#[test]
fn only_the_edit_tool_recognizes_a_scanned_page() {
    let mut h = edit_harness(scan());
    let id = h.state().active_ids().unwrap().1;
    h.run_steps(6);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), None);
}

#[test]
fn recognized_text_correction_keeps_scan_pixels() {
    let mut h = scan_harness(scan());
    h.run_steps(3);
    let id = h.state().views[0].id;
    let word = pdfcraft_engine::ocr::PlacedWord { text: "Mistkae".into(), origin: [25.0, 40.0], across: [80.0, 0.0], up: [0.0, 12.0] };
    h.state_mut().apply_edit(Edit::AddOcrText { page: 0, words: vec![word] });
    let before = export::Exporter::new(h.state().session.get(id).unwrap()).png(0, 144.0).unwrap();
    assert!(h.state_mut().execute("ocr.correct"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::CorrectOcrText));
    h.get_by_label("Corrected text");
    if let Ok(dir) = std::env::var("PDFCRAFT_FEATURE_SHOTS") {
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(std::path::Path::new(&dir).join("correct-recognized-text.png")).unwrap();
    }
    h.state_mut().ocr_correction.text = "Mistake".into();
    h.run_steps(2);
    h.get_by_label("Apply").click();
    h.run_steps(3);
    let doc = h.state().session.get(id).unwrap();
    assert!(doc.text_blocks(0).iter().any(|b| b.invisible && b.text == "Mistake"));
    assert_eq!(export::Exporter::new(doc).png(0, 144.0).unwrap(), before);
    assert_eq!(doc.can_undo(), Some("Correct recognized text"));
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert_eq!(h.state().dialog, None);
}

#[test]
fn enhance_scan_dialog_corrects_without_adding_searchable_text() {
    if !pdfcraft_engine::ocr::available() {
        return;
    }
    let mut h = scan_harness(scan());
    h.run_steps(3);
    let id = h.state().views[0].id;
    let generation = h.state().session.get(id).unwrap().edit_generation();
    assert!(h.state_mut().execute("ocr.enhance"));
    h.run_steps(3);
    h.get_by_label("Enhance scanned file");
    assert!(!h.state().ocr_draft.recognize);
    if let Ok(dir) = std::env::var("PDFCRAFT_FEATURE_SHOTS") {
        std::fs::create_dir_all(&dir).unwrap();
        h.render().unwrap().save(std::path::Path::new(&dir).join("enhance-scanned-file.png")).unwrap();
    }
    h.state_mut().ocr_draft.auto_rotate = false;
    h.state_mut().ocr_draft.deskew = false;
    h.state_mut().ocr_draft.auto_crop = false;
    h.get_by_label("Correct scans").click();
    h.run_steps(3);
    assert!(h.state().ocr_run.is_none());
    assert!(h.state().session.get(id).unwrap().text_blocks(0).is_empty());
    assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "no corrections means no edit");
    assert!(h.state().toast.as_ref().unwrap().0.starts_with("Enhanced "));
}
