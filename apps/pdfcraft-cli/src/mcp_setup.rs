//! `pdfcraft-cli mcp-setup`: print the settings that connect an AI assistant to Lion PDF's MCP
//! server. Nothing is registered or started: the person copies the text into their client.

use std::path::{Path, PathBuf};

use pdfcraft_engine::ai_connect::{Connection, Shell, claude_desktop_config};

use super::options::{OptionSpec, flag, validate_options};
use super::{CliError, stdout_line};

pub(crate) const MCP_SETUP_OPTIONS: &[OptionSpec] =
    &[OptionSpec { name: "--folder", takes_value: true }, OptionSpec { name: "--client", takes_value: true }];

/// Which client's settings to print.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Client {
    All,
    Json,
    ClaudeCode,
}

pub(crate) fn mcp_setup(args: &[String]) -> Result<(), CliError> {
    validate_options("mcp-setup", args, MCP_SETUP_OPTIONS)?;
    if let Some(stray) = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && !is_value(args, *i)).map(|(_, a)| a) {
        return Err(format!("mcp-setup: unexpected argument `{stray}`; choose the folder with --folder DIR").into());
    }
    let client = match flag(args, "--client") {
        None | Some("all") => Client::All,
        Some("json" | "claude-desktop") => Client::Json,
        Some("claude-code") => Client::ClaudeCode,
        Some(other) => return Err(format!("mcp-setup: --client is all, claude-desktop, claude-code or json, not `{other}`").into()),
    };
    if args.iter().any(|a| a == "--folder") && flag(args, "--folder").is_none_or(|f| f.is_empty() || f.starts_with("--")) {
        return Err("mcp-setup: --folder needs a folder".to_string().into());
    }
    let folder = folder(flag(args, "--folder"))?;
    let exe = std::env::current_exe().map_err(|e| format!("mcp-setup: can't find this program's location: {e}"))?;
    let connection = Connection::new(Connection::cli_beside(&exe), folder);
    print(&connection, client)
}

/// Whether `args[i]` is the value of the option before it.
fn is_value(args: &[String], i: usize) -> bool {
    i.checked_sub(1).and_then(|p| args.get(p)).is_some_and(|prev| MCP_SETUP_OPTIONS.iter().any(|o| o.takes_value && o.name == prev))
}

/// The folder the assistant may use, made absolute: the current folder when none is given. It
/// must exist, because the server refuses to start with a missing root.
fn folder(given: Option<&str>) -> Result<PathBuf, CliError> {
    let path = match given {
        Some(f) => PathBuf::from(f),
        None => std::env::current_dir().map_err(|e| format!("mcp-setup: can't read the current folder: {e}"))?,
    };
    let absolute = std::path::absolute(&path).map_err(|e| format!("mcp-setup: {}: {e}", path.display()))?;
    if !absolute.is_dir() {
        return Err(format!("mcp-setup: {} is not an existing folder", absolute.display()).into());
    }
    Ok(absolute)
}

fn print(c: &Connection, client: Client) -> Result<(), CliError> {
    if client == Client::Json {
        return stdout_line(format_args!("{}", c.json()));
    }
    if client == Client::ClaudeCode {
        return stdout_line(format_args!("{}", c.claude_code(Shell::native())));
    }
    let desktop = claude_desktop_config().map_or_else(|| "claude_desktop_config.json".to_owned(), |p| p.display().to_string());
    let missing = if Path::new(&c.cli).is_file() {
        String::new()
    } else {
        format!("\nNote: {} was not found; install Lion PDF, or point \"command\" at pdfcraft-cli.\n", c.cli.display())
    };
    stdout_line(format_args!(
        "Lion PDF MCP server: an AI assistant connected with these settings can create, edit and save PDFs\n\
         in {folder} (and nowhere else). Lion PDF starts nothing itself: your AI client runs the server.\n{missing}\n\
         Claude Desktop: add this to {desktop} (Settings > Developer > Edit Config), then restart it:\n\n{json}\n\n\
         Claude Code: run\n\n  {code}\n\n\
         Other MCP clients (Cursor, VS Code, ...): use the same \"mcpServers\" block.",
        folder = c.folder.display(),
        json = c.json(),
        code = c.claude_code(Shell::native()),
    ))
}
