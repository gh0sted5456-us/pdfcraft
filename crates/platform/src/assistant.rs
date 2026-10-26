//! Per-user AI connections. No bundled account, implicit environment key, or automatic request.
#[cfg(not(target_arch = "wasm32"))]
mod auth;
#[cfg(not(target_arch = "wasm32"))]
mod claude;
#[cfg(not(target_arch = "wasm32"))]
mod copilot;
#[cfg(not(target_arch = "wasm32"))]
mod transport;
#[cfg(not(target_arch = "wasm32"))]
mod vault;
#[cfg(not(target_arch = "wasm32"))]
pub use auth::{Login, Profile, accounts, disconnect};
#[cfg(not(target_arch = "wasm32"))]
pub use claude::{connected as claude_connected, login as claude_login, logout as claude_logout};
#[cfg(not(target_arch = "wasm32"))]
pub use copilot::{CopilotLogin, connected as copilot_connected, disconnect as copilot_disconnect};
#[cfg(not(target_arch = "wasm32"))]
pub use transport::{answer, api_key_present, forget_api_key, models, save_api_key};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    ChatGpt,
    ClaudeSubscription,
    OpenAi,
    Anthropic,
    CopilotPersonal,
    CopilotWork,
}
impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::ChatGpt => "ChatGPT subscription",
            Self::ClaudeSubscription => "Claude subscription (Claude Code)",
            Self::OpenAi => "OpenAI API (separate billing)",
            Self::Anthropic => "Claude API (separate billing)",
            Self::CopilotPersonal => "Microsoft Copilot (personal)",
            Self::CopilotWork => "Microsoft 365 Copilot (work/school)",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::ClaudeSubscription => "claude-subscription",
            Self::OpenAi => "openai-api",
            Self::Anthropic => "anthropic-api",
            Self::CopilotPersonal => "copilot-personal",
            Self::CopilotWork => "copilot-work",
        }
    }
    pub fn requires_model(self) -> bool {
        !matches!(self, Self::CopilotPersonal | Self::CopilotWork)
    }
    pub fn browser_url(self) -> Option<&'static str> {
        match self {
            Self::ChatGpt => Some("https://chatgpt.com/"),
            Self::ClaudeSubscription => Some("https://claude.ai/"),
            Self::CopilotPersonal => Some("https://copilot.microsoft.com/"),
            Self::CopilotWork => Some("https://m365.cloud.microsoft/chat"),
            _ => None,
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chatgpt" => Ok(Self::ChatGpt),
            "claude-subscription" => Ok(Self::ClaudeSubscription),
            "openai-api" => Ok(Self::OpenAi),
            "anthropic-api" => Ok(Self::Anthropic),
            "copilot-personal" => Ok(Self::CopilotPersonal),
            "copilot-work" => Ok(Self::CopilotWork),
            _ => Err("Choose chatgpt, claude-subscription, copilot-personal, copilot-work, openai-api or anthropic-api".into()),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Model {
    pub id: String,
    pub label: String,
}

pub fn check_model(model: &str) -> Result<(), String> {
    if model.is_empty() || model.len() > 160 || model.starts_with('-') || !model.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b)) {
        return Err("Choose a model from your account, or enter its model ID (at most 160 letters, digits, - _ . : /)".into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CopilotCloud {
    #[default]
    Global,
    Government,
    Defense,
}
impl CopilotCloud {
    pub fn label(self) -> &'static str {
        match self {
            Self::Global => "Microsoft 365 global",
            Self::Government => "US Government (GCC High)",
            Self::Defense => "US Government (DoD)",
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "global" => Ok(Self::Global),
            "government" => Ok(Self::Government),
            "defense" => Ok(Self::Defense),
            _ => Err("Choose global, government or defense".into()),
        }
    }
}
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Microsoft365Config {
    pub client_id: String,
    pub tenant_id: String,
    pub cloud: CopilotCloud,
}

/// Browser profiles are separate from API tokens and from every installed browser's profile.
#[cfg(not(target_arch = "wasm32"))]
pub fn browser_data_dir(provider: Provider) -> Result<std::path::PathBuf, String> {
    if provider.browser_url().is_none() {
        return Err("Choose a subscription provider for browser chat".into());
    }
    let path = auth::data_dir()?.join("Browser").join(provider.key());
    std::fs::create_dir_all(&path).map_err(|_| "Couldn't create this user's browser profile")?;
    Ok(path)
}
/// Top-level navigation only; remote websites receive no PdfCraft file/IPC bridge.
#[cfg(not(target_arch = "wasm32"))]
pub fn browser_origin(provider: Provider, address: &str) -> Option<String> {
    if address.len() > 16_384 {
        return None;
    }
    let url = url::Url::parse(address).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() || url.port().is_some_and(|p| p != 443) {
        return None;
    }
    let host = url.host_str()?;
    let under = |root: &str| host == root || host.strip_suffix(root).is_some_and(|prefix| prefix.ends_with('.'));
    let provider_host = match provider {
        Provider::ChatGpt => under("chatgpt.com") || under("openai.com"),
        Provider::ClaudeSubscription => under("claude.ai") || under("anthropic.com"),
        Provider::CopilotPersonal | Provider::CopilotWork => {
            under("microsoft.com")
                || under("microsoftonline.com")
                || under("microsoftonline.us")
                || under("live.com")
                || host == "m365.cloud.microsoft"
        }
        _ => false,
    };
    let identity = [
        "accounts.google.com",
        "appleid.apple.com",
        "account.apple.com",
        "login.live.com",
        "account.live.com",
        "login.microsoftonline.com",
        "login.microsoftonline.us",
    ]
    .contains(&host);
    (provider.browser_url().is_some() && (provider_host || identity)).then(|| url.origin().ascii_serialization())
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod browser_tests {
    use super::*;
    #[test]
    fn browser_connections_are_fixed_https_origins_without_privileged_schemes() {
        for provider in [Provider::ChatGpt, Provider::ClaudeSubscription, Provider::CopilotPersonal, Provider::CopilotWork] {
            assert!(browser_origin(provider, provider.browser_url().unwrap()).is_some());
            for bad in [
                "file:///C:/private.pdf",
                "http://chatgpt.com/",
                "https://chatgpt.com.evil.example/",
                "javascript:alert(1)",
                "https://x@chatgpt.com/",
                "https://chatgpt.com:444/",
                "https://evil.example/?token=secret",
            ] {
                assert!(browser_origin(provider, bad).is_none(), "{provider:?} {bad}");
            }
        }
        assert_eq!(
            browser_origin(Provider::ChatGpt, "https://auth.openai.com/authorize?secret=never-display"),
            Some("https://auth.openai.com".into())
        );
        assert!(browser_origin(Provider::OpenAi, "https://accounts.google.com").is_none());
        assert!(!Provider::CopilotPersonal.requires_model());
    }
}
