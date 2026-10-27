//! AI calls are explicit, read their own user's connections, and never edit a PDF.
use crate::{Args, Automation, Result, ToolError, failed};
fn bad(message: impl Into<String>) -> ToolError {
    ToolError::InvalidArgs(message.into())
}
use pdfcraft_engine::assistant::{self, Task};
use pdfcraft_platform::assistant::{self as platform, Provider};
use serde_json::{Value, json};

fn provider(a: &Args) -> Result<Provider> {
    Provider::parse(a.str("provider")?).map_err(bad)
}
impl Automation {
    #[cfg(not(target_arch = "wasm32"))]
    fn ai_network(&self) -> Result<()> {
        if self.network {
            Ok(())
        } else {
            Err(failed("AI network access requires explicit permission: start pdfcraft-cli with --network. Nothing was sent."))
        }
    }
    fn ai_pages(&self, a: &Args) -> Result<Vec<usize>> {
        if a.opt_ints("pages")?.is_some() {
            return self.pages(a, "pages");
        }
        let count = self.doc(a)?.info.pages.len();
        if count > assistant::MAX_PAGES {
            return Err(bad("Choose at most 256 pages explicitly for the Assistant"));
        }
        Ok((0..count).collect())
    }
    pub(crate) fn ai_confidential(&mut self, a: &Args) -> Result<Value> {
        let pages = if a.opt_ints("pages")?.is_some() { Some(self.pages(a, "pages")?) } else { None };
        self.apply(
            a,
            pdfcraft_engine::Edit::SetAiConfidential {
                pages,
                confidential: a.opt_bool("confidential")?.ok_or_else(|| bad("confidential must be true or false"))?,
            },
        )
    }
    pub(crate) fn ai_prepare(&mut self, a: &Args) -> Result<Value> {
        let pages = self.ai_pages(a)?;
        let source = assistant::Source::new(self.doc(a)?, &pages).map_err(failed)?;
        let text = source.prepare(|| false).map_err(failed)?;
        Ok(json!({"text":text,"pages":pages.iter().map(|p|p.saturating_add(1)).collect::<Vec<_>>(),"sent":false}))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_assist(&mut self, a: &Args) -> Result<Value> {
        self.ai_network()?;
        let pages = self.ai_pages(a)?;
        self.doc(a)?.check_ai_pages(&pages).map_err(failed)?;
        let provider = provider(a)?;
        let task = Task::parse(a.opt_str("task")?.unwrap_or("summary")).map_err(bad)?;
        let question = a.opt_str("question")?.unwrap_or("").to_string();
        let text = if let Some(text) = a.opt_str("reviewed_text")? {
            text.to_string()
        } else {
            let pages = self.ai_pages(a)?;
            assistant::Source::new(self.doc(a)?, &pages).map_err(failed)?.prepare(|| false).map_err(failed)?
        };
        let prompt = assistant::prompt(task, &question, &text).map_err(bad)?;
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let answer = platform::answer(
            provider,
            a.opt_str("account")?.unwrap_or(""),
            a.opt_str("model")?.unwrap_or(""),
            assistant::INSTRUCTIONS,
            &prompt,
            &cancel,
        )
        .map_err(failed)?;
        Ok(json!({"answer":answer,"provider":provider.key(),"document_changed":false}))
    }
    pub(crate) fn ai_browser_prepare(&mut self, a: &Args) -> Result<Value> {
        let provider = provider(a)?;
        let url = provider.browser_url().ok_or_else(|| bad("Choose a subscription provider for browser chat"))?;
        let pages = self.ai_pages(a)?;
        self.doc(a)?.check_ai_pages(&pages).map_err(failed)?;
        let text = match a.opt_str("reviewed_text")? {
            Some(text) => text.to_string(),
            None => assistant::Source::new(self.doc(a)?, &pages).map_err(failed)?.prepare(|| false).map_err(failed)?,
        };
        let task = Task::parse(a.opt_str("task")?.unwrap_or("summary")).map_err(bad)?;
        let prompt = assistant::prompt(task, a.opt_str("question")?.unwrap_or(""), &text).map_err(bad)?;
        Ok(
            json!({"url":url,"text":format!("{}\n\n{prompt}",assistant::INSTRUCTIONS),"sent":false,"next":"Review, then explicitly paste into your provider chat. No file access or automatic website submission."}),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_copilot_login_start(&mut self, a: &Args) -> Result<Value> {
        self.ai_network()?;
        if self.copilot_login.is_some() {
            return Err(bad("Finish or cancel the pending Microsoft sign-in first"));
        }
        let config = platform::Microsoft365Config {
            client_id: a.str("client_id")?.into(),
            tenant_id: a.str("tenant_id")?.into(),
            cloud: platform::CopilotCloud::parse(a.opt_str("cloud")?.unwrap_or("global")).map_err(bad)?,
        };
        let login = platform::CopilotLogin::begin(&config).map_err(failed)?;
        let result = json!({"authorization_url":login.url,"user_code":login.code,"next":"Sign in yourself to the official Microsoft page, then poll ai_copilot_login_finish. Requires an eligible work/school Copilot license and IT-approved delegated Graph permissions. No PDF text is sent."});
        self.copilot_login = Some(login);
        Ok(result)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_copilot_login_finish(&mut self, a: &Args) -> Result<Value> {
        if a.opt_bool("cancel")?.unwrap_or(false) {
            self.copilot_login = None;
            return Ok(json!({"canceled":true}));
        }
        self.ai_network()?;
        let mut login = self.copilot_login.take().ok_or_else(|| bad("Call ai_copilot_login_start first"))?;
        match login.poll(&std::sync::atomic::AtomicBool::new(false)).map_err(failed)? {
            Some(account) => Ok(json!({"connected":true,"account":account})),
            None => {
                self.copilot_login = Some(login);
                Ok(json!({"connected":false,"pending":true}))
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_accounts(&self) -> Result<Value> {
        Ok(json!({"accounts":platform::accounts().map_err(failed)?}))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_models(&self, a: &Args) -> Result<Value> {
        self.ai_network()?;
        Ok(json!({"models":platform::models(provider(a)?,a.opt_str("account")?.unwrap_or("")).map_err(failed)?}))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_key_connect(&self, a: &Args) -> Result<Value> {
        let provider = provider(a)?;
        let expected = match provider {
            Provider::OpenAi => "OPENAI_API_KEY",
            Provider::Anthropic => "ANTHROPIC_API_KEY",
            _ => return Err(bad("API keys require openai-api or anthropic-api")),
        };
        if a.str("key_env")? != expected {
            return Err(bad(format!("Use only the explicitly selected {expected} environment variable; raw keys are not tool arguments")));
        }
        let key = std::env::var(expected).map_err(|_| failed(format!("{expected} is not set; connect your own key in the Assistant setup")))?;
        platform::save_api_key(provider, &key).map_err(failed)?;
        Ok(json!({"provider":provider.key(),"saved_in_os_store":true}))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_disconnect(&self, a: &Args) -> Result<Value> {
        let provider = provider(a)?;
        let revoked = match provider {
            Provider::ChatGpt => platform::disconnect(a.str("account")?, self.network).map_err(failed)?,
            Provider::ClaudeSubscription => {
                self.ai_network()?;
                platform::claude_logout(&std::sync::atomic::AtomicBool::new(false)).map_err(failed)?;
                true
            }
            Provider::CopilotWork => {
                platform::copilot_disconnect(a.str("account")?).map_err(failed)?;
                false
            }
            Provider::CopilotPersonal => return Err(bad("Sign out inside the Copilot browser profile; no native connection exists")),
            _ => {
                platform::forget_api_key(provider).map_err(failed)?;
                false
            }
        };
        Ok(json!({"disconnected_locally":true,"remote_revocation_confirmed":revoked}))
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_login_start(&mut self, a: &Args) -> Result<Value> {
        self.ai_network()?;
        if self.ai_login.is_some() {
            return Err(failed("A sign-in is already pending. Finish or cancel it before starting another."));
        }
        let login = platform::Login::begin(a.opt_str("account")?).map_err(failed)?;
        let url = login.url.clone();
        self.ai_login = Some(login);
        Ok(
            json!({"authorization_url":url,"next":"Open this official OpenAI URL in your browser, then call ai_login_finish. No PDF text is sent during login."}),
        )
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_login_finish(&mut self, a: &Args) -> Result<Value> {
        if a.opt_bool("cancel")?.unwrap_or(false) {
            self.ai_login = None;
            return Ok(json!({"canceled":true}));
        }
        self.ai_network()?;
        let mut login = self.ai_login.take().ok_or_else(|| bad("No sign-in is pending; call ai_login_start first"))?;
        match login.poll().map_err(failed)? {
            Some(profile) => Ok(json!({"connected":true,"account":profile})),
            None => {
                self.ai_login = Some(login);
                Ok(json!({"connected":false,"pending":true}))
            }
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn ai_claude_login(&self) -> Result<Value> {
        self.ai_network()?;
        platform::claude_login(&std::sync::atomic::AtomicBool::new(false)).map_err(failed)?;
        Ok(json!({"connected":platform::claude_connected(&std::sync::atomic::AtomicBool::new(false)).map_err(failed)?}))
    }
}
