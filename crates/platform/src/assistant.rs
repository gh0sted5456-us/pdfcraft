//! Per-user AI connections. No bundled account, implicit environment key, or automatic request.
#[cfg(not(target_arch = "wasm32"))]
mod auth;
#[cfg(not(target_arch = "wasm32"))]
mod claude;
#[cfg(not(target_arch = "wasm32"))]
mod transport;
#[cfg(not(target_arch = "wasm32"))]
mod vault;
#[cfg(not(target_arch = "wasm32"))]
pub use auth::{Login, Profile, accounts, disconnect};
#[cfg(not(target_arch = "wasm32"))]
pub use claude::{connected as claude_connected, login as claude_login, logout as claude_logout};
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
}
impl Provider {
    pub fn label(self) -> &'static str {
        match self {
            Self::ChatGpt => "ChatGPT subscription",
            Self::ClaudeSubscription => "Claude subscription (Claude Code)",
            Self::OpenAi => "OpenAI API (separate billing)",
            Self::Anthropic => "Claude API (separate billing)",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::ClaudeSubscription => "claude-subscription",
            Self::OpenAi => "openai-api",
            Self::Anthropic => "anthropic-api",
        }
    }
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "chatgpt" => Ok(Self::ChatGpt),
            "claude-subscription" => Ok(Self::ClaudeSubscription),
            "openai-api" => Ok(Self::OpenAi),
            "anthropic-api" => Ok(Self::Anthropic),
            _ => Err("Choose chatgpt, claude-subscription, openai-api or anthropic-api".into()),
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
