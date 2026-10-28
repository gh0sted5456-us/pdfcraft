//! Fork installers: bounded HTTPS streaming, atomic staging and verification before activation.
use pdfcraft_ui_egui::updates::{Installer, RELEASES_PAGE};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

const LIMIT: u64 = 512 * 1024 * 1024;

pub fn installer(value: &serde_json::Value, tag: &str, name: &str) -> Option<Installer> {
    let size = value["size"].as_u64().filter(|size| (1..=LIMIT).contains(size))?;
    let sha256 = value["digest"].as_str()?.strip_prefix("sha256:")?;
    if value["name"].as_str()? != name
        || sha256.len() != 64
        || !sha256.bytes().all(|b| b.is_ascii_hexdigit())
        || tag.len() > 64
        || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
        || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
    {
        return None;
    }
    let url = value["browser_download_url"].as_str()?;
    if url != format!("{RELEASES_PAGE}/download/{tag}/{name}") {
        return None;
    }
    Some(Installer { name: name.into(), url: url.into(), size, sha256: sha256.to_ascii_lowercase() })
}

fn allowed_url(url: &str) -> bool {
    let Ok(uri) = url.parse::<ureq::http::Uri>() else { return false };
    uri.scheme_str() == Some("https")
        && uri
            .authority()
            .is_some_and(|a| matches!(a.as_str(), "github.com" | "release-assets.githubusercontent.com" | "objects.githubusercontent.com"))
}

fn transfer(reader: &mut impl Read, writer: &mut impl Write, expected: &Installer, progress: &AtomicU64) -> Result<(), String> {
    let mut hash = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buffer).map_err(|e| format!("Download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        total = total.checked_add(n as u64).filter(|n| *n <= expected.size && *n <= LIMIT).ok_or("Installer is larger than its published size")?;
        let bytes = buffer.get(..n).ok_or("Invalid download chunk")?;
        hash.update(bytes);
        writer.write_all(bytes).map_err(|e| format!("Could not save update: {e}"))?;
        progress.store(total, Ordering::Relaxed);
    }
    if total != expected.size || !hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>().eq_ignore_ascii_case(&expected.sha256)
    {
        return Err("Installer size or SHA-256 does not match GitHub. Nothing was installed.".into());
    }
    Ok(())
}

pub fn download(asset: Installer, progress: Arc<AtomicU64>) -> Result<PathBuf, String> {
    // Validate again: the UI callback is also callable by stand-in sources.
    let tag = asset.url.strip_prefix(&format!("{RELEASES_PAGE}/download/")).and_then(|rest| rest.split('/').next()).ok_or("Invalid installer URL")?;
    if installer(
        &serde_json::json!({"name":asset.name,"size":asset.size,"digest":format!("sha256:{}",asset.sha256),"browser_download_url":asset.url}),
        tag,
        &asset.name,
    )
    .is_none()
    {
        return Err("Invalid installer metadata".into());
    }
    let dir = super::settings_dir().ok_or("No writable update folder")?.join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create update folder: {e}"))?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .max_redirects(0)
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(super::updates::os_roots()?).build())
        .build()
        .new_agent();
    let mut url = asset.url.clone();
    for _ in 0..6 {
        if url.len() > 16384 || !allowed_url(&url) {
            return Err("GitHub redirected the installer outside its HTTPS download hosts".into());
        }
        let mut response =
            agent.get(&url).config().http_status_as_error(false).build().call().map_err(|e| format!("Could not download update: {e}"))?;
        if response.status().is_redirection() {
            url = response.headers().get("location").and_then(|v| v.to_str().ok()).ok_or("Invalid GitHub download redirect")?.to_string();
            continue;
        }
        if !response.status().is_success() {
            return Err(format!("Update download returned HTTP {}", response.status()));
        }
        let mut file = tempfile::NamedTempFile::new_in(&dir).map_err(|e| format!("Could not stage update: {e}"))?;
        transfer(&mut response.body_mut().as_reader(), file.as_file_mut(), &asset, &progress)?;
        file.as_file().sync_all().map_err(|e| format!("Could not finish update: {e}"))?;
        let destination = dir.join("PdfCraft-update.exe");
        file.persist(&destination).map_err(|e| format!("Could not publish verified update: {e}"))?;
        return Ok(destination);
    }
    Err("Too many GitHub download redirects".into())
}

pub fn install(asset: Installer, path: PathBuf, session: pdfcraft_ui_egui::last_session::LastSession) -> Result<(), String> {
    let expected_dir = super::settings_dir().ok_or("No update folder")?.join("updates");
    if path.parent() != Some(expected_dir.as_path()) || path.file_name().and_then(|s| s.to_str()) != Some("PdfCraft-update.exe") {
        return Err("Installer is not in PdfCraft's update folder".into());
    }
    #[cfg(windows)]
    {
        let mut processes = winsafe::HPROCESSLIST::CreateToolhelp32Snapshot(winsafe::co::TH32CS::SNAPPROCESS, None)
            .map_err(|e| format!("Could not check other PdfCraft windows: {e}"))?;
        for process in processes.iter_processes() {
            let process = process.map_err(|e| format!("Could not check other PdfCraft windows: {e}"))?;
            if other_instance(process.th32ProcessID, &process.szExeFile(), std::process::id()) {
                return Err("Close the other PdfCraft instance before installing this update.".into());
            }
        }
    }
    verify_file(&path, &asset)?;
    #[cfg(windows)]
    {
        if pdfcraft_ui_egui::portable::current().dir.is_some() {
            return Err("Portable builds need the portable ZIP from the release page; an MSI cannot update the portable folder.".into());
        }
        let application = std::env::current_exe().map_err(|e| format!("Could not locate Lion PDF: {e}"))?;
        let mut resume = tempfile::Builder::new()
            .prefix("resume-")
            .suffix(".json")
            .tempfile_in(&expected_dir)
            .map_err(|e| format!("Could not stage the restart session: {e}"))?;
        let job = serde_json::json!({"parent": std::process::id(), "application": application, "session": session,
            "size": asset.size, "sha256": asset.sha256});
        serde_json::to_writer(resume.as_file_mut(), &job).map_err(|e| format!("Could not save the restart session: {e}"))?;
        resume.as_file().sync_all().map_err(|e| format!("Could not finish the restart session: {e}"))?;
        let (_, resume) = resume.keep().map_err(|e| format!("Could not retain the restart session: {e}"))?;
        let mut child = match std::process::Command::new(&path).arg("--update").arg(&resume).spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = std::fs::remove_file(&resume);
                return Err(format!("Could not start the update: {error}"));
            }
        };
        let ready = resume.with_extension("ready");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !ready.is_file() {
            if child.try_wait().map_err(|e| e.to_string())?.is_some() || std::time::Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&resume);
                return Err("The staged updater did not become ready. Your PDFs remain open.".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = std::fs::remove_file(ready);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = session;
        Err("Use the release page to install this platform's update".into())
    }
}

#[cfg(windows)]
fn other_instance(pid: u32, image: &str, current: u32) -> bool {
    pid != current && image.eq_ignore_ascii_case("pdfcraft.exe")
}

fn verify_file(path: &Path, asset: &Installer) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| format!("Could not read installer: {e}"))?;
    if !metadata.is_file() || metadata.len() != asset.size {
        return Err("The downloaded installer has changed; download it again".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| format!("Could not open installer: {e}"))?;
    transfer(&mut file, &mut std::io::sink(), asset, &AtomicU64::new(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn asset(bytes: &[u8]) -> Installer {
        Installer {
            name: "pdfcraft-0.14.1001-with-printer-x64.exe".into(),
            url: format!("{RELEASES_PAGE}/download/v0.14.1001/pdfcraft-0.14.1001-with-printer-x64.exe"),
            size: bytes.len() as u64,
            sha256: Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>(),
        }
    }
    #[test]
    fn only_bounded_fork_assets_with_digests_are_accepted() {
        let good = asset(b"test");
        let mut value =
            serde_json::json!({"name":good.name,"size":good.size,"digest":format!("sha256:{}",good.sha256),"browser_download_url":good.url});
        assert!(installer(&value, "v0.14.1001", &good.name).is_some());
        value["browser_download_url"] = "https://example.com/installer.exe".into();
        assert!(installer(&value, "v0.14.1001", &good.name).is_none());
        value["browser_download_url"] = good.url.into();
        value["size"] = (LIMIT + 1).into();
        assert!(installer(&value, "v0.14.1001", &good.name).is_none());
        value["size"] = 4.into();
        value["digest"] = serde_json::Value::Null;
        assert!(installer(&value, "v0.14.1001", &good.name).is_none());
        for bad in [
            "http://github.com/file",
            "https://github.com.evil/file",
            "https://github.com@evil/file",
            "https://127.0.0.1/file",
            "https://github.com:444/file",
        ] {
            assert!(!allowed_url(bad));
        }
        assert!(allowed_url("https://release-assets.githubusercontent.com/github-production-release-asset/file?sig=value"));
    }
    #[test]
    fn truncated_oversized_and_changed_payloads_never_become_installers() {
        let good = asset(b"abcd");
        let mut target = Vec::new();
        let progress = AtomicU64::new(0);
        transfer(&mut b"abcd".as_slice(), &mut target, &good, &progress).unwrap();
        assert_eq!(target, b"abcd");
        assert_eq!(progress.load(Ordering::Relaxed), 4);
        for wrong in [b"abc".as_slice(), b"abcde", b"abce"] {
            assert!(transfer(&mut &*wrong, &mut Vec::new(), &good, &progress).is_err());
        }
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"abce").unwrap();
        assert!(verify_file(file.path(), &good).is_err());
    }
    #[test]
    #[ignore = "downloads the published fork installer over HTTPS; never installs it"]
    fn a_published_fork_installer_downloads_and_verifies() {
        let release = super::super::updates::latest_release().unwrap();
        let installer = release.installer.expect("the fork release needs a Windows x64 installer and GitHub digest");
        let path = download(installer.clone(), Arc::new(AtomicU64::new(0))).unwrap();
        verify_file(&path, &installer).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn installation_waits_for_other_instances_without_blocking_its_own() {
        assert!(!other_instance(10, "pdfcraft.exe", 10));
        assert!(other_instance(11, "PdfCraft.EXE", 10));
        assert!(!other_instance(11, "pdfcraft-printer-setup.exe", 10));
        assert!(!other_instance(11, "notepad.exe", 10));
    }
}

/// Consume the one-shot session without changing the normal startup preference.
pub fn resume(app: &mut pdfcraft_ui_egui::PdfCraftApp, path: &Path) -> Result<(), String> {
    let dir = super::settings_dir().ok_or("No update folder")?.join("updates");
    resume_in(app, path, &dir)
}

fn resume_in(app: &mut pdfcraft_ui_egui::PdfCraftApp, path: &Path, dir: &Path) -> Result<(), String> {
    if path.parent() != Some(dir) || !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("resume-") && n.ends_with(".json")) {
        return Err("Restart session is outside the update folder".into());
    }
    let file = std::fs::File::open(path).map_err(|e| format!("Could not reopen the update session: {e}"))?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 1 << 20 {
        return Err("Restart session is too large".into());
    }
    let job: serde_json::Value = serde_json::from_reader(file.take((1 << 20) + 1)).map_err(|e| format!("Invalid restart session: {e}"))?;
    if !job["session"].is_object() {
        return Err("Missing restart session".into());
    }
    app.last_session = pdfcraft_ui_egui::last_session::LastSession::from_json(&job["session"]);
    let preference = app.reopen_last_session;
    app.reopen_last_session = true;
    app.reopen_last_files(&[]);
    app.reopen_last_session = preference;
    std::fs::remove_file(path).map_err(|e| format!("Could not clear the used restart session: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    #[test]
    fn update_reopens_saved_pdfs_without_changing_theme_or_normal_startup() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("Synthetic resume.pdf");
        let resume = folder.path().join("resume-test.json");
        let mut app = pdfcraft_ui_egui::PdfCraftApp::new();
        app.run_inline = true;
        app.set_option("theme", "glass").unwrap();
        app.set_option("glass-opacity", "42").unwrap();
        let glass = app.glass;
        let theme = app.theme_preference;
        let bytes = app.session.create_blank(300.0, 400.0, 2).unwrap();
        std::fs::write(&path, bytes.as_ref()).unwrap();
        std::fs::write(&resume, serde_json::json!({"session":{"files":[{"path":path,"page":1}],"active":path}}).to_string()).unwrap();
        app.reopen_last_session = false;
        resume_in(&mut app, &resume, folder.path()).unwrap();
        assert_eq!(app.views.len(), 1);
        assert_eq!(app.views[0].current, 1);
        assert_eq!(app.glass, glass);
        assert_eq!(app.theme_preference, theme);
        assert!(!app.reopen_last_session);
        assert!(!resume.exists());
        assert!(resume_in(&mut app, &folder.path().join("wrong.json"), folder.path()).is_err());
    }
}
