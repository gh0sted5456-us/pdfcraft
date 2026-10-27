//! The Preferences dialog keeps OK in reach in a short window (#3557): the settings scroll, the
//! OK row does not.

use egui::Rect;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn category_rail_switches_the_right_pane_without_losing_changes_or_moving_ok() {
    let mut h = harness(800.0);
    let ok = h.get_by_label("OK").rect();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Documents").click();
    h.run_steps(4);
    assert!(h.query_by_label("Interface language").is_none(), "only the chosen category is drawn");
    let gaps = h.get_by_label("Show gaps between pages").rect();
    let category = h.get_by_role_and_label(egui::accesskit::Role::Button, "Documents").rect();
    assert!(category.right() < gaps.left(), "categories belong on the left, settings on the right");
    h.get_by_label("Show gaps between pages").click();
    h.run_steps(3);
    let chosen = h.state().view_defaults.show_page_gaps;
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Language").click();
    h.run_steps(4);
    h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Interface language");
    assert!(h.query_by_label("Show gaps between pages").is_none());
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Documents").click();
    h.run_steps(4);
    assert_eq!(h.state().view_defaults.show_page_gaps, chosen);
    assert!(h.state_mut().set_option("preferences-category", "missing").is_err());
    assert_eq!(h.state().preferences_category, pdfcraft_ui_egui::PreferencesCategory::Documents);
    assert_eq!(h.get_by_label("OK").rect(), ok, "the footer stays put when changing categories");
}

#[test]
fn all_categories_stay_centered_and_bounded_in_both_themes() {
    for theme in ["light", "dark"] {
        for (width, height) in [(1200.0, 800.0), (640.0, 480.0)] {
            let mut h = Harness::builder().with_size(egui::vec2(width, height)).build_eframe(|_cc| {
                let mut app = PdfCraftApp::new();
                app.set_option("language", "en").unwrap();
                app.set_option("dialog", "preferences").unwrap();
                app
            });
            h.state_mut().set_option("theme", theme).unwrap();
            for category in pdfcraft_ui_egui::PreferencesCategory::ALL {
                h.state_mut().set_option("preferences-category", category.key()).unwrap();
                h.run_steps(6);
                let card = h.ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("dialog").with("card"))).unwrap();
                let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
                assert!(card.width() <= 810.0 && card.height() <= 520.0, "compact Settings: {card:?}");
                assert!(screen.contains_rect(card), "{theme} {category:?}: {card:?} exceeds {screen:?}");
                assert!((card.center() - screen.center()).length() < 2.0, "{theme} {category:?}: off center {card:?}");
                assert!(card.contains_rect(h.get_by_label("OK").rect()), "{theme} {category:?}: card {card:?}, OK {:?}", h.get_by_label("OK").rect());
                if let Ok(dir) = std::env::var("PDFCRAFT_SETTINGS_SHOTS") {
                    let _gpu = crate::gpu();
                    h.render().unwrap().save(format!("{dir}/settings-{theme}-{}-{width:.0}.png", category.key())).unwrap();
                }
            }
        }
    }
}

fn harness(height: f32) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1000.0, height)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(6);
    h
}

fn on_screen(rect: Rect, height: f32) -> bool {
    rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= 1000.0 && rect.max.y <= height
}

#[test]
fn ok_is_on_screen_without_scrolling_at_any_window_height() {
    // From a short laptop window to a tall monitor, the settings never fit the short ones. 173 is
    // the smallest window (520 pt) at the largest interface size (300%), 208 is 250%.
    for height in [173.0, 208.0, 260.0, 420.0, 480.0, 560.0, 700.0, 900.0, 1100.0] {
        let h = harness(height);
        let ok = h.get_by_label("OK").rect();
        assert!(on_screen(ok, height), "{height}: OK at {ok:?} is off the {height} pt window");
        let title = h.get_by_label("Title: Preferences").rect();
        assert!(on_screen(title, height), "{height}: the title at {title:?} is off the window");
        // The OK row sits below the scrolled settings, never over them: the settings' last visible
        // row ends above it. The title is above everything.
        assert!(title.max.y < ok.min.y, "{height}: title {title:?} above OK {ok:?}");
    }
}

#[test]
fn ok_closes_the_dialog_in_a_short_window_and_the_settings_still_scroll() {
    let mut h = harness(480.0);
    // A setting far down the list starts out below the visible settings; scrolling brings it in
    // between the title and the OK row, and OK did not move.
    let ok_before = h.get_by_label("OK").rect();
    let title = h.get_by_label("Title: Preferences").rect();
    let far = h.get_by_label("Highlight fields").rect();
    assert!(far.min.y >= ok_before.min.y, "Highlight fields starts out of sight: {far:?} vs OK {ok_before:?}");
    h.get_by_label("Highlight fields").scroll_to_me();
    h.run_steps(4);
    let far = h.get_by_label("Highlight fields").rect();
    assert!(far.min.y >= title.max.y && far.max.y <= ok_before.min.y, "Highlight fields scrolled in between the title and OK: {far:?}");
    assert_eq!(h.get_by_label("OK").rect(), ok_before, "OK stays put while the settings scroll");
    assert!(h.state().dialog.is_some());
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "OK closes Preferences");
}

#[test]
fn the_dialog_opens_at_its_full_size_and_does_not_grow_afterwards() {
    // With OK outside the scroll, the scroll area started from what the modal offered, which
    // grew some 45 pt a frame for ten frames: the dialog opened small and stretched, and clicks
    // in its first frames landed on controls that were still moving.
    for height in [260.0, 560.0, 900.0] {
        let mut h = harness(height);
        let settled = (h.get_by_label("Title: Preferences").rect(), h.get_by_label("OK").rect());
        h.run_steps(8);
        let later = (h.get_by_label("Title: Preferences").rect(), h.get_by_label("OK").rect());
        assert_eq!(settled, later, "{height}: the dialog moved after it had been shown for six frames");
    }
    // And it is in place within a few frames of opening, for the first click: a frame or two of
    // settling is fine, ten frames of stretching is not.
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app
    });
    h.run_steps(4);
    h.state_mut().dialog = Some(pdfcraft_ui_egui::Dialog::Preferences);
    h.run_steps(3);
    let early = h.get_by_label("Right").rect();
    h.run_steps(8);
    let late = h.get_by_label("Right").rect();
    assert!((early.min.y - late.min.y).abs() <= 2.0 && (early.min.x - late.min.x).abs() <= 2.0, "{early:?} then {late:?}");
}

#[test]
fn a_tall_window_keeps_a_bounded_centered_settings_card() {
    let mut h = harness(2600.0);
    h.state_mut().set_option("preferences-category", "javascript").unwrap();
    h.run_steps(6);
    let card = h.ctx.data(|data| data.get_temp::<Rect>(egui::Id::unique("dialog").with("card"))).unwrap();
    assert!(card.height() < 750.0, "settings should not stretch across a tall monitor: {card:?}");
    assert!((card.center() - egui::pos2(500.0, 1300.0)).length() < 2.0, "settings stay centered: {card:?}");
    assert!(on_screen(h.get_by_label("OK").rect(), 2600.0));
    h.get_by_label("Enable Acrobat JavaScript");
}

#[test]
fn additional_categories_adjust_existing_settings_without_starting_work() {
    let mut h = harness(800.0);
    h.state_mut().set_option("preferences-category", "ocr").unwrap();
    h.run_steps(4);
    let before = h.state().auto_ocr;
    h.get_by_label("Recognize scanned text automatically").click();
    h.run_steps(3);
    assert_eq!(h.state().auto_ocr, !before);
    assert!(h.state().ocr_run.is_none());
    h.state_mut().set_option("preferences-category", "editing").unwrap();
    h.run_steps(4);
    h.get_by_label("Bold").click();
    h.run_steps(3);
    assert!(h.state().text_style.bold);
    h.state_mut().set_option("preferences-category", "security").unwrap();
    h.run_steps(4);
    h.get_by_label("Timestamp service");
    h.state_mut().set_option("preferences-category", "assistant").unwrap();
    h.run_steps(4);
    h.get_by_label("Enable Assistant requests (off by default)").click();
    h.run_steps(3);
    let saved = h.state().persist();
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert_eq!(restored.auto_ocr, !before);
    assert!(serde_json::from_str::<serde_json::Value>(&saved).unwrap()["assistant"]["enabled"].as_bool().unwrap());
    h.get_by_label("Manage AI connections").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(pdfcraft_ui_egui::Dialog::Assistant));
    h.get_by_label("Continue with ChatGPT");
}

#[test]
fn the_left_rail_scrolls_to_categories_without_moving_the_dialog() {
    let mut h = harness(480.0);
    let ok = h.get_by_label("OK").rect();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "AI Assistant").scroll_to_me();
    h.run_steps(8);
    let last = h.get_by_role_and_label(egui::accesskit::Role::Button, "AI Assistant");
    assert!(last.rect().bottom() < ok.top(), "last category scrolls into the rail, above the footer");
    last.click();
    h.run_steps(4);
    h.get_by_label("Manage AI connections");
    assert_eq!(h.get_by_label("OK").rect(), ok);
    assert_eq!(h.state().preferences_category, pdfcraft_ui_egui::PreferencesCategory::Assistant);
}
