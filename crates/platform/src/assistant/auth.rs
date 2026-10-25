//! Official public-client Sign in with ChatGPT; never reads Codex/ChatGPT app credentials.
use super::{transport, vault};
use openidconnect::{
    AccessTokenHash, AuthType, AuthorizationCode, ClientId, CsrfToken, EndpointMaybeSet, EndpointNotSet, EndpointSet, IssuerUrl, Nonce,
    OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope,
    core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::TcpListener,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

const ISSUER: &str = "https://auth.openai.com";
const RESOURCE: &str = "https://api.openai.com/v1";
const PLAN_SCOPE: &str = "chatgpt.tokens.use.direct";
type Client = CoreClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointMaybeSet, EndpointMaybeSet>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub client_id: String,
    pub subject: String,
    pub email: String,
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
struct Account {
    #[zeroize(skip)]
    profile: Profile,
    access: String,
    refresh: String,
    id_token: String,
    scopes: Vec<String>,
    expires_at: i64,
}
fn record_key(client_id: &str) -> Result<String, String> {
    if client_id.is_empty() || client_id.len() > 128 || !client_id.bytes().all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b)) {
        return Err("The ChatGPT client registration is invalid".into());
    }
    Ok(format!("chatgpt:{client_id}"))
}
fn load(client_id: &str) -> Result<Account, String> {
    let bytes = vault::read(&record_key(client_id)?)?.ok_or("Connect this ChatGPT account first")?;
    let account: Account = serde_json::from_slice(&bytes).map_err(|_| "The saved ChatGPT connection is damaged; reconnect it")?;
    if account.profile.client_id != client_id {
        return Err("The saved ChatGPT registration does not match this account".into());
    }
    Ok(account)
}
fn save(account: &Account) -> Result<(), String> {
    let bytes = Zeroizing::new(serde_json::to_vec(account).map_err(|_| "Couldn't encode the ChatGPT connection")?);
    vault::write(&record_key(&account.profile.client_id)?, &bytes)
}
pub fn accounts() -> Result<Vec<Profile>, String> {
    let Some(bytes) = vault::read("chatgpt-accounts")? else { return Ok(Vec::new()) };
    let accounts: Vec<Profile> = serde_json::from_slice(&bytes).map_err(|_| "The saved ChatGPT account list is damaged")?;
    if accounts.len() > 8 || accounts.iter().any(|p| record_key(&p.client_id).is_err() || p.subject.len() > 512 || p.email.len() > 512) {
        return Err("The saved ChatGPT account list is invalid".into());
    }
    Ok(accounts)
}
fn host_id() -> Result<String, String> {
    if let Some(bytes) = vault::read("host-id")? {
        let id = std::str::from_utf8(&bytes).map_err(|_| "The Assistant host identifier is damaged")?;
        uuid::Uuid::parse_str(id.strip_prefix("urn:uuid:").ok_or("The Assistant host identifier is invalid")?)
            .map_err(|_| "The Assistant host identifier is invalid")?;
        return Ok(id.to_string());
    }
    let id = format!("urn:uuid:{}", uuid::Uuid::new_v4());
    vault::write("host-id", id.as_bytes())?;
    Ok(id)
}
fn metadata() -> Result<CoreProviderMetadata, String> {
    let issuer = IssuerUrl::new(ISSUER.into()).map_err(|_| "Invalid OpenAI issuer")?;
    CoreProviderMetadata::discover(&issuer, &transport::oauth_http)
        .map_err(|_| "Couldn't load OpenAI's sign-in metadata. Check your connection.")
        .map_err(str::to_owned)
}
fn client(metadata: CoreProviderMetadata, id: &str, redirect: Option<&str>) -> Result<Client, String> {
    let mut client = CoreClient::from_provider_metadata(metadata, ClientId::new(id.into()), None).set_auth_type(AuthType::RequestBody);
    if let Some(uri) = redirect {
        client = client.set_redirect_uri(RedirectUrl::new(uri.into()).map_err(|_| "Invalid local sign-in callback")?);
    }
    Ok(client)
}

/// Listener exists only after an explicit Connect action, on an ephemeral loopback port.
pub struct Login {
    listener: TcpListener,
    deadline: Instant,
    state: CsrfToken,
    nonce: Nonce,
    verifier: Option<PkceCodeVerifier>,
    metadata: CoreProviderMetadata,
    redirect: String,
    selected: Option<Profile>,
    used: bool,
    pub url: String,
}
impl Login {
    pub fn begin(selected: Option<&str>) -> Result<Self, String> {
        let _lock = connection_lock()?;
        let profile = selected.filter(|s| !s.is_empty()).map(|id| load(id).map(|a| a.profile.clone())).transpose()?;
        if profile.is_none() && accounts()?.len() >= 8 {
            return Err("Disconnect a saved ChatGPT account before adding another (limit 8)".into());
        }
        let host = host_id()?;
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "Couldn't start the local ChatGPT sign-in callback")?;
        listener.set_nonblocking(true).map_err(|_| "Couldn't configure the local sign-in callback")?;
        let redirect = format!("http://127.0.0.1:{}/auth/callback", listener.local_addr().map_err(|_| "Couldn't inspect the callback port")?.port());
        let metadata = metadata()?;
        let id = profile.as_ref().map_or("dynamic_agent_client", |p| p.client_id.as_str());
        let client = client(metadata.clone(), id, Some(&redirect))?;
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut request = client
            .authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random)
            .set_pkce_challenge(challenge)
            .add_extra_param("ext_agent_host_id", host)
            .add_extra_param("resource", RESOURCE);
        for scope in ["profile", "email", "offline_access", "resource.invoke", PLAN_SCOPE] {
            request = request.add_scope(Scope::new(scope.into()));
        }
        if profile.is_none() {
            request = request.add_extra_param("agent_name_hint", "PdfCraft");
        }
        let (url, state, nonce) = request.url();
        Ok(Self {
            listener,
            deadline: Instant::now() + Duration::from_secs(300),
            state,
            nonce,
            verifier: Some(verifier),
            metadata,
            redirect,
            selected: profile,
            used: false,
            url: url.to_string(),
        })
    }
    /// Poll from a worker. Tokens and the authorization code never enter the UI or tool output.
    pub fn poll(&mut self) -> Result<Option<Profile>, String> {
        if self.used {
            return Err("This sign-in attempt has already finished; start a new one".into());
        }
        if Instant::now() >= self.deadline {
            self.used = true;
            return Err("ChatGPT sign-in timed out; try again".into());
        }
        let (mut stream, peer) = match self.listener.accept() {
            Ok(value) => value,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
            Err(_) => return Err("The local sign-in callback stopped".into()),
        };
        if !peer.ip().is_loopback() {
            return Ok(None);
        }
        stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|_| "Couldn't read the sign-in callback")?;
        stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|_| "Couldn't respond to the sign-in callback")?;
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while request.len() < 8192 && !request.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(1) => request.push(byte[0]),
                _ => break,
            }
        }
        let callback = parse_callback(&request, self.state.secret(), self.selected.as_ref().map(|p| p.client_id.as_str()));
        let response = if callback.is_ok() {
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nReturn to PdfCraft to finish connecting."
        } else {
            "HTTP/1.1 400 Bad Request\r\nCache-Control: no-store\r\nConnection: close\r\n\r\nInvalid sign-in callback."
        };
        let _ = stream.write_all(response.as_bytes());
        let Some((code, issued)) = callback? else {
            self.used = true;
            return Err("ChatGPT sign-in was not authorized".into());
        };
        self.used = true;
        let verifier = self.verifier.take().ok_or("The sign-in attempt has expired")?;
        let client = client(self.metadata.clone(), &issued, Some(&self.redirect))?;
        let response = client
            .exchange_code(AuthorizationCode::new(code))
            .map_err(|_| "OpenAI did not provide a token endpoint")?
            .set_pkce_verifier(verifier)
            .add_extra_param("resource", RESOURCE)
            .request(&transport::oauth_http)
            .map_err(|_| "OpenAI rejected the sign-in exchange. Start a fresh sign-in.")?;
        let id_token = response.extra_fields().id_token().ok_or("OpenAI did not return a verified identity token")?;
        let verifier = client.id_token_verifier();
        let claims = id_token
            .claims(&verifier, &self.nonce)
            .map_err(|_| "OpenAI identity validation failed (signature, issuer, audience, expiry or nonce)")?;
        if let Some(expected) = claims.access_token_hash() {
            let actual = AccessTokenHash::from_token(
                response.access_token(),
                id_token.signing_alg().map_err(|_| "Invalid identity algorithm")?,
                id_token.signing_key(&verifier).map_err(|_| "Invalid identity signing key")?,
            )
            .map_err(|_| "Invalid access-token binding")?;
            if actual != *expected {
                return Err("The identity and access token do not match".into());
            }
        }
        let profile = Profile {
            client_id: issued,
            subject: claims.subject().as_str().into(),
            email: claims.email().map_or_else(String::new, |e| e.as_str().into()),
        };
        if profile.subject.is_empty() || self.selected.as_ref().is_some_and(|p| p.subject != profile.subject || p.client_id != profile.client_id) {
            return Err("The verified ChatGPT account differs from the selected account".into());
        }
        let scopes: Vec<_> = response.scopes().map_or_else(Vec::new, |s| s.iter().map(|s| s.as_str().to_string()).collect());
        if !scopes.iter().any(|s| s == PLAN_SCOPE) {
            return Err("ChatGPT plan usage was not granted. Reconnect and allow plan usage, or choose a personal API key explicitly.".into());
        }
        let expiry = response.expires_in().ok_or("OpenAI did not provide the token expiry")?.as_secs().min(86400) as i64;
        let account = Account {
            profile: profile.clone(),
            access: response.access_token().secret().clone(),
            refresh: response.refresh_token().map_or_else(String::new, |t| t.secret().clone()),
            id_token: id_token.to_string(),
            scopes,
            expires_at: chrono::Utc::now().timestamp().saturating_add(expiry),
        };
        let _lock = connection_lock()?;
        let mut profiles = accounts()?;
        if !profiles.iter().any(|p| p.client_id == profile.client_id) {
            profiles.push(profile.clone());
        }
        save(&account)?;
        vault::write("chatgpt-accounts", &serde_json::to_vec(&profiles).map_err(|_| "Couldn't save ChatGPT account metadata")?)?;
        Ok(Some(profile))
    }
}

fn parse_callback(request: &[u8], expected_state: &str, selected: Option<&str>) -> Result<Option<(String, String)>, String> {
    let text = std::str::from_utf8(request).map_err(|_| "Invalid sign-in callback")?;
    let mut parts = text.lines().next().unwrap_or("").split_whitespace();
    if parts.next() != Some("GET") {
        return Err("Invalid sign-in callback method".into());
    }
    let path = parts.next().ok_or("Invalid sign-in callback path")?;
    if !path.starts_with("/auth/callback?") || !text.ends_with("\r\n\r\n") {
        return Err("Invalid sign-in callback path or size".into());
    }
    let uri = url::Url::parse(&format!("http://127.0.0.1{path}")).map_err(|_| "Invalid sign-in callback")?;
    let mut params = std::collections::HashMap::new();
    for (key, value) in uri.query_pairs() {
        if params.insert(key.into_owned(), value.into_owned()).is_some() {
            return Err("Repeated sign-in callback parameter".into());
        }
    }
    let state = params.get("state").ok_or("Missing sign-in state")?;
    if !bool::from(state.as_bytes().ct_eq(expected_state.as_bytes())) {
        return Err("The sign-in state did not match".into());
    }
    if params.contains_key("error") {
        return Ok(None);
    }
    let code = params.remove("code").filter(|c| !c.is_empty() && c.len() <= 4096).ok_or("Missing sign-in authorization code")?;
    let issued = params.remove("client_id").or_else(|| selected.map(str::to_string)).ok_or("OpenAI did not issue a client registration")?;
    record_key(&issued)?;
    if issued == "dynamic_agent_client" || selected.is_some_and(|s| s != issued) {
        return Err("The sign-in client registration did not match".into());
    }
    Ok(Some((code, issued)))
}

pub(super) fn access_token(client_id: &str) -> Result<Zeroizing<String>, String> {
    let _lock = connection_lock()?;
    let mut account = load(client_id)?;
    if !account.scopes.iter().any(|s| s == PLAN_SCOPE) {
        return Err("This connection did not grant ChatGPT plan usage".into());
    }
    if account.expires_at <= chrono::Utc::now().timestamp().saturating_add(60) {
        if account.refresh.is_empty() {
            return Err("Reconnect this ChatGPT account to renew its session".into());
        }
        let client = client(metadata()?, client_id, None)?;
        let response = client
            .exchange_refresh_token(&RefreshToken::new(account.refresh.clone()))
            .map_err(|_| "OpenAI did not provide a token endpoint")?
            .add_extra_param("resource", RESOURCE)
            .request(&transport::oauth_http)
            .map_err(|_| "Couldn't renew ChatGPT authorization. Reconnect if the session has expired.")?;
        if let Some(id) = response.extra_fields().id_token() {
            let verifier = client.id_token_verifier();
            let claims = id.claims(&verifier, |_: Option<&Nonce>| Ok(())).map_err(|_| "The renewed OpenAI identity could not be verified")?;
            if let Some(expected) = claims.access_token_hash() {
                let actual = AccessTokenHash::from_token(
                    response.access_token(),
                    id.signing_alg().map_err(|_| "Invalid identity algorithm")?,
                    id.signing_key(&verifier).map_err(|_| "Invalid identity signing key")?,
                )
                .map_err(|_| "Invalid access-token binding")?;
                if actual != *expected {
                    return Err("The identity and access token do not match".into());
                }
            }
            if claims.subject().as_str() != account.profile.subject {
                return Err("The renewed OpenAI account changed unexpectedly".into());
            }
            account.id_token = id.to_string();
        }
        if let Some(scopes) = response.scopes() {
            account.scopes = scopes.iter().map(|s| s.as_str().to_string()).collect();
        }
        if !account.scopes.iter().any(|s| s == PLAN_SCOPE) {
            return Err("ChatGPT plan usage is no longer authorized".into());
        }
        account.access = response.access_token().secret().clone();
        if let Some(refresh) = response.refresh_token() {
            account.refresh = refresh.secret().clone();
        }
        let expiry = response.expires_in().ok_or("OpenAI did not provide the renewed token expiry")?.as_secs().min(86400) as i64;
        account.expires_at = chrono::Utc::now().timestamp().saturating_add(expiry);
        save(&account)?;
    }
    Ok(Zeroizing::new(account.access.clone()))
}

/// Returns whether the remote renewable session was confirmed revoked; local tokens are removed.
pub fn disconnect(client_id: &str, remote: bool) -> Result<bool, String> {
    let _lock = connection_lock()?;
    let account = load(client_id)?;
    let revoked = remote && transport::revoke(client_id, &account.refresh).is_ok();
    vault::remove(&record_key(client_id)?)?;
    let profiles: Vec<_> = accounts()?.into_iter().filter(|p| p.client_id != client_id).collect();
    vault::write("chatgpt-accounts", &serde_json::to_vec(&profiles).map_err(|_| "Couldn't update ChatGPT account metadata")?)?;
    Ok(revoked)
}

pub(super) fn data_dir() -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "windows")]
    let base = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".local/share")));
    let path = base.ok_or("Couldn't find this user's application data directory")?.join("PdfCraft/Assistant");
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&path).map_err(|_| "Couldn't create this user's Assistant directory")?;
    Ok(path)
}
fn connection_lock() -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir()?.join("connection.lock"))
        .map_err(|_| "Couldn't open the Assistant connection lock")?;
    file.try_lock().map_err(|_| "Another PdfCraft Assistant is updating this connection. Try again shortly.")?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(query: &str) -> Vec<u8> {
        format!("GET /auth/callback?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").into_bytes()
    }
    #[test]
    fn callback_requires_fresh_state_and_the_exact_registered_client() {
        assert_eq!(
            parse_callback(&request("code=c&state=fresh&client_id=oaiapp_test"), "fresh", None).unwrap(),
            Some(("c".into(), "oaiapp_test".into()))
        );
        assert!(parse_callback(&request("code=c&state=other&client_id=oaiapp_test"), "fresh", None).is_err());
        assert!(parse_callback(&request("code=c&state=fresh&state=fresh&client_id=oaiapp_test"), "fresh", None).is_err());
        assert!(parse_callback(&request("code=c&state=fresh&client_id=oaiapp_other"), "fresh", Some("oaiapp_test")).is_err());
        assert!(parse_callback(&request("code=c&state=fresh&client_id=dynamic_agent_client"), "fresh", None).is_err());
        assert_eq!(parse_callback(&request("code=c&state=fresh"), "fresh", Some("oaiapp_test")).unwrap(), Some(("c".into(), "oaiapp_test".into())));
        assert!(parse_callback(&request("error=access_denied&state=wrong"), "fresh", None).is_err());
        assert_eq!(parse_callback(&request("error=access_denied&state=fresh"), "fresh", None).unwrap(), None);
    }
}
