//! Help ▸ Check for updates (issue #28), with stand-in release sources (no network).

use std::sync::Arc;

use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_ui_egui::PdfCraftApp;
use pdfcraft_ui_egui::updates::{Release, UpdateSource, is_newer};

fn source(answer: Result<&str, &str>) -> UpdateSource {
    let answer = answer.map(str::to_string).map_err(str::to_string);
    Arc::new(move || {
        answer.clone().map(|v| Release { url: format!("https://github.com/gh0sted5456-us/pdfcraft/releases/tag/{v}"), version: v, installer: None })
    })
}

fn harness(answer: Result<&str, &str>) -> Harness<'static, PdfCraftApp> {
    Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.update_source = Some(source(answer));
        app
    })
}

/// Run frames until the background check has reported. By what the dialog shows, with the
/// same 30-second deadline as `support::wait_until`, not for a fixed second: under load the
/// check's thread can take longer (#1277).
fn settle(h: &mut Harness<'static, PdfCraftApp>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        h.run_steps(2);
        if h.query_by_label_contains("Checking for a newer version").is_none() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "timed out waiting for the update check");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
}

#[test]
fn versions_compare_by_number() {
    assert!(is_newer("v0.2.0", "0.1.1"));
    assert!(is_newer("fork-v0.14.1", "0.14.0"));
    assert!(!is_newer("fork-v0.14.0", "0.14.0"));
    assert!(is_newer("v0.1.10", "0.1.9"));
    assert!(is_newer("1", "0.9.9"));
    assert!(!is_newer("v0.1.1", "0.1.1"));
    assert!(!is_newer("v0.1.0", "0.1.1"));
    assert!(!is_newer("v0.1.1-beta.2", "0.1.1"), "suffixes are ignored");
    assert!(!is_newer("nightly", "0.1.1"), "a tag that isn't a version is never newer");
    assert!(!is_newer("v1.2.3.4", "0.1.1"));
    assert!(!is_newer("v99999999999999999999.0.0", "0.1.1"), "out of range");
}

#[test]
fn a_newer_release_is_offered_for_download() {
    let mut h = harness(Ok("v99.0.0"));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("PdfCraft 99.0.0 is available");
    h.get_by_label("Download");
    h.get_by_label("Later").click();
    h.run_steps(3);
    assert!(h.query_by_label_contains("is available").is_none(), "Later closes the dialog");
}

#[test]
fn an_up_to_date_or_failed_check_says_so() {
    let mut h = harness(Ok(concat!("v", env!("CARGO_PKG_VERSION"))));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("is up to date");
    assert!(h.query_by_label("Download").is_none());

    let mut h = harness(Err("couldn't reach GitHub"));
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    h.get_by_label_contains("Couldn't check for updates: couldn't reach GitHub");
}

#[test]
fn nothing_is_asked_until_the_user_checks() {
    // No check at start (the owner's decision): the source is only called from the command.
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        // Settings from a build that had the startup option are ignored.
        app.restore(r#"{"check_updates_at_start": true}"#);
        let counted = counted.clone();
        app.update_source = Some(Arc::new(move || {
            counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Release { installer: None, version: "v99.0.0".into(), url: "https://github.com/gh0sted5456-us/pdfcraft/releases/tag/v99.0.0".into() })
        }));
        app
    });
    settle(&mut h);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert!(h.query_by_label_contains("is available").is_none());
    h.state_mut().execute("help.check_updates");
    settle(&mut h);
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    h.get_by_label_contains("PdfCraft 99.0.0 is available");
}

#[test]
fn preferences_downloads_a_fork_update_and_requires_closed_documents_to_install() {
    use pdfcraft_ui_egui::updates::Installer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    let installs = Arc::new(AtomicUsize::new(0));
    let installed = installs.clone();
    let mut h = harness(Ok("v99.0.0"));
    h.state_mut().update_source = Some(Arc::new(|| {
        Ok(Release {
            version: "v99.0.0".into(),
            url: "https://github.com/gh0sted5456-us/pdfcraft/releases/tag/v99.0.0".into(),
            installer: Some(Installer { name: "update.exe".into(), url: "unused-test-source".into(), size: 20, sha256: "test".into() }),
        })
    }));
    h.state_mut().update_download = Some(Arc::new(|_, progress| {
        progress.store(20, Ordering::Relaxed);
        Ok(std::path::PathBuf::from("test-update.exe"))
    }));
    h.state_mut().update_install = Some(Arc::new(move |_, path| {
        assert_eq!(path, std::path::PathBuf::from("test-update.exe"));
        installed.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    h.state_mut().set_option("dialog", "preferences").unwrap();
    h.state_mut().set_option("preferences-category", "updates").unwrap();
    h.run_steps(3);
    h.get_by_label("Check for updates").click();
    settle(&mut h);
    h.get_by_label("Download").click();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        h.run_steps(2);
        if h.query_by_label("Download complete · SHA-256 verified").is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(h.query_by_label("Download").is_none(), "the ready state offers installation rather than another download");
    let bytes = h.state_mut().session.create_blank(300.0, 400.0, 1).unwrap();
    h.state_mut().open_bytes("unsaved.pdf", None, bytes.as_ref().clone()).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Close").is_none(), "opening a check inside Preferences doesn't create a second popup");
    assert!(h.get_by_label("Install update").accesskit_node().is_disabled());
    assert_eq!(installs.load(Ordering::SeqCst), 0);
    h.state_mut().close_tab(0);
    h.run_steps(3);
    h.get_by_label("Install update").click();
    h.run_steps(3);
    assert_eq!(installs.load(Ordering::SeqCst), 1);
}
