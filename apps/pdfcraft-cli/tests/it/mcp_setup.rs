//! Lion PDF: `pdfcraft-cli mcp-setup` prints settings that start a working MCP server, confined to
//! the chosen folder, through which an assistant can build and save a PDF.
#![cfg(feature = "mcp")]

use std::io::Write as _;
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_pdfcraft-cli")).args(args).output().unwrap()
}

/// A fresh folder inside a fresh parent, so a save outside the folder can be looked for.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir().join(format!("pdfcraft-cli-mcp-setup-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("folder")).unwrap();
        Self(base)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("folder")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn request(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn tool(id: u64, name: &str, arguments: Value) -> Value {
    request(id, "tools/call", json!({ "name": name, "arguments": arguments }))
}

#[test]
fn printed_settings_start_a_confined_server_that_builds_a_pdf() {
    let dir = Scratch::new("build");
    let folder = dir.path().canonicalize().unwrap();
    let folder_text = folder.to_str().unwrap();
    let out = cli(&["mcp-setup", "--client", "json", "--folder", folder_text]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let settings: Value = serde_json::from_slice(&out.stdout).unwrap();
    let server = &settings["mcpServers"]["lion-pdf"];
    let args: Vec<String> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_owned()).collect();
    assert_eq!(args, ["mcp", "--automation-read-root", folder_text, "--automation-write-root", folder_text]);
    // The settings name the CLI beside the running program; run exactly that command line.
    let command = server["command"].as_str().unwrap();
    assert_eq!(std::path::Path::new(command).file_name(), std::path::Path::new(env!("CARGO_BIN_EXE_pdfcraft-cli")).file_name(), "{command}");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pdfcraft-cli"))
        .args(&args)
        .current_dir(&folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let steps = [
        request(0, "initialize", json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "0" } })),
        tool(1, "doc_create", json!({ "from": "blank", "paper": "letter" })),
        tool(
            2,
            "page_add_text",
            json!({ "doc": 1, "page": 1, "at": [72, 72], "width": 468, "text": "Made by an assistant", "size": 24, "bold": true }),
        ),
        tool(3, "doc_save", json!({ "doc": 1, "path": "built.pdf" })),
        tool(4, "doc_save", json!({ "doc": 1, "path": dir.path().parent().unwrap().join("escaped.pdf").to_str().unwrap() })),
    ];
    let input: String = steps.iter().map(|s| format!("{s}\n")).collect();
    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let replies: Vec<Value> = String::from_utf8(out.stdout).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "lion-pdf");
    assert_eq!(replies[0]["result"]["serverInfo"]["title"], "Lion PDF");
    assert!(replies[0]["result"]["instructions"].as_str().unwrap().contains("To build a new PDF"));
    for reply in &replies[1..4] {
        assert_ne!(reply["result"]["isError"], true, "{reply}");
    }
    let saved = std::fs::read(folder.join("built.pdf")).unwrap();
    assert!(saved.starts_with(b"%PDF-"));
    assert_eq!(replies[4]["result"]["isError"], true, "a save outside the folder is refused: {}", replies[4]);
    assert!(!dir.path().parent().unwrap().join("escaped.pdf").exists());
}

#[test]
fn claude_code_settings_name_the_server_and_the_folder() {
    let dir = Scratch::new("code");
    let folder = dir.path();
    let folder = folder.to_str().unwrap();
    let out = cli(&["mcp-setup", "--client", "claude-code", "--folder", folder]);
    assert!(out.status.success());
    let line = String::from_utf8(out.stdout).unwrap();
    assert!(line.starts_with("claude mcp add --scope user lion-pdf -- "), "{line}");
    assert!(line.contains("--automation-read-root") && line.contains("--automation-write-root"), "{line}");
    let all = String::from_utf8(cli(&["mcp-setup", "--folder", folder]).stdout).unwrap();
    assert!(all.contains("Claude Desktop") && all.contains("\"lion-pdf\"") && all.contains("claude mcp add"), "{all}");
}

#[test]
fn a_missing_folder_or_unknown_client_is_refused() {
    let out = cli(&["mcp-setup", "--folder", "/definitely/not/a/folder"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("is not an existing folder"));
    let out = cli(&["mcp-setup", "--client", "chatbot"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--client is all, claude-desktop, claude-code or json"));
    let out = cli(&["mcp-setup", "stray"]);
    assert!(!out.status.success());
}
