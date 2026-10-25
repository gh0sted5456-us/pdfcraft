use super::{Model, Provider, auth, check_model, vault};
use serde_json::{Value, json};
use std::{sync::atomic::AtomicBool, time::Duration};
use zeroize::Zeroizing;

const MAX_REPLY: u64 = 4 * 1024 * 1024;
pub(super) fn agent() -> ureq::Agent {
    let roots: Vec<_> =
        rustls_native_certs::load_native_certs().certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .max_redirects(0)
        .http_status_as_error(false)
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::new_with_certs(&roots)).build())
        .build()
        .new_agent()
}
pub(super) fn oauth_http(request: openidconnect::HttpRequest) -> Result<openidconnect::HttpResponse, std::io::Error> {
    if request.uri().scheme_str() != Some("https") || request.uri().host() != Some("auth.openai.com") {
        return Err(std::io::Error::other("OpenAI authorization must use its official HTTPS host"));
    }
    let response = agent().run(request).map_err(|_| std::io::Error::other("OpenAI authorization request failed"))?;
    let (parts, mut body) = response.into_parts();
    let bytes =
        body.with_config().limit(MAX_REPLY).read_to_vec().map_err(|_| std::io::Error::other("Invalid or oversized authorization response"))?;
    Ok(openidconnect::http::Response::from_parts(parts, bytes))
}
pub(super) fn revoke(client_id: &str, refresh: &str) -> Result<(), String> {
    let mut response =
        agent().get("https://auth.openai.com/.well-known/openid-configuration").call().map_err(|_| "Couldn't discover OpenAI session revocation")?;
    let bytes = response.body_mut().with_config().limit(MAX_REPLY).read_to_vec().map_err(|_| "Couldn't read OpenAI session metadata")?;
    let metadata: Value = serde_json::from_slice(&bytes).map_err(|_| "Invalid OpenAI session metadata")?;
    let endpoint = metadata["revocation_endpoint"].as_str().ok_or("OpenAI did not publish a session revocation endpoint")?;
    let uri: url::Url = endpoint.parse().map_err(|_| "Invalid OpenAI session revocation endpoint")?;
    if uri.scheme() != "https" || uri.host_str() != Some("auth.openai.com") || !uri.username().is_empty() || uri.password().is_some() {
        return Err("Invalid OpenAI session revocation host".into());
    }
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([("token", refresh), ("token_type_hint", "refresh_token"), ("client_id", client_id)])
        .finish();
    let response = agent()
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(body)
        .map_err(|_| "Couldn't revoke the remote ChatGPT session")?;
    if response.status() != 200 {
        return Err("Remote ChatGPT session revocation was not confirmed".into());
    }
    Ok(())
}
fn api_provider(provider: Provider) -> Result<(), String> {
    if !matches!(provider, Provider::OpenAi | Provider::Anthropic) {
        return Err("Select an API provider to manage its personal API key".into());
    }
    Ok(())
}
pub fn save_api_key(provider: Provider, key: &str) -> Result<(), String> {
    api_provider(provider)?;
    let key = key.trim();
    if key.len() < 16 || key.len() > 8192 || !key.bytes().all(|b| (33..=126).contains(&b)) {
        return Err("Enter a valid personal API key (16–8192 printable characters, without spaces)".into());
    }
    vault::write(provider.key(), key.as_bytes())
}
pub fn forget_api_key(provider: Provider) -> Result<(), String> {
    api_provider(provider)?;
    vault::remove(provider.key())
}
pub fn api_key_present(provider: Provider) -> Result<bool, String> {
    api_provider(provider)?;
    Ok(vault::read(provider.key())?.is_some())
}
fn credential(provider: Provider, account: &str) -> Result<Zeroizing<String>, String> {
    if provider == Provider::ChatGpt {
        return auth::access_token(account);
    }
    api_provider(provider)?;
    let bytes = vault::read(provider.key())?.ok_or("Connect your own personal API key first")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "The saved API key is damaged; reconnect it")?;
    Ok(Zeroizing::new(text.to_string()))
}
fn read_reply(mut response: ureq::http::Response<ureq::Body>) -> Result<Vec<u8>, String> {
    let status = response.status().as_u16();
    if status != 200 {
        return Err(match status {
            401 | 403 => {
                "Your AI connection was rejected. Check this account's authorization and model access; no other account or API billing was used."
                    .into()
            }
            429 => {
                "This account reached its AI usage or rate limit. Try later or explicitly choose another connection; no billing fallback was used."
                    .into()
            }
            _ => format!("The AI provider returned HTTP {status}. Nothing in the PDF was changed."),
        });
    }
    response.body_mut().with_config().limit(MAX_REPLY).read_to_vec().map_err(|_| "The AI reply exceeded 4 MB or could not be read".into())
}
pub fn models(provider: Provider, account: &str) -> Result<Vec<Model>, String> {
    if provider == Provider::ClaudeSubscription {
        return Ok(vec![
            Model { id: "sonnet".into(), label: "Sonnet (Claude Code)".into() },
            Model { id: "opus".into(), label: "Opus (Claude Code)".into() },
            Model { id: "haiku".into(), label: "Haiku (Claude Code)".into() },
        ]);
    }
    let key = credential(provider, account)?;
    let agent = agent();
    let response = if provider == Provider::Anthropic {
        agent.get("https://api.anthropic.com/v1/models").header("x-api-key", key.as_str()).header("anthropic-version", "2023-06-01").call()
    } else {
        agent.get("https://api.openai.com/v1/models").header("Authorization", &format!("Bearer {}", key.as_str())).call()
    }
    .map_err(|_| "Couldn't contact the selected AI provider")?;
    let value: Value = serde_json::from_slice(&read_reply(response)?).map_err(|_| "The provider returned an invalid model catalog")?;
    parse_models(&value, provider == Provider::ChatGpt)
}
fn parse_models(value: &Value, subscription: bool) -> Result<Vec<Model>, String> {
    let array = value[if subscription { "models" } else { "data" }].as_array().ok_or("The provider did not return a model catalog")?;
    let mut out = Vec::new();
    for item in array.iter().take(2000) {
        if subscription && item["visibility"] != "list" {
            continue;
        }
        let Some(id) = item[if subscription { "slug" } else { "id" }].as_str().filter(|id| check_model(id).is_ok()) else { continue };
        let label = item["display_name"].as_str().filter(|s| s.len() <= 200).unwrap_or(id);
        out.push(Model { id: id.into(), label: label.into() });
    }
    if out.is_empty() {
        return Err("No usable models are available to this account".into());
    }
    Ok(out)
}
pub fn answer(provider: Provider, account: &str, model: &str, instructions: &str, prompt: &str, cancel: &AtomicBool) -> Result<String, String> {
    check_model(model)?;
    if instructions.len() > 16000 || prompt.is_empty() || prompt.len() > 220000 {
        return Err("The Assistant request is empty or too large".into());
    }
    if provider == Provider::ClaudeSubscription {
        return super::claude::answer(model, instructions, prompt, cancel);
    }
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Assistant request canceled".into());
    }
    let key = credential(provider, account)?;
    let (endpoint, body) = request(provider, model, instructions, prompt);
    let body = serde_json::to_vec(&body).map_err(|_| "Couldn't encode the Assistant request")?;
    let agent = agent();
    let request = agent.post(endpoint).header("Content-Type", "application/json");
    let request = if provider == Provider::Anthropic {
        request.header("x-api-key", key.as_str()).header("anthropic-version", "2023-06-01")
    } else {
        request.header("Authorization", &format!("Bearer {}", key.as_str()))
    };
    let response = request.send(body).map_err(|_| "Couldn't reach the selected AI provider. The document was kept.")?;
    let bytes = read_reply(response)?;
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Assistant request canceled; a request already sent may still count toward provider usage".into());
    }
    if provider == Provider::Anthropic {
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| "Claude returned invalid JSON")?;
        let text = value["content"]
            .as_array()
            .map(|a| a.iter().filter(|v| v["type"] == "text").filter_map(|v| v["text"].as_str()).collect::<String>())
            .unwrap_or_default();
        if value["stop_reason"] == "max_tokens" {
            return Err("Claude reached the output limit; choose fewer pages or a narrower request".into());
        }
        nonempty(text)
    } else {
        parse_stream(&bytes)
    }
}
fn request(provider: Provider, model: &str, instructions: &str, prompt: &str) -> (&'static str, Value) {
    if provider == Provider::Anthropic {
        (
            "https://api.anthropic.com/v1/messages",
            json!({"model":model,"max_tokens":4096,"system":instructions,"messages":[{"role":"user","content":prompt}]}),
        )
    } else {
        (
            "https://api.openai.com/v1/responses",
            json!({"model":model,"instructions":instructions,"input":[{"role":"user","content":prompt}],"store":false,"stream":true,"tools":[]}),
        )
    }
}
pub(super) fn nonempty(text: String) -> Result<String, String> {
    if text.trim().is_empty() { Err("The AI provider returned no text".into()) } else { Ok(text) }
}
fn parse_stream(bytes: &[u8]) -> Result<String, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "The AI stream was not valid UTF-8")?.replace("\r\n", "\n");
    let mut answer = String::new();
    let mut completed = false;
    for block in text.split("\n\n") {
        let data = block.lines().filter_map(|line| line.strip_prefix("data:").map(str::trim_start)).collect::<Vec<_>>().join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let event: Value = serde_json::from_str(&data).map_err(|_| "The AI stream contained an invalid event")?;
        match event["type"].as_str() {
            Some("response.output_text.delta") => {
                if let Some(delta) = event["delta"].as_str() {
                    answer.push_str(delta)
                }
            }
            Some("response.completed") => {
                completed = true;
            }
            Some("response.failed" | "response.incomplete" | "error") => {
                return Err("The provider did not complete the answer. Try a smaller request; the PDF was not changed.".into());
            }
            _ => {}
        }
    }
    if !completed {
        return Err("The AI connection ended before response.completed; a partial answer was not accepted".into());
    }
    nonempty(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subscriptions_are_explicit_and_never_fall_back_to_api_billing() {
        let (endpoint, body) = request(Provider::ChatGpt, "model", "rules", "reviewed text");
        assert_eq!(endpoint, "https://api.openai.com/v1/responses");
        assert_eq!(body["store"], false);
        assert_eq!(body["stream"], true);
        assert_eq!(body["tools"], json!([]));
        let catalog = json!({"models":[{"slug":"private","visibility":"hide"},{"slug":"eligible","display_name":"Eligible","visibility":"list"}],"data":[{"id":"api-model"}]});
        assert_eq!(parse_models(&catalog, true).unwrap()[0].id, "eligible");
        assert_eq!(parse_models(&catalog, false).unwrap()[0].id, "api-model");
    }
    #[test]
    fn an_interrupted_or_failed_stream_is_not_a_success() {
        let delta = b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Summary [Page 1]\"}\n\n";
        assert!(parse_stream(delta).is_err());
        let mut full = delta.to_vec();
        full.extend_from_slice(b"data: {\"type\":\"response.completed\"}\n\n");
        assert_eq!(parse_stream(&full).unwrap(), "Summary [Page 1]");
        full.extend_from_slice(b"data: {\"type\":\"response.failed\"}\n\n");
        assert!(parse_stream(&full).is_err());
        assert!(parse_stream(b"data: broken\n\n").is_err());
    }
}
