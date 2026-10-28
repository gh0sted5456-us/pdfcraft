//! pdfcraft-automation — agent control for PdfCraft (architecture §13).
//!
//! - **Layer:** L7. Headless: depends on the engine, never on a UI toolkit.
//! - [`Automation`] is a tool set over an engine [`Session`]: open, inspect, render, extract and
//!   find text, edit pages and metadata, undo/redo, save, combine, extract and split. Every tool
//!   has a JSON Schema ([`tools`]) and takes and returns JSON, so the same table drives the MCP
//!   server ([`mcp`]), `pdfcraft-cli run` and (later) the UI control channel.
//! - Pages are **1-based** in every tool, as people number them. Rectangles are in PDF points
//!   with the origin at the top-left of the displayed page.
//! - An optional root directory confines every path a tool reads or writes. Separately,
//!   [`file_access::AutomationRoots`] (`pdfcraft-cli mcp --automation-read-root` and
//!   `--automation-write-root`) holds a read folder and a write folder, each left out meaning no
//!   access of its kind.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod a11y;
mod batch;
mod comments;
mod content;
mod conventions;
pub mod file_access;
mod forms;
mod hairlines;
mod layers;
mod links;
#[cfg(feature = "mcp")]
pub mod mcp;
mod measure;
mod printing;
mod reading;
mod redact;
mod signing;
mod text_cache;
mod tools;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pdfcraft_engine::{BlockKey, CombineSource, DocId, Document, Edit, Session, TextBlock, block_overlapping, commands};
use pdfcraft_platform::staging::{StagingName, staging_suffixes};
use pdfcraft_render::{PageRenderer, PageText, RenderConfig, RenderRequest, RequestKind};
use serde_json::{Value, json};

pub use pdfcraft_render::Pixels;
pub use tools::{ToolDef, tools};

use file_access::foreign_share;
use text_cache::TextCache;

mod assemble;
mod dispatch;
mod documents;
mod error;
mod json;
mod pages;
mod paths;

pub use dispatch::Content;
pub use error::ToolError;
use error::{Result, failed};
pub(crate) use json::destination;
pub use json::view_rect;
use json::{Args, info, summary};
use pages::{DEFAULT_DPI, MAX_DPI, encode_png};
#[cfg(test)]
use pages::{FIND_FIRST_BATCH, FIND_MAX_WARNING_BYTES, FIND_MAX_WARNINGS, extract_parallel};
#[cfg(test)]
pub(crate) use paths::MAX_INPUT;
use paths::{child, lexical, unused, write_atomic};
pub(crate) use paths::{read_input, read_input_bytes};
#[cfg(test)]
use paths::{write_atomic_synced, write_atomic_with};

/// A headless PdfCraft session driven by tool calls.
pub struct Automation {
    pub(crate) network: bool,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) ai_login: Option<pdfcraft_platform::assistant::Login>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) copilot_login: Option<pdfcraft_platform::assistant::CopilotLogin>,
    session: Session,
    root: Option<PathBuf>,
    /// The read and write folders of `--automation-read-root` / `--automation-write-root`.
    roots: Option<Arc<file_access::AutomationRoots>>,
    /// Synchronous renderers, rebuilt when a document's working bytes change.
    renderers: HashMap<DocId, (Arc<Vec<u8>>, PageRenderer)>,
    /// Extracted page text, under one memory budget across all open documents.
    texts: TextCache,
    /// The paragraphs of the last `text_paragraphs` listing of each page, so a paragraph number
    /// keeps naming that paragraph after edits regroup the page (#2516).
    listed: HashMap<(DocId, usize), Vec<BlockKey>>,
    /// Documents that lost a listing to the cap. A page of one with no listing is refused rather
    /// than read as "a number is a position", which the page may no longer match.
    evicted: HashSet<DocId>,
}

impl Default for Automation {
    fn default() -> Self {
        Self::new()
    }
}

impl Automation {
    pub fn new() -> Self {
        Self {
            network: false,
            #[cfg(not(target_arch = "wasm32"))]
            ai_login: None,
            #[cfg(not(target_arch = "wasm32"))]
            copilot_login: None,
            session: {
                let mut session = Session::new();
                session.set_timestamp_certificates(pdfcraft_platform::timestamp::trusted_certificates());
                session
            },
            root: None,
            roots: None,
            renderers: HashMap::new(),
            texts: TextCache::default(),
            listed: HashMap::new(),
            evicted: HashSet::new(),
        }
    }

    /// Confine every path the tools read or write to `root` (relative paths resolve inside it).
    pub fn with_root(mut self, root: impl Into<PathBuf>) -> std::io::Result<Self> {
        let root = root.into().canonicalize()?;
        if !root.is_dir() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the root must be a folder"));
        }
        self.root = Some(root);
        Ok(self)
    }

    /// Confine the files the tools read to the read root and the files they write to the write
    /// root (`None`: no confinement). Applies on top of [`with_root`](Self::with_root) when both
    /// are set.
    pub fn with_automation_roots(mut self, roots: Option<Arc<file_access::AutomationRoots>>) -> Self {
        self.roots = roots;
        self
    }

    /// Use a fixed clock for saves (deterministic output in tests).
    pub fn with_clock(mut self, clock: fn() -> i64) -> Self {
        self.session = std::mem::take(&mut self.session).with_clock(clock);
        self
    }

    /// Multiply the time XFA form scripts may take (tests). See [`Session::with_xfa_time_scale`].
    pub fn with_xfa_time_scale(mut self, factor: u32) -> Self {
        self.session = std::mem::take(&mut self.session).with_xfa_time_scale(factor);
        self
    }

    /// The clock and time zone of a host without its own. See [`Session::with_wall_clock`].
    pub fn with_wall_clock(mut self, clock: fn() -> i64, utc_offset: fn() -> i64) -> Self {
        self.session = std::mem::take(&mut self.session).with_wall_clock(clock, utc_offset);
        self
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Save `bytes` that a caller writes on the session's behalf, such as `pdfcraft-cli run`
    /// saving a rendered page. With a root, it is confined like a tool's own output (a relative
    /// path resolves inside it). Written atomically either way (#2166), except to something
    /// that is not a regular file, such as `/dev/stdout` or a pipe, which is written as given.
    /// Returns where the file went.
    pub fn write_output(&self, path: &str, bytes: &[u8]) -> Result<PathBuf> {
        let target = self.resolve(path, true)?;
        let special = self.root.is_none() && self.roots.is_none() && std::fs::metadata(&target).is_ok_and(|m| !m.is_file() && !m.is_dir());
        if special {
            std::fs::write(&target, bytes).map_err(|e| failed(format!("{path}: {e}")))?;
        } else {
            write_atomic(&target, bytes)?;
        }
        Ok(target)
    }
}

#[cfg(test)]
mod tests;

impl Automation {
    /// Allow explicitly invoked timestamp and AI tools to contact their selected service. Off by default.
    pub fn with_network(mut self, enabled: bool) -> Self {
        self.network = enabled;
        self
    }
}

impl Automation {
    /// Direct engine access for operations that dispatch document actions (save and print).
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }
}

impl Automation {
    fn template_export(&self, a: &Args) -> Result<Value> {
        let doc = self.doc(a)?;
        let info = pdfcraft_engine::templates::TemplateInfo {
            title: a.opt_str("title")?.unwrap_or(doc.name.trim_end_matches(".pdf")).into(),
            description: a.opt_str("description")?.unwrap_or("").into(),
            category: a.opt_str("category")?.unwrap_or("Custom").into(),
        };
        let target = self.resolve(a.str("path")?, true)?;
        if self.session.docs().iter().filter_map(|d| d.path.as_deref()).any(|p| Path::new(p) == target) {
            return Err(failed("Choose a different path for the template; the original document is open"));
        }
        let bytes = self.session.export_template(doc.id, &info).map_err(failed)?;
        let path = self.write_output(a.str("path")?, &bytes)?;
        Ok(json!({"path":path.to_string_lossy(),"format":"pdfcraft-template","version":1}))
    }
}

impl Automation {
    /// The permission bits the restriction arguments mean (a stand-in permissions password
    /// makes `Protection` compute them; none given = everything).
    fn permission_bits_of(a: &Args) -> Result<i32> {
        use pdfcraft_engine::{Changes, Printing, Protection};
        if ["printing", "changes", "copy", "accessibility"].iter().all(|k| a.get(k).is_none()) {
            return Ok(-1);
        }
        let d = Protection::default();
        let p = Protection {
            open_password: None,
            permissions_password: Some("-".into()),
            printing: match a.opt_str("printing")? {
                None => d.printing,
                Some("none") => Printing::None,
                Some("low") => Printing::Low,
                Some("high") => Printing::High,
                Some(o) => return Err(ToolError::InvalidArgs(format!("unknown printing {o:?}"))),
            },
            changes: match a.opt_str("changes")? {
                None => d.changes,
                Some("none") => Changes::None,
                Some("pages") => Changes::Pages,
                Some("fill-sign") => Changes::FillSign,
                Some("comment-fill-sign") => Changes::CommentFillSign,
                Some("any-except-extract") => Changes::AnyExceptExtract,
                Some(o) => return Err(ToolError::InvalidArgs(format!("unknown changes {o:?}"))),
            },
            copy: a.opt_bool("copy")?.unwrap_or(d.copy),
            accessibility: a.opt_bool("accessibility")?.unwrap_or(d.accessibility),
            algorithm: d.algorithm,
            encrypt_metadata: d.encrypt_metadata,
        };
        Ok(p.permission_bits())
    }
}

impl Automation {
    fn doc_protect_certificates(&mut self, a: &Args) -> Result<Value> {
        use pdfcraft_engine::{Algorithm, CertificateProtection, CertificateRecipient};
        let list = |key: &str| -> Result<Vec<String>> {
            match a.get(key) {
                None => Ok(Vec::new()),
                Some(Value::Array(v)) => Ok(v.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect()),
                _ => Err(ToolError::InvalidArgs(format!("{key} must be an array of file paths"))),
            }
        };
        let password = a.opt_str("password")?.unwrap_or("");
        let load = |paths: &[String]| -> Result<Vec<pdfcraft_engine::sign::Certificate>> {
            let mut out = Vec::new();
            for p in paths {
                let path = self.resolve(p, false)?;
                let bytes = std::fs::read(&path).map_err(|e| failed(format!("{}: {e}", path.display())))?;
                let is_p12 =
                    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("p12") || e.eq_ignore_ascii_case("pfx"));
                let certs = if is_p12 {
                    let id = pdfcraft_engine::sign::pkcs12::open(&bytes, password).map_err(|e| failed(format!("{}: {e}", path.display())))?;
                    vec![id.certificate]
                } else {
                    pdfcraft_engine::sign::x509::load_certificates(&bytes).map_err(|e| failed(format!("{}: {e}", path.display())))?
                };
                if certs.is_empty() {
                    return Err(ToolError::InvalidArgs(format!("{p} holds no certificate")));
                }
                out.extend(certs);
            }
            Ok(out)
        };
        let certificates = load(&list("certificates")?)?;
        let owners = load(&list("owners")?)?;
        if certificates.is_empty() && owners.is_empty() {
            return Err(ToolError::InvalidArgs("certificates must name at least one file".into()));
        }
        let permissions = Self::permission_bits_of(a)?;
        let algorithm = match a.opt_str("compatibility")? {
            None | Some("aes-256") => Algorithm::Aes256,
            Some("aes-128") => Algorithm::Aes128,
            Some(o) => return Err(ToolError::InvalidArgs(format!("unknown compatibility {o:?}"))),
        };
        let mut recipients: Vec<CertificateRecipient> =
            owners.iter().map(|c| CertificateRecipient { certificate: c.raw.clone(), permissions: -1 }).collect();
        recipients.extend(
            certificates
                .iter()
                .filter(|c| !owners.iter().any(|o| o.raw == c.raw))
                .map(|c| CertificateRecipient { certificate: c.raw.clone(), permissions }),
        );
        let p = CertificateProtection {
            recipients,
            algorithm,
            encrypt_metadata: a.opt_bool("encrypt_metadata")?.unwrap_or(true),
            seed: self.session.new_seed().map_err(failed)?,
        };
        let mut out = self.apply(a, Edit::ProtectWithCertificates(p))?;
        out["security"] = security(self.doc(a)?);
        out["recipients"] = Value::Array(
            owners
                .iter()
                .map(|c| json!({ "name": c.display_name(), "email": c.subject.email(), "owner": true }))
                .chain(
                    certificates
                        .iter()
                        .filter(|c| !owners.iter().any(|o| o.raw == c.raw))
                        .map(|c| json!({ "name": c.display_name(), "email": c.subject.email(), "owner": false })),
                )
                .collect(),
        );
        Ok(out)
    }
}

impl Automation {
    fn identity_add(&mut self, a: &Args) -> Result<Value> {
        if a.opt_bool("clear")?.unwrap_or(false) {
            self.session.clear_identities();
        }
        if let Some(p) = a.opt_str("path")? {
            let path = self.resolve(p, false)?;
            let bytes = std::fs::read(&path).map_err(|e| failed(format!("{}: {e}", path.display())))?;
            let id = pdfcraft_engine::sign::pkcs12::open(&bytes, a.opt_str("password")?.unwrap_or(""))
                .map_err(|e| failed(format!("{}: {e}", path.display())))?;
            self.session.add_identity(id);
        }
        let listed: Vec<Value> = self
            .session
            .identities()
            .iter()
            .map(|i| json!({ "name": i.certificate.display_name(), "email": i.certificate.subject.email() }))
            .collect();
        Ok(json!({ "identities": listed }))
    }
}

mod assistant;

use json::security;
