//! Microsoft 365 Copilot's documented preview API; personal Copilot stays a browser handoff.
use super::{CopilotCloud, Microsoft365Config, transport, vault};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

impl CopilotCloud {
    fn authority(self) -> &'static str {
        match self {
            Self::Global => "https://login.microsoftonline.com",
            _ => "https://login.microsoftonline.us",
        }
    }
    fn graph(self) -> &'static str {
        match self {
            Self::Global => "https://graph.microsoft.com",
            Self::Government => "https://graph.microsoft.us",
            Self::Defense => "https://dod-graph.microsoft.us",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Government => "government",
            Self::Defense => "defense",
        }
    }
}
impl Microsoft365Config {
    pub fn account(&self) -> Result<String, String> {
        let client = uuid::Uuid::parse_str(&self.client_id).map_err(|_| "Enter the application (client) ID supplied by your IT team")?;
        let tenant = uuid::Uuid::parse_str(&self.tenant_id).map_err(|_| "Enter your organization's directory (tenant) ID")?;
        Ok(format!("m365_{}_{}_{}", self.cloud.key(), client.simple(), tenant.simple()))
    }
    fn endpoint(&self, suffix: &str) -> Result<String, String> {
        self.account()?;
        let tenant = uuid::Uuid::parse_str(&self.tenant_id).map_err(|_| "Invalid tenant ID")?;
        Ok(format!("{}/{tenant}/oauth2/v2.0/{suffix}", self.cloud.authority()))
    }
    fn scopes(&self) -> String {
        let mut scopes = vec!["offline_access".to_string()];
        for scope in [
            "Sites.Read.All",
            "Mail.Read",
            "People.Read.All",
            "OnlineMeetingTranscript.Read.All",
            "Chat.Read",
            "ChannelMessage.Read.All",
            "ExternalItem.Read.All",
        ] {
            scopes.push(format!("{}/{scope}", self.cloud.graph()));
        }
        scopes.join(" ")
    }
}
fn record(account: &str) -> Result<String, String> {
    let mut parts = account.split('_');
    let valid = account.len() <= 128
        && parts.next() == Some("m365")
        && matches!(parts.next(), Some("global" | "government" | "defense"))
        && (0..2).all(|_| parts.next().is_some_and(|p| p.len() == 32 && p.bytes().all(|b| b.is_ascii_hexdigit())))
        && parts.next().is_none();
    if !valid {
        return Err("Connect your Microsoft 365 account first".into());
    }
    Ok(format!("copilot:{account}"))
}
static CONNECTION: Mutex<()> = Mutex::new(());
#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct Account {
    #[zeroize(skip)]
    config: Microsoft365Config,
    access: String,
    refresh: String,
    expires_at: i64,
}
fn load(account: &str) -> Result<Account, String> {
    let bytes = vault::read(&record(account)?)?.ok_or("Connect your Microsoft 365 account first")?;
    let saved: Account = serde_json::from_slice(&bytes).map_err(|_| "Reconnect the damaged Microsoft 365 connection")?;
    if saved.config.account()? != account {
        return Err("The Microsoft 365 connection does not match this configuration".into());
    }
    Ok(saved)
}
fn save(account: &Account) -> Result<(), String> {
    let bytes = Zeroizing::new(serde_json::to_vec(account).map_err(|_| "Couldn't encode the Microsoft connection")?);
    vault::write(&record(&account.config.account()?)?, &bytes)
}
fn response(mut response: ureq::http::Response<ureq::Body>, limit: u64) -> Result<(u16, Value), String> {
    let status = response.status().as_u16();
    let bytes =
        Zeroizing::new(response.body_mut().with_config().limit(limit).read_to_vec().map_err(|_| "Microsoft's reply was too large or incomplete")?);
    let value = serde_json::from_slice(&bytes).map_err(|_| "Microsoft returned an unreadable reply")?;
    Ok((status, value))
}
fn tokens(config: &Microsoft365Config, value: &Value, previous_refresh: &str) -> Result<Account, String> {
    let access = value["access_token"].as_str().filter(|s| !s.is_empty() && s.len() <= 32000).ok_or("Microsoft did not return an access token")?;
    if !value["token_type"].as_str().is_some_and(|s| s.eq_ignore_ascii_case("bearer")) {
        return Err("Microsoft returned an unsupported token type".into());
    }
    let refresh = value["refresh_token"].as_str().unwrap_or(previous_refresh);
    if refresh.len() > 32000 {
        return Err("Microsoft's refresh token was too large".into());
    }
    let seconds = value["expires_in"].as_i64().filter(|s| (1..=86400).contains(s)).ok_or("Microsoft returned an invalid token lifetime")?;
    Ok(Account {
        config: config.clone(),
        access: access.into(),
        refresh: refresh.into(),
        expires_at: chrono::Utc::now().timestamp().saturating_add(seconds),
    })
}
fn verification_uri(value: &str) -> Result<String, String> {
    let url = url::Url::parse(value).map_err(|_| "Microsoft returned an invalid sign-in URL")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !matches!(url.host_str(), Some("microsoft.com" | "www.microsoft.com" | "login.microsoftonline.com" | "login.microsoftonline.us"))
    {
        return Err("Microsoft's sign-in URL was not an approved Microsoft address".into());
    }
    Ok(url.into())
}
/// Created only by an explicit Connect action. Device tokens are never logged or persisted.
pub struct CopilotLogin {
    config: Microsoft365Config,
    device: Zeroizing<String>,
    deadline: Instant,
    next_poll: Instant,
    interval: Duration,
    pub url: String,
    pub code: String,
}
impl CopilotLogin {
    pub fn begin(config: &Microsoft365Config) -> Result<Self, String> {
        let scopes = config.scopes();
        let endpoint = config.endpoint("devicecode")?;
        let result = transport::agent()
            .post(&endpoint)
            .send_form([("client_id", config.client_id.as_str()), ("scope", scopes.as_str())])
            .map_err(|_| "Couldn't start Microsoft sign-in")?;
        let (status, value) = response(result, 65536)?;
        if status != 200 {
            return Err("Microsoft sign-in was refused. Check the IT-approved app registration and public-client device sign-in setting.".into());
        }
        let device = value["device_code"].as_str().filter(|s| !s.is_empty() && s.len() <= 16384).ok_or("Microsoft did not return a device code")?;
        let code = value["user_code"]
            .as_str()
            .filter(|s| (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
            .ok_or("Microsoft returned an invalid sign-in code")?;
        let url = verification_uri(value["verification_uri"].as_str().ok_or("Microsoft did not return a sign-in address")?)?;
        let lifetime = value["expires_in"].as_u64().filter(|n| (1..=900).contains(n)).ok_or("Microsoft returned an invalid sign-in lifetime")?;
        let interval = Duration::from_secs(value["interval"].as_u64().unwrap_or(5).clamp(1, 60));
        Ok(Self {
            config: config.clone(),
            device: Zeroizing::new(device.into()),
            deadline: Instant::now() + Duration::from_secs(lifetime),
            next_poll: Instant::now() + interval,
            interval,
            url,
            code: code.into(),
        })
    }
    pub fn poll(&mut self, cancel: &AtomicBool) -> Result<Option<String>, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("Microsoft sign-in canceled".into());
        }
        let now = Instant::now();
        if now >= self.deadline {
            return Err("Microsoft sign-in expired; start again".into());
        }
        if now < self.next_poll {
            return Ok(None);
        }
        let result = transport::agent()
            .post(&self.config.endpoint("token")?)
            .send_form([
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", self.config.client_id.as_str()),
                ("device_code", self.device.as_str()),
            ])
            .map_err(|_| "Couldn't check Microsoft sign-in")?;
        let (status, value) = response(result, 65536)?;
        if status == 200 {
            let account = tokens(&self.config, &value, "")?;
            let _guard = CONNECTION.lock().map_err(|_| "Microsoft connection is busy; try again")?;
            if cancel.load(Ordering::Relaxed) {
                return Err("Microsoft sign-in canceled".into());
            }
            save(&account)?;
            return Ok(Some(self.config.account()?));
        }
        match value["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => self.interval = (self.interval + Duration::from_secs(5)).min(Duration::from_secs(60)),
            Some("authorization_declined" | "access_denied") => return Err("Microsoft sign-in was declined".into()),
            Some("expired_token") => return Err("Microsoft sign-in expired; start again".into()),
            _ => return Err("Microsoft sign-in failed; check your account and organization policy".into()),
        }
        self.next_poll = Instant::now() + self.interval;
        Ok(None)
    }
}
pub fn connected(account: &str) -> Result<bool, String> {
    Ok(vault::read(&record(account)?)?.is_some())
}
pub fn disconnect(account: &str) -> Result<(), String> {
    let _guard = CONNECTION.lock().map_err(|_| "Microsoft connection is busy; try again")?;
    vault::remove(&record(account)?)
}
fn access(account: &str) -> Result<(Microsoft365Config, Zeroizing<String>), String> {
    let _guard = CONNECTION.lock().map_err(|_| "Microsoft connection is busy; try again")?;
    let mut saved = load(account)?;
    if saved.expires_at <= chrono::Utc::now().timestamp().saturating_add(60) {
        if saved.refresh.is_empty() {
            return Err("Reconnect your Microsoft 365 account".into());
        }
        let result = transport::agent()
            .post(&saved.config.endpoint("token")?)
            .send_form([
                ("grant_type", "refresh_token"),
                ("client_id", saved.config.client_id.as_str()),
                ("refresh_token", saved.refresh.as_str()),
                ("scope", saved.config.scopes().as_str()),
            ])
            .map_err(|_| "Couldn't renew Microsoft sign-in")?;
        let (status, value) = response(result, 65536)?;
        if status != 200 {
            return Err("Microsoft sign-in could not be renewed; reconnect your account".into());
        }
        saved = tokens(&saved.config, &value, &saved.refresh)?;
        save(&saved)?;
    }
    Ok((saved.config.clone(), Zeroizing::new(saved.access.clone())))
}
fn chat_body(instructions: &str, prompt: &str) -> Value {
    json!({"message":{"text":format!("{instructions}\n\n{prompt}")},"locationHint":{"timeZone":"UTC"},"contextualResources":{"webContext":{"isWebEnabled":false}}})
}
fn graph(config: &Microsoft365Config, access: &str, path: &str, body: Value) -> Result<Value, String> {
    let bytes = serde_json::to_vec(&body).map_err(|_| "Couldn't encode the Copilot request")?;
    let result = transport::agent()
        .post(format!("{}/beta/copilot/{path}", config.cloud.graph()))
        .header("Authorization", &format!("Bearer {access}"))
        .header("Content-Type", "application/json")
        .send(bytes)
        .map_err(|_| "Couldn't reach Microsoft 365 Copilot")?;
    let (status, value) = response(result, 4 * 1024 * 1024)?;
    match status {
        200..=299 => Ok(value),
        401|403 => Err("Microsoft 365 Copilot requires an eligible license, IT-approved Graph permissions and a current sign-in. Check your connection; no other provider was used.".into()),
        429 => Err("Microsoft 365 Copilot reached a usage or rate limit. Try again later.".into()),
        _ => Err(format!("Microsoft 365 Copilot returned HTTP {status}; the PDF was not changed")),
    }
}
fn answer_text(value: &Value, sent: &str) -> Result<String, String> {
    let messages = value["messages"].as_array().filter(|m| m.len() >= 2).ok_or("Copilot did not return an answer")?;
    let message = messages.last().ok_or("Copilot did not return an answer")?;
    let text = message["text"].as_str().filter(|s| !s.trim().is_empty() && *s != sent).ok_or("Copilot did not return answer text")?;
    let mut out = text.to_string();
    let mut sources = Vec::new();
    if let Some(attributions) = message["attributions"].as_array() {
        for item in attributions.iter().take(50) {
            if let Some(url) = item["seeMoreWebUrl"].as_str().filter(|s| s.len() <= 4096)
                && url::Url::parse(url).is_ok_and(|u| u.scheme() == "https" && u.username().is_empty() && u.password().is_none())
                && !sources.contains(&url)
            {
                sources.push(url);
            }
        }
    }
    if !sources.is_empty() {
        out.push_str("\n\nSources returned by Microsoft:\n");
        out.push_str(&sources.join("\n"));
    }
    Ok(out)
}
pub fn answer(account: &str, instructions: &str, prompt: &str, cancel: &AtomicBool) -> Result<String, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("Copilot request canceled".into());
    }
    let (config, access) = access(account)?;
    let conversation = graph(&config, &access, "conversations", json!({}))?;
    let id = conversation["id"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'))
        .ok_or("Microsoft returned an invalid conversation ID")?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Copilot request canceled before sending PDF text".into());
    }
    let body = chat_body(instructions, prompt);
    let sent = body["message"]["text"].as_str().ok_or("Invalid Copilot message")?;
    let reply = graph(&config, &access, &format!("conversations/{id}/chat"), body.clone())?;
    if cancel.load(Ordering::Relaxed) {
        return Err("Copilot request canceled; Microsoft may already have processed it".into());
    }
    answer_text(&reply, sent)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Microsoft365Config {
        Microsoft365Config {
            client_id: "11111111-1111-1111-1111-111111111111".into(),
            tenant_id: "22222222-2222-2222-2222-222222222222".into(),
            ..Default::default()
        }
    }
    #[test]
    fn configurations_cannot_redirect_authentication_or_mix_clouds() {
        let mut c = config();
        for bad in ["https://evil.example", "../common", "organizations", ""] {
            c.tenant_id = bad.into();
            assert!(c.endpoint("token").is_err());
        }
        c = config();
        for (cloud, host) in [
            (CopilotCloud::Global, "https://graph.microsoft.com"),
            (CopilotCloud::Government, "https://graph.microsoft.us"),
            (CopilotCloud::Defense, "https://dod-graph.microsoft.us"),
        ] {
            c.cloud = cloud;
            assert!(c.scopes().contains(host));
            assert!(record(&c.account().unwrap()).is_ok());
        }
        for bad in ["", "chatgpt:token", "m365_global_../../key_token", "m365_evil_a_b"] {
            assert!(record(bad).is_err());
        }
    }
    #[test]
    fn login_urls_are_official_and_tokens_are_bounded() {
        for bad in
            ["http://microsoft.com", "https://microsoft.com.evil.example", "https://x@microsoft.com", "file:///tmp/key", "https://microsoft.com:444/"]
        {
            assert!(verification_uri(bad).is_err());
        }
        assert!(verification_uri("https://microsoft.com/devicelogin").is_ok());
        let v = json!({"access_token":"test","refresh_token":"refresh","token_type":"Bearer","expires_in":3600});
        assert!(tokens(&config(), &v, "").is_ok());
        let mut invalid = v;
        invalid["expires_in"] = json!(-1);
        assert!(tokens(&config(), &invalid, "").is_err());
    }
    #[test]
    fn chats_disable_web_grounding_and_do_not_accept_an_echo_as_an_answer() {
        let body = chat_body("rules", "reviewed PDF");
        assert_eq!(body["contextualResources"]["webContext"]["isWebEnabled"], false);
        assert!(body["message"]["text"].as_str().unwrap().contains("reviewed PDF"));
        assert!(answer_text(&json!({"messages":[{"text":"prompt"}]}), "prompt").is_err());
        let reply = json!({"messages":[{"text":"prompt"},{"text":"summary","attributions":[{"seeMoreWebUrl":"https://example.org/source"},{"seeMoreWebUrl":"javascript:bad"}]}]});
        let text = answer_text(&reply, "prompt").unwrap();
        assert!(text.contains("summary") && text.contains("https://example.org/source") && !text.contains("javascript:"));
    }
}
