//! Lion PDF: connecting an AI assistant to Lion PDF's MCP server.
//!
//! `pdfcraft-cli mcp` (installed beside the app) is an MCP server over stdio that an AI client
//! starts itself; Lion PDF never starts or registers it (AGENTS.md §3). This module writes the
//! settings a person copies into their client: the command, its arguments and the folder the
//! assistant may read and write. Help ▸ Connect AI assistants… and `pdfcraft-cli mcp-setup`
//! show the same text.

use std::path::{Path, PathBuf};

/// The name the server is registered under in an AI client's settings.
pub const SERVER_NAME: &str = "lion-pdf";

/// Where the fork lives, for the MCP server's `websiteUrl`.
pub const PROJECT_PAGE: &str = "https://github.com/gh0sted5456-us/pdfcraft";

/// The command-line program beside the app (`pdfcraft-cli`, `pdfcraft-cli.exe` on Windows).
pub const CLI_NAME: &str = if cfg!(windows) { "pdfcraft-cli.exe" } else { "pdfcraft-cli" };

/// The command line and folder one AI client connection uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// The `pdfcraft-cli` program the client starts.
    pub cli: PathBuf,
    /// The one folder the assistant may read from and write to.
    pub folder: PathBuf,
}

/// How the shell that runs `claude mcp add` quotes an argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shell {
    /// sh, bash and zsh: single quotes.
    Posix,
    /// PowerShell and cmd: double quotes.
    Windows,
}

impl Shell {
    /// The shell people on this system type into.
    pub const fn native() -> Self {
        if cfg!(windows) { Self::Windows } else { Self::Posix }
    }

    fn quote(self, arg: &str) -> String {
        let plain = !arg.is_empty() && arg.chars().all(|c| c.is_ascii_alphanumeric() || "-_./:\\=@+,".contains(c));
        match self {
            _ if plain => arg.to_owned(),
            Self::Posix => format!("'{}'", arg.replace('\'', r"'\''")),
            // Neither PowerShell nor cmd allows `"` in a Windows path, so it never needs escaping.
            Self::Windows => format!("\"{arg}\""),
        }
    }
}

impl Connection {
    pub fn new(cli: impl Into<PathBuf>, folder: impl Into<PathBuf>) -> Self {
        Self { cli: cli.into(), folder: folder.into() }
    }

    /// The `pdfcraft-cli` beside a running executable (the app or the CLI itself).
    pub fn cli_beside(exe: &Path) -> PathBuf {
        exe.parent().map_or_else(|| PathBuf::from(CLI_NAME), |dir| dir.join(CLI_NAME))
    }

    /// `mcp` and the two roots: the assistant reads and writes the chosen folder and nothing else.
    pub fn args(&self) -> Vec<String> {
        let folder = self.folder.display().to_string();
        vec!["mcp".into(), "--automation-read-root".into(), folder.clone(), "--automation-write-root".into(), folder]
    }

    /// The `mcpServers` block Claude Desktop (`claude_desktop_config.json`), Cursor and most other
    /// clients read.
    pub fn json(&self) -> String {
        let args: Vec<String> = self.args().iter().map(|a| json_string(a)).collect();
        format!(
            "{{\n  \"mcpServers\": {{\n    {name}: {{\n      \"command\": {cli},\n      \"args\": [{args}]\n    }}\n  }}\n}}",
            name = json_string(SERVER_NAME),
            cli = json_string(&self.cli.display().to_string()),
            args = args.join(", "),
        )
    }

    /// The command that registers the server with Claude Code for every project (`--scope user`).
    pub fn claude_code(&self, shell: Shell) -> String {
        let mut words = vec!["claude".to_owned(), "mcp".into(), "add".into(), "--scope".into(), "user".into(), SERVER_NAME.into(), "--".into()];
        words.push(shell.quote(&self.cli.display().to_string()));
        words.extend(self.args().iter().map(|a| shell.quote(a)));
        words.join(" ")
    }
}

/// `s` as a JSON string literal (RFC 8259 §7: quote, backslash and control characters escaped).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Where Claude Desktop keeps its settings on this system, for the instructions (Lion PDF only
/// shows the path; it never writes the file).
#[cfg(not(target_arch = "wasm32"))]
pub fn claude_desktop_config() -> Option<PathBuf> {
    let env = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library").join("Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(base.join("Claude").join("claude_desktop_config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_names_the_program_and_confines_it_to_the_folder() {
        let c = Connection::new(r"C:\Program Files\Lion PDF\pdfcraft-cli.exe", r"C:\Users\me\Documents\PDFs");
        let v: serde_json::Value = serde_json::from_str(&c.json()).unwrap();
        let server = &v["mcpServers"]["lion-pdf"];
        assert_eq!(server["command"], r"C:\Program Files\Lion PDF\pdfcraft-cli.exe");
        let args: Vec<&str> = server["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        assert_eq!(args, ["mcp", "--automation-read-root", r"C:\Users\me\Documents\PDFs", "--automation-write-root", r"C:\Users\me\Documents\PDFs"]);
    }

    #[test]
    fn claude_code_quotes_paths_for_each_shell() {
        let c = Connection::new("/Applications/Lion PDF.app/Contents/MacOS/pdfcraft-cli", "/Users/me/it's here");
        assert_eq!(
            c.claude_code(Shell::Posix),
            "claude mcp add --scope user lion-pdf -- '/Applications/Lion PDF.app/Contents/MacOS/pdfcraft-cli' mcp \
             --automation-read-root '/Users/me/it'\\''s here' --automation-write-root '/Users/me/it'\\''s here'"
        );
        let w = Connection::new(r"C:\Program Files\Lion PDF\pdfcraft-cli.exe", r"D:\pdfs");
        assert_eq!(
            w.claude_code(Shell::Windows),
            r#"claude mcp add --scope user lion-pdf -- "C:\Program Files\Lion PDF\pdfcraft-cli.exe" mcp --automation-read-root D:\pdfs --automation-write-root D:\pdfs"#
        );
    }

    #[test]
    fn the_cli_is_found_beside_the_app() {
        let exe = Path::new("/opt/lion-pdf").join("pdfcraft");
        assert_eq!(Connection::cli_beside(&exe), Path::new("/opt/lion-pdf").join(CLI_NAME));
        assert_eq!(Connection::cli_beside(Path::new("")), PathBuf::from(CLI_NAME));
    }
}
