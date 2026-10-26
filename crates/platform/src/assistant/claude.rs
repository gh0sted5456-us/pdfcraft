//! The user's official Claude Code installation owns subscription authentication.
use super::{auth, transport::nonempty};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 4 * 1024 * 1024;
fn command() -> Result<Command, String> {
    let mut command = Command::new(if cfg!(windows) { "claude.exe" } else { "claude" });
    let dir = auth::data_dir()?.join("Claude");
    std::fs::create_dir_all(&dir).map_err(|_| "Couldn't create this user's Claude connection directory")?;
    isolate(&mut command, &dir);
    Ok(command)
}
fn isolate(command: &mut Command, dir: &std::path::Path) {
    command.current_dir(dir).env("CLAUDE_CONFIG_DIR", dir);
    // Never inherit the developer's API key, token, cloud account, or agent configuration.
    for key in [
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "CLAUDECODE",
        "CLAUDE_CODE_USE_BEDROCK",
        "CLAUDE_CODE_USE_VERTEX",
        "CLAUDE_CODE_USE_FOUNDRY",
        "ANTHROPIC_BASE_URL",
    ] {
        command.env_remove(key);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
}
struct Running(std::process::Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn read_limited(mut pipe: impl Read, limit: usize, too_large: Arc<AtomicBool>) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    pipe.by_ref().take(limit.saturating_add(1) as u64).read_to_end(&mut bytes).map_err(|_| "Couldn't read the Claude Code reply")?;
    if bytes.len() > limit {
        too_large.store(true, Ordering::Relaxed);
        return Err("Claude Code output exceeded its safe limit".into());
    }
    Ok(bytes)
}
fn capture(command: &mut Command, input: &str, cancel: &AtomicBool, timeout: Duration) -> Result<Vec<u8>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Claude request canceled before starting the CLI".into());
    }
    let mut child = Running(
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "Claude Code could not start. Install the official native Claude Code CLI, then connect your own subscription.")?,
    );
    let mut stdin = child.0.stdin.take().ok_or("Claude Code did not open its input pipe")?;
    let stdout = child.0.stdout.take().ok_or("Claude Code did not open its output pipe")?;
    let stderr = child.0.stderr.take().ok_or("Claude Code did not open its error pipe")?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Claude request canceled before sending PDF text".into());
    }
    let input = input.as_bytes().to_vec();
    let writer = std::thread::Builder::new()
        .name("assistant-input".into())
        .spawn(move || stdin.write_all(&input))
        .map_err(|_| "Couldn't start the Claude input worker")?;
    let large = Arc::new(AtomicBool::new(false));
    let out_flag = large.clone();
    let err_flag = large.clone();
    let reader = std::thread::Builder::new()
        .name("assistant-output".into())
        .spawn(move || read_limited(stdout, MAX_OUTPUT, out_flag))
        .map_err(|_| "Couldn't start the Claude output worker")?;
    let errors = std::thread::Builder::new()
        .name("assistant-errors".into())
        .spawn(move || read_limited(stderr, 64 * 1024, err_flag))
        .map_err(|_| "Couldn't start the Claude error worker")?;
    let deadline = Instant::now() + timeout;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Claude request canceled; a request already sent may still count toward usage".into());
        }
        if large.load(Ordering::Relaxed) {
            return Err("Claude Code output exceeded its safe limit".into());
        }
        if Instant::now() >= deadline {
            return Err("Claude Code timed out; the PDF was kept".into());
        }
        if let Some(status) = child.0.try_wait().map_err(|_| "Couldn't inspect Claude Code status")? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    let wrote = writer.join().map_err(|_| "Claude Code input stopped unexpectedly")?;
    let out = reader.join().map_err(|_| "Claude Code output stopped unexpectedly")??;
    let _ = errors.join().map_err(|_| "Claude Code error output stopped unexpectedly")??;
    if !status.success() {
        return Err("Claude Code did not complete the request. Check your own subscription login, model access and CLI version. No API billing fallback was used.".into());
    }
    wrote.map_err(|_| "The complete reviewed text could not be sent to Claude Code")?;
    Ok(out)
}
pub fn login(cancel: &AtomicBool) -> Result<(), String> {
    capture(command()?.args(["auth", "login"]), "", cancel, Duration::from_secs(300))?;
    Ok(())
}
pub fn logout(cancel: &AtomicBool) -> Result<(), String> {
    capture(command()?.args(["auth", "logout"]), "", cancel, Duration::from_secs(30))?;
    Ok(())
}
pub fn connected(cancel: &AtomicBool) -> Result<bool, String> {
    let bytes = capture(command()?.args(["auth", "status"]), "", cancel, Duration::from_secs(20))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "Claude Code returned an invalid authentication status")?;
    Ok(value["loggedIn"] == true && value["authMethod"] == "claude.ai")
}
fn answer_args() -> [&'static str; 17] {
    [
        "-p",
        "--output-format",
        "json",
        "--tools",
        "",
        "--disallowedTools",
        "*",
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--setting-sources",
        "",
        "--no-session-persistence",
        "--safe-mode",
        "--restricted",
        "--settings",
        "{\"disableAllHooks\":true}",
    ]
}
pub(super) fn answer(model: &str, instructions: &str, prompt: &str, cancel: &AtomicBool) -> Result<String, String> {
    if !connected(cancel)? {
        return Err("Connect your own Claude subscription first".into());
    }
    let bytes =
        capture(command()?.args(answer_args()).args(["--model", model, "--system-prompt", instructions]), prompt, cancel, Duration::from_secs(120))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| "Claude Code returned invalid JSON")?;
    if value["is_error"] == true {
        return Err("Claude Code reported an unsuccessful request. Check your account limits and model access.".into());
    }
    nonempty(value["result"].as_str().ok_or("Claude Code returned no answer text")?.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_uses_only_the_pdfcraft_profile_and_removes_inherited_credentials() {
        let mut command = Command::new("not-executed");
        let dir = std::path::Path::new("synthetic-profile");
        isolate(&mut command, dir);
        assert_eq!(command.get_current_dir(), Some(dir));
        let env: std::collections::HashMap<_, _> = command.get_envs().collect();
        assert_eq!(env.get(std::ffi::OsStr::new("CLAUDE_CONFIG_DIR")), Some(&Some(dir.as_os_str())));
        for name in ["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_CODE_OAUTH_TOKEN", "ANTHROPIC_BASE_URL", "CLAUDECODE"] {
            assert_eq!(env.get(std::ffi::OsStr::new(name)), Some(&None));
        }
    }

    #[test]
    fn subscription_requests_disable_execution_tools_mcp_hooks_and_saved_sessions() {
        let args = answer_args();
        assert!(args.windows(2).any(|a| a == ["--tools", ""]));
        assert!(args.windows(2).any(|a| a == ["--disallowedTools", "*"]));
        assert!(args.contains(&"--strict-mcp-config") && args.contains(&"--safe-mode") && args.contains(&"--restricted"));
        assert!(args.contains(&"--no-session-persistence"));
        assert!(!args.iter().any(|a| a.contains("bypass") || a.contains("skip-permissions")));
    }
    #[test]
    fn canceled_requests_never_start_a_cli_process() {
        let mut command = Command::new("pdfcraft-synthetic-never-executed");
        let error = capture(&mut command, "private words", &AtomicBool::new(true), Duration::from_secs(1)).unwrap_err();
        assert!(error.contains("canceled before starting"));
    }
}
