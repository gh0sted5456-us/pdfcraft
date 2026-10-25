//! Create a PDF and Reduce File Size in the real shell.

use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn physical_paper_defaults_roundtrip_and_create_print_sized_cards() {
    let mut app = PdfCraftApp::new();
    app.set_option("default-paper", "European business card").unwrap();
    app.set_option("actual-size-zoom", "1.25").unwrap();
    let saved = app.persist();
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert!(restored.execute("create.blank"));
    let p = &restored.session.docs()[0].info.pages[0];
    assert!((p.width - 85.0 * 72.0 / 25.4).abs() < 0.001);
    assert!((p.height - 55.0 * 72.0 / 25.4).abs() < 0.001);
    assert_eq!(serde_json::from_str::<serde_json::Value>(&restored.persist()).unwrap()["actual_size_zoom"], 1.25);
    assert!(restored.set_option("default-page-width", "NaN").is_err());
    assert!(restored.set_option("default-page-height", "0").is_err());
    assert!(restored.set_option("actual-size-zoom", "100").is_err());
    restored.set_option("default-page-width", "144").unwrap();
    restored.set_option("default-page-height", "252").unwrap();
    assert!(restored.execute("create.blank"));
    let p = &restored.session.docs()[1].info.pages[0];
    assert_eq!((p.width, p.height), (144.0, 252.0));
}

#[test]
fn paper_settings_calibrate_view_without_resizing_the_pdf() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 800.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.execute("create.blank");
        app.set_option("preferences-category", "paper").unwrap();
        app.set_option("dialog", "preferences").unwrap();
        app.measured_ruler_mm = 40.0;
        app
    });
    h.run_steps(3);
    let original = h.state().session.docs()[0].bytes.clone();
    h.get_by_label("Calibrate").scroll_to_me();
    h.run_steps(24);
    h.get_by_label("Calibrate").click();
    h.run_steps(2);
    h.get_by_label("Show actual size").scroll_to_me();
    h.run_steps(24);
    h.get_by_label("Show actual size").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().views[0].zoom, 1.25);
    h.state_mut().views[0].set_zoom(0.8);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Num1);
    h.run_steps(3);
    assert_eq!(h.state().views[0].zoom, 1.25, "the normal Actual size shortcut uses calibration too");
    assert_eq!(h.state().session.docs()[0].bytes, original);
}

#[test]
fn custom_page_dialog_creates_the_requested_dimensions() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("default-paper", "US business card").unwrap();
        assert!(app.execute("create.custom"));
        app
    });
    h.run_steps(3);
    if let Ok(dir) = std::env::var("PDFCRAFT_PAPER_SHOTS") {
        let _gpu = crate::gpu();
        h.render().unwrap().save(format!("{dir}/custom-page.png")).unwrap();
    }
    h.get_by_label("Swap width and height").click();
    h.run_steps(2);
    h.get_by_label("Create").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    let p = &h.state().session.docs()[0].info.pages[0];
    assert_eq!((p.width, p.height), (144.0, 252.0));
}

#[test]
fn export_to_opens_a_format_picker() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut h = Harness::builder().with_size(egui::vec2(900.0, 700.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.execute("create.blank");
        assert!(app.execute("export.choose"));
        app
    });
    h.run_steps(3);
    if let Ok(dir) = std::env::var("PDFCRAFT_PAPER_SHOTS") {
        let _gpu = crate::gpu();
        h.render().unwrap().save(format!("{dir}/export-to.png")).unwrap();
    }
    h.get_by_label("WordCraft / Microsoft Word (.docx)");
    h.get_by_label("PowerPoint (.pptx)");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().session.docs().len(), 1);
}

#[test]
fn office_export_preserves_open_original_and_exports_docx_and_pptx() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-export-picker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.pdf");
    let mut app = PdfCraftApp::new();
    let bytes = app.session.create_from_text("Example", "Editable export example").unwrap();
    std::fs::write(&source, bytes.as_ref()).unwrap();
    app.open_bytes("source.pdf", Some(source.to_string_lossy().into_owned()), bytes.as_ref().clone()).unwrap();
    app.save_override = Some(source.to_string_lossy().into_owned());
    app.execute("export.docx");
    assert_eq!(std::fs::read(&source).unwrap(), bytes.as_ref().clone(), "an export cannot replace an open PDF");
    for ext in ["docx", "pptx"] {
        let out = dir.join(format!("output.{ext}"));
        app.save_override = Some(out.to_string_lossy().into_owned());
        assert!(app.execute(&format!("export.{ext}")));
        let exported = std::fs::read(&out).unwrap();
        assert!(exported.starts_with(b"PK"));
        let entry = if ext == "docx" { b"word/document.xml".as_slice() } else { b"ppt/presentation.xml".as_slice() };
        assert!(exported.windows(entry.len()).any(|w| w == entry));
    }
    std::fs::remove_dir_all(dir).unwrap();
}

fn png() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 8, 4);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[120u8; 8 * 4 * 3]).unwrap();
    }
    out
}

#[test]
fn image_import_dialog_chooses_dpi_and_cancels() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.begin_image_import(vec![("scan.png".into(), png())]);
        app
    });
    h.run_steps(3);
    if let Ok(path) = std::env::var("PDFCRAFT_IMAGE_IMPORT_SHOT") {
        let _gpu = crate::gpu();
        h.render().unwrap().save(path).unwrap();
    }
    h.get_by_label("Use 72 DPI (one point per pixel)").click();
    h.run_steps(1);
    h.get_by_label("Create").click();
    h.run_steps(3);
    assert!(h.state().image_import.is_none());
    let page = &h.state().session.docs()[0].info.pages[0];
    assert_eq!((page.width, page.height), (8.0, 4.0));
    h.state_mut().begin_image_import(vec![("scan.png".into(), png())]);
    h.state_mut().image_import.as_mut().unwrap().dpi = 144.0;
    h.run_steps(2);
    h.get_by_label("Use custom DPI").click();
    h.run_steps(1);
    h.get_by_label("Create").click();
    h.run_steps(3);
    let page = &h.state().session.docs()[1].info.pages[0];
    assert_eq!((page.width, page.height), (4.0, 2.0));
    h.state_mut().begin_image_import(vec![("scan.png".into(), png())]);
    h.run_steps(2);
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state().image_import.is_none());
    assert_eq!(h.state().session.docs().len(), 2);
}

#[test]
fn opening_images_and_text_converts_them_to_new_pdfs() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.open_bytes("photo.png", Some("/tmp/photo.png".into()), png()).unwrap();
    app.open_bytes("notes.txt", None, b"first line\nsecond line".to_vec()).unwrap();
    app.create_from_images(vec![("a.png".into(), png()), ("b.png".into(), png())]);
    assert!(app.execute("create.blank"));
    let docs = app.session.docs();
    let summary: Vec<(String, usize, bool)> = docs.iter().map(|d| (d.name.clone(), d.info.pages.len(), d.dirty)).collect();
    assert_eq!(
        summary,
        [("photo.pdf".to_string(), 1, true), ("notes.pdf".into(), 1, true), ("Images.pdf".into(), 2, true), ("Untitled.pdf".into(), 1, true)]
    );
    assert!(docs[0].path.is_none(), "a converted file has no PDF path yet: Save asks where");
    assert!(app.open_bytes("junk.png", None, b"\x89PNG\r\n\x1a\nnot really".to_vec()).is_err());
}

struct ReduceDir(std::path::PathBuf);
impl ReduceDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let path = std::env::temp_dir().join(format!("pdfcraft-reduce-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("creating Reduce test directory: {error}"),
            }
        }
        panic!("no unused Reduce test directory after 128 attempts");
    }
}
impl Drop for ReduceDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Open `name` and run Reduce File Size on it, saving to `out` without a dialog.
fn reduce(name: &'static str, bytes: Vec<u8>, out: &std::path::Path) -> egui_kittest::Harness<'static, PdfCraftApp> {
    let out2 = out.to_path_buf();
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        // Keep metadata identical: changing ModDate can make a compact PDF one byte smaller.
        app.session = std::mem::take(&mut app.session).with_clock(|| 1_700_000_000);
        app.open_bytes(name, None, bytes).unwrap();
        app.save_override = Some(out2.to_string_lossy().into_owned());
        app
    });
    h.run_steps(2);
    assert!(h.state_mut().execute("optimize.reduce"));
    // It runs on a worker thread like the PDF Optimizer, with the progress card meanwhile.
    let start = std::time::Instant::now();
    while h.state().optimize_run.is_some() && start.elapsed() < std::time::Duration::from_secs(30) {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.run_steps(2);
    assert!(h.state().optimize_run.is_none() && h.state().progress_notice.is_none(), "the run finished");
    h
}

#[test]
fn reduce_file_size_writes_a_compact_copy() {
    use egui_kittest::kittest::Queryable;
    // A photo-like picture, stored losslessly: Reduce recompresses it as JPEG.
    let (w, h) = (600u32, 400u32);
    let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 255 / w) as u8 ^ (i & 7) as u8, (i / w * 255 / h) as u8, 128]).collect();
    let mut photo = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut photo, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&px).unwrap();
    }
    let dir = ReduceDir::new();
    let out = dir.0.join("reduced.pdf");
    let h = reduce("photo.png", photo, &out);
    let bytes = std::fs::read(&out).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() < h.state().session.docs()[0].bytes.len(), "the copy is smaller");
    assert!(h.state().session.docs()[0].dirty, "the open document is unchanged");
    h.get_by_label_contains("% smaller");
}

#[test]
fn reduce_file_size_saves_nothing_when_the_file_is_already_small() {
    use egui_kittest::kittest::Queryable;
    let dir = ReduceDir::new();
    let out = dir.0.join("reduced.pdf");
    // A new text PDF is already compact: a copy would be no smaller (#490).
    let h = reduce("notes.txt", "lorem ipsum ".repeat(500).into_bytes(), &out);
    assert!(!out.exists(), "no copy was written");
    h.get_by_label_contains("This file is already as small as it can be made");
}

#[test]
fn clipboard_images_and_text_become_new_pdfs() {
    use pdfcraft_ui_egui::Clip;
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.create_from_clip(Clip::Image { width: 40, height: 20, rgba: [10u8, 20, 30, 255].repeat(40 * 20) }).unwrap();
    app.create_from_clip(Clip::Text("Pasted\nlines".into())).unwrap();
    let docs = app.session.docs();
    assert_eq!(docs.len(), 2);
    assert_eq!(docs[0].name, "Clipboard.pdf");
    // The image page takes the image's size (one pixel per point at 72 dpi).
    let p = &docs[0].info.pages[0];
    assert!((p.width / p.height - 2.0).abs() < 0.01, "{} × {}", p.width, p.height);
    assert!(app.create_from_clip(Clip::Image { width: 4, height: 4, rgba: vec![0; 3] }).is_err(), "malformed images are refused");
}

#[test]
fn the_pdf_optimizer_dialog_saves_an_optimized_copy() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    let dir = std::env::temp_dir().join(format!("pdfcraft-optimizer-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("optimized.pdf");
    let out2 = out.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("notes.txt", None, "lorem ipsum ".repeat(500).into_bytes()).unwrap();
        app.save_override = Some(out2.to_string_lossy().into_owned());
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("optimize.advanced"));
    h.run_steps(2);
    h.get_by_label("PDF Optimizer");
    h.get_by_label("Color Images");
    h.get_by_label("Discard Objects").click();
    h.run_steps(2);
    h.get_by_label("Discard document tags").click();
    h.get_by_label("Discard User Data").click();
    h.run_steps(2);
    h.get_by_label("Discard document information and metadata").click();
    h.run_steps(1);
    {
        let d = &h.state().optimize_draft;
        assert!(d.settings.discard_tags && d.discard == vec![pdfcraft_engine::Hidden::Metadata]);
    }
    h.get_by_label("OK").click();
    // The click is handled next frame. The optimization then runs on a worker thread and the
    // copy is saved when it is done.
    h.run_steps(1);
    let start = std::time::Instant::now();
    while (h.state().optimize_run.is_some() || !out.exists()) && start.elapsed() < std::time::Duration::from_secs(30) {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    h.run_steps(2);
    assert!(h.state().progress_notice.is_none(), "the progress card is gone");
    assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF-"));
    assert_eq!(h.state().dialog, None);
}

#[test]
fn the_pdf_optimizer_dialog_picks_a_preset() {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;
    use pdfcraft_engine::optimize::Preset;
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("notes.txt", None, b"lorem ipsum".to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("optimize.advanced"));
    h.run_steps(2);
    assert_eq!(Preset::of(&h.state().optimize_draft.settings), Some(Preset::Standard));
    h.get_by_label("Merge image fragments");
    h.get_by_value("Standard").click();
    h.run_steps(4);
    h.get_by_label("Mobile").click();
    h.run_steps(2);
    assert_eq!(h.state().optimize_draft.settings, Preset::Mobile.settings(), "Mobile chosen");
    // Changing an option makes the settings custom.
    h.get_by_label("Merge image fragments").click();
    h.run_steps(2);
    assert!(!h.state().optimize_draft.settings.merge_fragments);
    h.get_by_value("Custom");
}

fn mixed_files(app: &PdfCraftApp) -> Vec<(String, Vec<u8>)> {
    let pdf = app.session.create_from_text("a", "from a pdf").unwrap();
    vec![
        ("notes.txt".into(), b"from text".to_vec()),
        ("report.docx".into(), b"PK\x03\x04".to_vec()),
        ("a.pdf".into(), pdf.to_vec()),
        ("photo.png".into(), png()),
    ]
}

#[test]
fn separate_batch_creates_one_pdf_per_source_without_replacing_existing_files() {
    use pdfcraft_ui_egui::FilePurpose;
    let dir = std::env::temp_dir().join(format!("pdfcraft-separate-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.pdf"), b"keep this file").unwrap();

    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.export_dir_override = Some(dir.to_string_lossy().into_owned());
    let mut files = mixed_files(&app);
    files.push(("notes.txt".into(), b"another text file".to_vec()));
    app.use_files(FilePurpose::CreateMultipleSeparate, files);

    assert_eq!(std::fs::read(dir.join("notes.pdf")).unwrap(), b"keep this file");
    for name in ["notes (2).pdf", "notes (3).pdf", "photo.pdf"] {
        assert!(std::fs::read(dir.join(name)).unwrap().starts_with(b"%PDF-"), "{name}");
    }
    assert!(!dir.join("a (2).pdf").exists(), "an already-PDF input is skipped");
    assert!(!dir.join("report.pdf").exists(), "an invalid Office file is skipped");
    assert!(app.views.is_empty(), "batch conversion must not open PDF tabs");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn separate_batch_command_picks_files_and_writes_to_the_chosen_folder() {
    use egui_kittest::Harness;
    let dir = std::env::temp_dir().join(format!("pdfcraft-separate-command-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("letter.txt");
    std::fs::write(&source, b"A separate PDF").unwrap();
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.pick_override = Some(vec![source.to_string_lossy().into_owned()]);
        app.export_dir_override = Some(dir.to_string_lossy().into_owned());
        app
    });
    assert!(h.state_mut().execute("create.multiple_separate"));
    h.run_steps(2);
    assert!(std::fs::read(dir.join("letter.pdf")).unwrap().starts_with(b"%PDF-"));
    assert!(h.state().views.is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn multiple_files_open_as_one_document_in_the_page_grid() {
    use egui_kittest::{Harness, kittest::Queryable};
    let path = std::env::temp_dir().join(format!("pdfcraft-create-multiple-{}.pdf", std::process::id()));
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, 720.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        let files = mixed_files(&app);
        app.use_files(pdfcraft_ui_egui::FilePurpose::CreateMultiple, files);
        app
    });
    h.run_steps(4);
    {
        let app = h.state();
        assert_eq!(app.views.len(), 1);
        assert!(app.views[0].organize, "the pages are shown as a grid");
        let doc = app.session.get(app.views[0].id).unwrap();
        assert_eq!(doc.name, "Combined.pdf");
        assert!(doc.dirty && doc.path.is_none(), "unsaved until the user saves it");
        assert_eq!(doc.info.pages.len(), 3, "the Word file can't be converted and is left out");
        assert_eq!(doc.info.outline.iter().map(|o| o.title.as_str()).collect::<Vec<_>>(), ["notes", "a", "photo"]);
    }
    h.get_by_label_contains("left out: report.docx");
    h.get_by_label("Insert a file before page 2");
    if let Ok(path) = std::env::var("PDFCRAFT_CREATE_MULTIPLE_SHOT") {
        let _gpu = crate::gpu();
        h.run_steps(20);
        h.render().unwrap().save(path).unwrap();
    }
    // Remove the middle page and save what is left.
    h.get_by_label("Page 2").click();
    h.run_steps(2);
    h.get_by_label("Delete pages (Delete)").click();
    h.run_steps(3);
    h.state_mut().save_override = Some(path.to_string_lossy().into_owned());
    h.get_by_label("Save pages").click();
    h.run_steps(4);
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    assert!(!doc.dirty && doc.info.pages.len() == 2);
    assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF-"));
}

#[test]
fn multiple_files_that_cannot_be_converted_open_nothing() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.use_files(pdfcraft_ui_egui::FilePurpose::CreateMultiple, vec![("report.docx".into(), b"PK\x03\x04".to_vec())]);
    assert!(app.views.is_empty());
    assert!(app.toast.is_some());
}

#[test]
fn multiple_files_past_a_thousand_are_all_used() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    let pdf = app.session.create_from_text("a", "from a pdf").unwrap();
    let files: Vec<(String, Vec<u8>)> = (0..1001).map(|i| (format!("f{i}.pdf"), pdf.to_vec())).collect();
    app.use_files(pdfcraft_ui_egui::FilePurpose::CreateMultiple, files);
    assert_eq!(app.views.len(), 1);
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.info.pages.len(), 1001, "every picked file is used, in order");
}

#[test]
fn new_pdf_opens_a_template_gallery_without_creating_a_document() {
    use egui_kittest::{Harness, kittest::Queryable};
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        assert!(app.execute("create.new"));
        app
    });
    h.run_steps(4);
    assert!(h.state().session.docs().is_empty());
    h.get_by_label("New PDF");
    assert!(h.query_by_label("Close").is_none(), "the gallery has one action footer");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().session.docs().is_empty());
}

fn wait_for_template(h: &mut egui_kittest::Harness<'static, PdfCraftApp>) {
    use egui_kittest::kittest::{NodeT, Queryable};
    for _ in 0..200 {
        h.run_steps(2);
        if !h.get_by_label("Create").accesskit_node().is_disabled() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("template preview did not finish");
}

#[test]
fn gallery_creates_fillable_starters_and_template_open_is_always_a_copy() {
    use egui_kittest::{Harness, kittest::Queryable};
    let dir = std::env::temp_dir().join(format!("pdfcraft-gallery-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let template = dir.join("intake.pdfcraft-template");
    let save = template.to_string_lossy().into_owned();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.save_override = Some(save);
        assert!(app.execute("create.new"));
        app
    });
    h.run_steps(4);
    h.get_by_label("Client intake").scroll_to_me();
    h.run_steps(4);
    h.get_by_label("Client intake").click();
    wait_for_template(&mut h);
    h.get_by_label("Export template...").click();
    h.run_steps(4);
    let original = std::fs::read(&template).unwrap();
    h.get_by_label("Create").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    let app = h.state();
    let doc = &app.session.docs()[0];
    assert_eq!(doc.form.len(), 5);
    assert!(doc.dirty && doc.path.is_none());
    h.state_mut().open_bytes("intake.pdfcraft-template", Some(template.to_string_lossy().into_owned()), original.clone()).unwrap();
    h.run_steps(4);
    assert_eq!(h.state().session.docs().len(), 2);
    assert!(h.state().session.docs().iter().all(|d| d.path.is_none() && d.dirty));
    assert_eq!(std::fs::read(template).unwrap(), original);
}

#[test]
fn template_gallery_fits_small_windows_and_both_themes() {
    use egui_kittest::{Harness, kittest::Queryable};
    for size in [egui::vec2(1200.0, 800.0), egui::vec2(640.0, 480.0)] {
        for theme in ["light", "dark"] {
            let mut builder = Harness::builder().with_size(size);
            if std::env::var_os("PDFCRAFT_TEMPLATE_SHOTS").is_some() {
                builder = builder.wgpu();
            }
            let mut h = builder.build_eframe(move |_cc| {
                let mut app = PdfCraftApp::new();
                app.set_option("language", "en").unwrap();
                app.set_option("theme", theme).unwrap();
                assert!(app.execute("create.new"));
                app
            });
            h.run_steps(4);
            h.get_by_label("Flyer").scroll_to_me();
            h.run_steps(3);
            h.get_by_label("Flyer").click();
            wait_for_template(&mut h);
            h.run_steps(6);
            for label in ["Cancel", "Create", "Import template...", "Export template..."] {
                let rect = h.get_by_label(label).rect();
                assert!(
                    rect.top() >= 0.0 && rect.bottom() <= size.y && rect.left() >= 0.0 && rect.right() <= size.x,
                    "{label}: {rect:?} outside {size:?}"
                );
            }
            if let Ok(dir) = std::env::var("PDFCRAFT_TEMPLATE_SHOTS") {
                let _gpu = crate::gpu();
                h.render().unwrap().save(format!("{dir}/template-gallery-{theme}-{}.png", size.x as u32)).unwrap();
            }
        }
    }
}
