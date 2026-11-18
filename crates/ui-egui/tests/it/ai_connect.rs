//! Lion PDF: Help ▸ Connect AI assistants… shows the MCP settings to copy, confined to one folder.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{By, Queryable};
use pdfcraft_ui_egui::PdfCraftApp;

fn harness(folder: std::path::PathBuf) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.ai_connect_folder = Some(folder);
        app
    });
    h.run_steps(4);
    h
}

fn copied(h: &Harness<'static, PdfCraftApp>) -> Option<String> {
    h.output().platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(t) => Some(t.clone()),
        _ => None,
    })
}

#[test]
fn connect_ai_shows_settings_for_the_chosen_folder_and_copies_them() {
    let folder = std::env::temp_dir().join("lion-pdf-ai-connect-test");
    std::fs::create_dir_all(&folder).unwrap();
    let mut h = harness(folder.clone());
    assert!(h.state_mut().execute("help.connect_ai"));
    h.run_steps(3);
    h.get_by_label("Title: Connect AI assistants");
    h.get_by_label_contains("Lion PDF starts nothing");
    h.get_by_label(&folder.display().to_string());
    assert_eq!(h.query_all(By::new().role(Role::Button).label("Copy")).count(), 2, "Claude Desktop and Claude Code");

    h.query_all(By::new().role(Role::Button).label("Copy")).next().unwrap().click();
    let mut json = None;
    for _ in 0..3 {
        h.step();
        json = json.or_else(|| copied(&h));
    }
    let json = json.expect("Copy puts the Claude Desktop settings on the clipboard");
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let server = &value["mcpServers"]["lion-pdf"];
    assert!(server["command"].as_str().unwrap().contains("pdfcraft-cli"), "{json}");
    let args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let folder = folder.display().to_string();
    assert_eq!(args, ["mcp", "--automation-read-root", folder.as_str(), "--automation-write-root", folder.as_str()]);

    h.query_all(By::new().role(Role::Button).label("Copy")).nth(1).unwrap().scroll_to_me();
    h.run_steps(3);
    h.query_all(By::new().role(Role::Button).label("Copy")).nth(1).unwrap().click();
    let mut command = None;
    for _ in 0..3 {
        h.step();
        command = command.or_else(|| copied(&h));
    }
    let command = command.expect("Copy puts the Claude Code command on the clipboard");
    assert!(command.starts_with("claude mcp add --scope user lion-pdf -- "), "{command}");

    h.get_by_role_and_label(Role::Button, "Done").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none(), "Done closes the dialog");
}

#[test]
fn connect_ai_is_on_the_help_menu() {
    let ids: Vec<&str> = pdfcraft_engine::commands::COMMANDS.iter().map(|c| c.id).collect();
    assert!(ids.contains(&"help.connect_ai"));
}
