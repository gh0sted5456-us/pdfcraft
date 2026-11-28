//! Community links: the About dialog shows the credits, and the home screen's community card
//! closes for the run.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |cc| {
        crate::support::install_runtime(&cc.egui_ctx);
        {
            let mut app = crate::control::new_app();
            app.set_option(&egui::Context::default(), "language", "en").unwrap();
            setup(&mut app);
            app
        }
    });
    h.run_steps(4);
    h
}

#[test]
fn home_community_card_closes_for_this_run() {
    let mut h = harness(|_| {});
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Join the ArtCraft community");
    h.get_by_role_and_label(egui::accesskit::Role::Group, "Join the ArtCraft community").get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().community_card_closed);
    assert!(h.query_by_label("Upstream PdfCraft community").is_none());
    assert!(h.query_by_label("Join our Discord").is_none());
    // Not remembered: a fresh app shows the card again.
    assert!(!crate::control::new_app().community_card_closed);
}

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it. A table row is named by
/// its first cell, so a name may be the row's and its cell's: the first (outer) node is the one returned.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, PdfCraftApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_all_by_label(label).next().map(|n| n.rect());
        if now.is_some() && now == last {
            return h.query_all_by_label(label).next().expect("the node was just found");
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

#[test]
fn about_dialog_has_contributors_and_models_tabs() {
    let mut h = harness(|app| app.set_option(&egui::Context::default(), "dialog", "about").unwrap());
    settled(&mut h, "Contributors").click();
    // The owner is always in the compiled-in credits (contributors/contributors.json), shown by username.
    settled(&mut h, "@echelon");
    settled(&mut h, "Table").click();
    settled(&mut h, "PRs");
    settled(&mut h, "Display name").click();
    settled(&mut h, "Brandon Thomas");
    settled(&mut h, "Models").click();
    h.run_steps(4);
    assert!(h.query_all_by_label("Anthropic").count() >= 1);
}

#[test]
fn about_tabs_text_and_links_are_centered_at_normal_and_narrow_widths() {
    for width in [1400.0, 640.0] {
        let mut h = Harness::builder().with_size(egui::vec2(width, 850.0)).build_eframe(|_cc| {
            let mut app = PdfCraftApp::new();
            app.set_option("language", "en").unwrap();
            let bytes = app.session.create_blank(200.0, 200.0, 1).unwrap();
            app.open_bytes("about-layout.pdf", None, bytes.as_ref().clone()).unwrap();
            app.dialog = Some(Dialog::About);
            app
        });
        h.run_steps(8);
        let center = width / 2.0;
        let tabs = ["About", "Contributors", "Models"].into_iter().fold(egui::Rect::NOTHING, |rect, label| rect.union(h.get_by_label(label).rect()));
        assert!((tabs.center().x - center).abs() < 1.5, "tab group is off center at {width}: {tabs:?}");
        for label in ["Personal build by @gh0sted5456-us", "Based on PdfCraft by the ArtCraft team and community."] {
            assert!((h.get_by_label(label).rect().center().x - center).abs() < 1.5, "{label} is off center at {width}");
        }
        let links = ["Join our Discord", "PdfCraft web page", "PdfCraft on GitHub", "ArtCraft website"]
            .into_iter()
            .fold(egui::Rect::NOTHING, |rect, label| rect.union(h.get_by_label(label).rect()));
        assert!((links.center().x - center).abs() < 1.5, "community links are off center at {width}: {links:?}");
    }
}
