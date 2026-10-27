//! Document: cross-reference data, lazy object loading, and a copy-on-write overlay of edits.
//!
//! `Document` is cheap to clone (the original bytes, xref index and parse caches are shared), so
//! a clone is a snapshot: undo/redo and background jobs hold clones while the UI edits another.
//! Edits never touch the original bytes; the writer appends them (incremental save) or rewrites
//! the file (full save).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};

use crate::CosError;
use crate::object::{Dict, ObjRef, Object};
use crate::parser::{EndstreamIndex, Lexer, is_whitespace, parse_indirect, parse_indirect_shared};

mod objects;
mod open;
mod reader;
mod state;
mod xref;

#[cfg(test)]
pub(crate) use reader::find;
#[cfg(test)]
use reader::rfind;
pub(crate) use state::ObjectReader;
use state::{ObjStm, Slot};
pub use state::{ReadState, SourceIdentity};
#[cfg(test)]
use state::{SECTION_READS, objstm_member, owned_bytes};
use xref::XrefIndex;
pub use xref::{Revision, XrefEntry};

#[derive(Clone)]
pub struct Document {
    data: Arc<Vec<u8>>,
    /// Where `endstream` occurs in `data`; see [`EndstreamIndex`].
    endstreams: Arc<EndstreamIndex>,
    source_identity: Arc<()>,
    entries: Arc<XrefIndex>,
    trailer: Dict,
    revisions: Arc<Vec<Revision>>,
    /// See [`Self::xref_oddities`].
    xref_oddities: Arc<Vec<(u64, u32)>>,
    /// Where the file's object headers are, as (offset, generation) of the last header of each
    /// number. Filled only when the cross-reference data proved inconsistent; see
    /// `fill_xref_gaps`.
    located: Arc<BTreeMap<u32, (u64, u16)>>,
    repair_log: Arc<Vec<String>>,
    cache: Arc<Mutex<HashMap<u32, Arc<Object>>>>,
    objstms: Arc<Mutex<HashMap<u32, Arc<ObjStm>>>>,
    stream_limit: usize,
    overlay: BTreeMap<u32, Slot>,
    next_num: u32,
    /// Position of the `%PDF-` header (some files have junk before it).
    header_offset: usize,
    version: String,
    /// The authenticated security handler of an encrypted document.
    security: Option<Arc<pdfcraft_crypt::SecurityHandler>>,
    /// Object number of the `/Encrypt` dictionary (never encrypted itself).
    encrypt_num: Option<u32>,
    /// Encryption was added, changed or removed since opening: only a full save can apply it.
    encryption_changed: bool,
    /// Set by edits that must not leave earlier revisions in the file (redaction).
    full_save: bool,
    /// The handler and `/Encrypt` object number that saves use, when encryption changed.
    out_security: Option<Arc<pdfcraft_crypt::SecurityHandler>>,
    out_encrypt_num: Option<u32>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document").field("bytes", &self.data.len()).field("xref", &self.entries).field("edits", &self.overlay.len()).finish()
    }
}

#[cfg(test)]
mod tests;

/// How an encrypted document is opened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Credential<'a> {
    /// The standard handler: the user or owner password (`None` tries the empty one).
    Password(Option<&'a str>),
    /// Public-key security: what one of the envelopes held for this person.
    Certificate { seed: [u8; 20], permissions: i32 },
}

/// `params` as the handler that opened the same encryption holds it. A public-key handler
/// keeps the permissions its envelope gave up in place of the file's `/P`, so for public-key
/// security those are not compared; the envelopes and every other entry are.
fn same_encryption(handler: &pdfcraft_crypt::SecurityHandler, mut params: pdfcraft_crypt::EncryptDict) -> pdfcraft_crypt::EncryptDict {
    if params.filter == pdfcraft_crypt::PUBSEC_FILTER {
        params.p = handler.dict().p;
    }
    params
}

impl Document {
    /// Open with a password or, for public-key security (§7.6.5), with the seed and
    /// permissions one of the document's envelopes gave up. A public-key document opened with
    /// a password fails with [`CosError::NeedsCertificate`], which carries the envelopes.
    pub fn open_with_credential(data: Arc<Vec<u8>>, credential: &Credential<'_>) -> Result<Self, CosError> {
        let credentials = match credential {
            Credential::Password(password) => Credentials::Password(*password),
            Credential::Certificate { seed, permissions } => Credentials::Certificate { seed: *seed, permissions: *permissions },
        };
        Self::open_as(data, credentials, crate::object::MAX_DECODED)
    }
}

impl Document {
    /// Protect the document for the holders of certificates (§7.6.5): the envelopes in
    /// `params` were made for them by pdfcraft-sign. Takes effect on the next save, a full
    /// rewrite; this document stays readable (authenticated as owner).
    pub fn set_public_key_encryption(&mut self, params: &pdfcraft_crypt::PublicKeyEncryption) -> Result<(), CosError> {
        let h = pdfcraft_crypt::create_public_key(params).map_err(|e| CosError::Security(e.to_string()))?;
        let d = h.dict();
        let hex = |b: &[u8]| Object::String(crate::PdfString { bytes: b.to_vec(), hex: true });
        let mut cf = Dict::new();
        let mut filter = Dict::new();
        filter.set(b"Type".to_vec(), Object::name("CryptFilter"));
        filter.set(b"CFM".to_vec(), Object::name(if d.v >= 5 { "AESV3" } else { "AESV2" }));
        filter.set(b"AuthEvent".to_vec(), Object::name("DocOpen"));
        filter.set(b"Length".to_vec(), Object::Int(if d.v >= 5 { 32 } else { 16 }));
        filter.set(b"Recipients".to_vec(), Object::Array(d.recipients.iter().map(|r| hex(r)).collect()));
        if !d.encrypt_metadata {
            filter.set(b"EncryptMetadata".to_vec(), Object::Bool(false));
        }
        cf.set(pdfcraft_crypt::PUBSEC_CF.to_vec(), Object::Dict(filter));
        let mut e = Dict::new();
        e.set(b"Filter".to_vec(), Object::Name(d.filter.clone()));
        e.set(b"SubFilter".to_vec(), Object::Name(d.sub_filter.clone()));
        e.set(b"V".to_vec(), Object::Int(d.v));
        e.set(b"Length".to_vec(), Object::Int(d.length_bits));
        e.set(b"CF".to_vec(), Object::Dict(cf));
        e.set(b"StmF".to_vec(), Object::Name(d.stm_f.clone()));
        e.set(b"StrF".to_vec(), Object::Name(d.str_f.clone()));
        if let Some(Object::Ref(old)) = self.trailer.get(b"Encrypt").cloned() {
            self.free(old);
        }
        let r = self.add(e);
        self.trailer.set(b"Encrypt".to_vec(), Object::Ref(r));
        self.out_security = Some(Arc::new(h));
        self.out_encrypt_num = Some(r.num);
        self.encryption_changed = true;
        Ok(())
    }
}

use state::Credentials;
