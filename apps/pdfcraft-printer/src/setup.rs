#![forbid(unsafe_code)]

use windows::Management::Deployment::PackageManager;
use windows_core::HSTRING;

const NAME: &str = "PdfCraft.Printer.Personal";
const PUBLISHER: &str = "CN=gh0sted5456-us, OID.2.25.311729368913984317654407730594956997722=1";

pub fn message(title: &str, text: &str, error: bool) {
    let level = if error { rfd::MessageLevel::Error } else { rfd::MessageLevel::Info };
    let _ = rfd::MessageDialog::new().set_title(title).set_description(text).set_level(level).show();
}

pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    #[cfg(feature = "bundle")]
    if args.first().is_some_and(|a| a == "--update") {
        if args.len() != 2 {
            return Err("Invalid update arguments".into());
        }
        return apply_update(std::path::Path::new(args.get(1).ok_or("Missing update session")?));
    }
    if args.iter().any(|a| !["--status", "--remove-printer", "--printer-only", "--quiet", "--install"].contains(&a.as_str())) {
        return Err("Usage: pdfcraft-printer [--status | --remove-printer | --install] [--printer-only] [--quiet]".into());
    }
    if args.iter().any(|a| a == "--status") {
        return status(false);
    }
    if args.iter().any(|a| a == "--remove-printer") {
        return status(true);
    }
    #[cfg(feature = "bundle")]
    {
        install(&args)?;
        Ok(())
    }
    #[cfg(not(feature = "bundle"))]
    Err("This is the PdfCraft printer component. Run the combined Lion PDF installer to install it, or use --status / --remove-printer.".into())
}

fn status(remove: bool) -> Result<(), String> {
    let manager = PackageManager::new().map_err(|e| e.to_string())?;
    let packages = manager
        .FindPackagesByUserSecurityIdNamePublisher(&HSTRING::new(), &HSTRING::from(NAME), &HSTRING::from(PUBLISHER))
        .map_err(|e| e.to_string())?;
    let mut installed = Vec::new();
    for package in packages {
        let id = package.Id().and_then(|id| id.FullName()).map_err(|e| e.to_string())?;
        if remove {
            let result = manager.RemovePackageAsync(&id).and_then(|op| op.join()).map_err(|e| e.to_string())?;
            let error = result.ExtendedErrorCode().map_err(|e| e.to_string())?;
            if error.is_err() {
                return Err(format!("Printer removal failed: {} ({error:?})", result.ErrorText().map_err(|e| e.to_string())?));
            }
        }
        installed.push(id.to_string_lossy());
    }
    println!(
        "{}",
        serde_json::json!({"plugin":NAME,"installed":!remove && !installed.is_empty(),"printer_ready":printer_ready(),"removed":remove,"packages":installed})
    );
    Ok(())
}

#[cfg(any(feature = "bundle", test))]
fn stage_msi(folder: &std::path::Path, source: &std::path::Path, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    use std::io::Write as _;
    // Windows repair looks for the filename registered by the earlier installation.
    let name = source.file_name().ok_or("The embedded installer has no source filename")?;
    let path = folder.join(name);
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    drop(file);
    // Older combined setups registered this name; stage both names in our fresh private folder.
    if !name.to_string_lossy().eq_ignore_ascii_case("PdfCraft.msi") {
        std::fs::copy(&path, folder.join("PdfCraft.msi")).map_err(|e| e.to_string())?;
    }
    Ok(path)
}

#[cfg(feature = "bundle")]
fn install(args: &[String]) -> Result<(), String> {
    use std::io::Write as _;
    use windows::Foundation::{Metadata::ApiInformation, Uri};
    use windows::Management::Deployment::AddPackageOptions;
    if !ApiInformation::IsTypePresent(&HSTRING::from("Windows.Graphics.Printing.Workflow.PrintWorkflowVirtualPrinterSession"))
        .map_err(|e| e.to_string())?
    {
        return Err("PdfCraft Printer requires Windows 11 24H2 (build 26100) or newer. This installer did not change your system.".into());
    }
    let temporary = tempfile::Builder::new().prefix("craftpdf-setup-").tempdir().map_err(|e| e.to_string())?;
    if !args.iter().any(|a| a == "--printer-only") {
        let path = stage_msi(temporary.path(), std::path::Path::new(env!("PDFCRAFT_SETUP_MSI")), include_bytes!(env!("PDFCRAFT_SETUP_MSI")))?;
        let log_folder = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("Windows local application data is unavailable")?)
            .join("PdfCraft")
            .join("setup");
        std::fs::create_dir_all(&log_folder).map_err(|e| format!("Could not create the setup log folder: {e}"))?;
        let log = tempfile::Builder::new()
            .prefix("PdfCraft-install-")
            .suffix(".log")
            .tempfile_in(&log_folder)
            .map_err(|e| format!("Could not create the setup log: {e}"))?;
        let (file, log_path) = log.keep().map_err(|e| e.to_string())?;
        drop(file);
        let system = std::env::var_os("SystemRoot").ok_or("Windows system directory is unavailable")?;
        let status = std::process::Command::new(std::path::PathBuf::from(system).join("System32/msiexec.exe"))
            .arg("/i")
            .arg(&path)
            .args(msi_ui_args(args.iter().any(|a| a == "--quiet")))
            .arg(&log_path)
            .status()
            .map_err(|e| format!("Could not start Windows Installer: {e}. Setup log: {}", log_path.display()))?;
        if !matches!(status.code(), Some(0 | 3010)) {
            return Err(format!(
                "Lion PDF installation did not finish (Windows Installer result {}). Printer setup did not run. Setup log: {}",
                status.code().map_or_else(|| "unavailable".to_string(), |code| code.to_string()),
                log_path.display()
            ));
        }
    }
    let path = temporary.path().join("PdfCraft.Printer.msix");
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| e.to_string())?;
    file.write_all(include_bytes!(env!("PDFCRAFT_SETUP_MSIX"))).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    drop(file);
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let path_text = path.to_string_lossy();
    let path_text = path_text.strip_prefix(r"\\?\").unwrap_or(&path_text);
    let uri = Uri::CreateUri(&HSTRING::from(path_text)).map_err(|e| e.to_string())?;
    let options = AddPackageOptions::new().map_err(|e| e.to_string())?;
    // Applies only to this embedded personal-build package; never changes certificate stores or system policy.
    options.SetAllowUnsigned(true).map_err(|e| e.to_string())?;
    let previous_default = default_printer()?;
    let registration = (|| {
        let result = PackageManager::new()
            .and_then(|p| p.AddPackageByUriAsync(&uri, &options))
            .and_then(|op| op.join())
            .map_err(|e| format!("Lion PDF setup could not register the printer plugin: {e}"))?;
        let error = result.ExtendedErrorCode().map_err(|e| e.to_string())?;
        if error.is_err() {
            return Err(format!("Printer plugin registration failed: {} ({error:?})", result.ErrorText().map_err(|e| e.to_string())?));
        }
        for _ in 0..50 {
            if printer_ready() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Err("The plugin package installed, but Windows did not register PdfCraft Printer. The printer is not ready; see the setup troubleshooting guide.".to_string())
    })();
    let restored = restore_default(previous_default.as_deref());
    registration.and(restored)?;
    status(false)
}

fn printer_ready() -> bool {
    windows::Devices::Printers::IppPrintDevice::IsIppPrinter(&HSTRING::from("PdfCraft Printer")).unwrap_or(false)
}

#[cfg(feature = "bundle")]
fn default_printer() -> Result<Option<String>, String> {
    let key = windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows NT\CurrentVersion\Windows")
        .map_err(|e| format!("Could not preserve the default printer: {e}"))?;
    match key.get_string("Device") {
        Ok(device) => default_device_name(&device)
            .map(Some)
            .ok_or_else(|| "The current default printer could not be identified; setup did not register a printer".into()),
        Err(error) if error.code().0 == 0x80070002u32 as i32 => Ok(None),
        Err(error) => Err(format!("Could not preserve the default printer: {error}")),
    }
}

#[cfg(any(feature = "bundle", test))]
fn default_device_name(device: &str) -> Option<String> {
    let separator = device.to_ascii_lowercase().rfind(",winspool,")?;
    device.get(..separator).filter(|s| !s.is_empty()).map(str::to_owned)
}

#[cfg(feature = "bundle")]
fn restore_default(previous: Option<&str>) -> Result<(), String> {
    let Some(previous) = previous else {
        return Ok(());
    };
    if default_printer()?.as_deref() == Some(previous) {
        return Ok(());
    }
    let system = std::env::var_os("SystemRoot").ok_or("Windows system directory is unavailable")?;
    let result = std::process::Command::new(std::path::PathBuf::from(system).join("System32/rundll32.exe"))
        .args(["printui.dll,PrintUIEntry", "/y", "/n", previous])
        .status()
        .map_err(|e| format!("Could not restore your default printer: {e}"))?;
    if !result.success() || default_printer()?.as_deref() != Some(previous) {
        return Err("The printer plugin installed, but Windows could not restore your previous default printer".into());
    }
    Ok(())
}

#[cfg(any(feature = "bundle", test))]
fn msi_ui_args(quiet: bool) -> &'static [&'static str] {
    if quiet { &["/passive", "/norestart", "/l*v"] } else { &["/norestart", "/l*v"] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_setup_shows_the_standard_finish_screen() {
        assert_eq!(msi_ui_args(false), ["/norestart", "/l*v"]);
        assert_eq!(msi_ui_args(true), ["/passive", "/norestart", "/l*v"]);
    }

    #[test]
    fn extracted_msi_preserves_its_registered_source_filename() {
        let bytes = b"synthetic MSI payload";
        for name in ["pdfcraft-0.14.0-scan-security-update-x64.msi", "PdfCraft release x64.msi", "PdfCraft.msi", "pdfcraft.MSI"] {
            let folder = tempfile::tempdir().unwrap();
            let source = std::path::Path::new(r"D:\build").join(name);
            let extracted = stage_msi(folder.path(), &source, bytes).unwrap();
            assert_eq!(extracted, folder.path().join(name), "Windows repair needs the original MSI filename");
            assert_eq!(std::fs::read(extracted).unwrap(), bytes);
            assert_eq!(std::fs::read(folder.path().join("PdfCraft.msi")).unwrap(), bytes, "old combined setup source names remain usable");
        }
        let folder = tempfile::tempdir().unwrap();
        assert!(stage_msi(folder.path(), std::path::Path::new(""), bytes).is_err());
    }
    #[test]
    fn default_printer_names_keep_commas_and_unicode() {
        assert_eq!(default_device_name("Office, étage,winspool,Ne01:"), Some("Office, étage".into()));
        assert_eq!(default_device_name("Printer,WinSpool,port"), Some("Printer".into()));
        assert_eq!(default_device_name("damaged"), None);
        assert_eq!(default_device_name(",winspool,port"), None);
    }
}

#[cfg(any(feature = "bundle", test))]
fn read_update(path: &std::path::Path, executable: &std::path::Path) -> Result<(u32, std::path::PathBuf), String> {
    use sha2::{Digest, Sha256};
    use std::io::Read as _;
    if path.parent() != executable.parent()
        || executable.file_name().and_then(|n| n.to_str()) != Some("PdfCraft-update.exe")
        || !path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("resume-") && n.ends_with(".json"))
    {
        return Err("Invalid staged update location".into());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > 1 << 20 {
        return Err("Update session is too large".into());
    }
    let job: serde_json::Value = serde_json::from_reader(file.take((1 << 20) + 1)).map_err(|e| e.to_string())?;
    let parent = job["parent"].as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0).ok_or("Invalid parent process")?;
    let application = std::path::PathBuf::from(job["application"].as_str().ok_or("Missing application")?);
    if !application.is_absolute() || application.file_name().and_then(|n| n.to_str()) != Some("pdfcraft.exe") || !application.is_file() {
        return Err("Invalid application to restart".into());
    }
    let expected_size = job["size"].as_u64().filter(|n| (1..=512 * 1024 * 1024).contains(n)).ok_or("Invalid installer size")?;
    let expected_hash =
        job["sha256"].as_str().filter(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())).ok_or("Invalid installer digest")?;
    let mut installer = std::fs::File::open(executable).map_err(|e| e.to_string())?;
    if installer.metadata().map_err(|e| e.to_string())?.len() != expected_size {
        return Err("The staged installer size changed".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = installer.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(buffer.get(..n).ok_or("Invalid installer chunk")?);
    }
    let actual = hash.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>();
    if !actual.eq_ignore_ascii_case(expected_hash) {
        return Err("The staged installer digest changed".into());
    }
    Ok((parent, application))
}

#[cfg(feature = "bundle")]
fn apply_update(path: &std::path::Path) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let (pid, application) = read_update(path, &executable)?;
    wait_for_parent(pid, &application, &path.with_extension("ready"), 120_000)?;
    let installed = install(&["--quiet".into()]);
    // This process stays unelevated, even if Windows Installer asked for administrator approval.
    let restarted = std::process::Command::new(&application)
        .arg("--resume-update")
        .arg(path)
        .spawn()
        .map_err(|e| format!("Could not reopen Lion PDF: {e}. Your session is saved at {}", path.display()));
    installed.and(restarted.map(|_| ()))
}

#[cfg(any(feature = "bundle", test))]
fn wait_for_parent(pid: u32, application: &std::path::Path, ready: &std::path::Path, timeout: u32) -> Result<(), String> {
    // Hold the actual process handle, so PID reuse cannot make an update wait on a different app.
    match winsafe::HPROCESS::OpenProcess(winsafe::co::PROCESS::SYNCHRONIZE | winsafe::co::PROCESS::QUERY_LIMITED_INFORMATION, false, pid) {
        Ok(parent) => {
            let image = parent.QueryFullProcessImageName(winsafe::co::PROCESS_NAME::WIN32).map_err(|e| e.to_string())?;
            if !image.eq_ignore_ascii_case(&application.to_string_lossy()) {
                return Err("The update parent is not Lion PDF".into());
            }
            std::fs::write(ready, b"ready").map_err(|e| format!("Could not acknowledge the staged update: {e}"))?;
            if parent.WaitForSingleObject(Some(timeout)).map_err(|e| e.to_string())? != winsafe::co::WAIT::OBJECT_0 {
                return Err("Lion PDF did not finish closing; nothing was installed".into());
            }
        }
        Err(e) if e == winsafe::co::ERROR::INVALID_PARAMETER => {
            std::fs::write(ready, b"ready").map_err(|e| e.to_string())?;
        } // It already exited.
        Err(e) => return Err(format!("Could not wait for Lion PDF: {e}")),
    }
    Ok(())
}

#[cfg(test)]
mod update_tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn updater_acknowledges_readiness_but_never_installs_while_parent_is_alive() {
        let folder = tempfile::tempdir().unwrap();
        let ready = folder.path().join("resume-test.ready");
        let app = std::env::current_exe().unwrap();
        let error = wait_for_parent(std::process::id(), &app, &ready, 0).unwrap_err();
        assert!(ready.is_file());
        assert!(error.contains("nothing was installed"));
        std::fs::remove_file(&ready).unwrap();
        assert!(wait_for_parent(std::process::id(), &folder.path().join("other.exe"), &ready, 0).is_err());
        assert!(!ready.exists(), "a mismatched parent never acknowledges readiness");
    }
    #[test]
    fn restart_job_is_bounded_and_rejects_changed_installers() {
        let folder = tempfile::tempdir().unwrap();
        let application = folder.path().join("pdfcraft.exe");
        let executable = folder.path().join("PdfCraft-update.exe");
        let resume = folder.path().join("resume-test.json");
        std::fs::write(&application, b"synthetic app").unwrap();
        std::fs::write(&executable, b"synthetic setup").unwrap();
        let digest = Sha256::digest(b"synthetic setup").iter().map(|b| format!("{b:02x}")).collect::<String>();
        let mut job = serde_json::json!({"parent": 42,"application": application,"session": {},"size":15,"sha256":digest});
        std::fs::write(&resume, job.to_string()).unwrap();
        assert_eq!(read_update(&resume, &executable).unwrap(), (42, application));
        std::fs::write(&executable, b"synthetic other").unwrap();
        assert!(read_update(&resume, &executable).is_err());
        std::fs::write(&executable, b"synthetic setup").unwrap();
        for parent in [serde_json::json!(0), serde_json::json!(-1), serde_json::json!(u64::MAX)] {
            job["parent"] = parent;
            std::fs::write(&resume, job.to_string()).unwrap();
            assert!(read_update(&resume, &executable).is_err());
        }
        assert!(read_update(&folder.path().join("elsewhere.json"), &executable).is_err());
    }
}
