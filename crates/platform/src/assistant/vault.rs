//! Chunked OS-store records: Windows credential blobs are small. Publish the manifest last.
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

const SERVICE: &str = "org.pdfcraft.assistant.v1";
const CHUNK: usize = 2048;
const MAX_CHUNKS: usize = 32;

#[derive(Serialize, Deserialize)]
struct Manifest {
    generation: String,
    chunks: usize,
}

trait Backend {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String>;
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String>;
    fn remove(&self, key: &str) -> Result<(), String>;
}
struct Os;
fn entry(key: &str) -> Result<keyring::Entry, String> {
    if !cfg!(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "linux", target_os = "freebsd", target_os = "openbsd")) {
        return Err("This platform has no supported operating system credential store".into());
    }
    keyring::Entry::new(SERVICE, key).map_err(|_| "The operating system credential store is unavailable".into())
}
impl Backend for Os {
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        match entry(key)?.get_secret() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Couldn't read this user's AI credentials from the operating system store".into()),
        }
    }
    fn write(&self, key: &str, bytes: &[u8]) -> Result<(), String> {
        entry(key)?.set_secret(bytes).map_err(|_| "Couldn't save AI credentials in the operating system store; no plaintext fallback was used".into())
    }
    fn remove(&self, key: &str) -> Result<(), String> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Couldn't remove AI credentials from the operating system store".into()),
        }
    }
}
fn manifest(backend: &impl Backend, key: &str) -> Result<Option<Manifest>, String> {
    let Some(bytes) = backend.read(key)? else { return Ok(None) };
    let m: Manifest = serde_json::from_slice(&bytes).map_err(|_| "The AI credential record is damaged")?;
    if m.chunks == 0 || m.chunks > MAX_CHUNKS || uuid::Uuid::parse_str(&m.generation).is_err() {
        return Err("The AI credential record is invalid".into());
    }
    Ok(Some(m))
}
fn chunk_key(key: &str, manifest: &Manifest, n: usize) -> String {
    format!("{key}:{}:{n}", manifest.generation)
}
fn read_from(backend: &impl Backend, key: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let Some(m) = manifest(backend, key)? else { return Ok(None) };
    let mut value = Zeroizing::new(Vec::new());
    for n in 0..m.chunks {
        let bytes = Zeroizing::new(backend.read(&chunk_key(key, &m, n))?.ok_or("The AI credential record is incomplete")?);
        if bytes.len() > CHUNK {
            return Err("An AI credential chunk exceeds its limit".into());
        }
        value.extend_from_slice(&bytes);
    }
    Ok(Some(value))
}
fn write_to(backend: &impl Backend, key: &str, value: &[u8]) -> Result<(), String> {
    if value.is_empty() || value.len() > CHUNK * MAX_CHUNKS {
        return Err("The AI credential record is empty or too large".into());
    }
    let previous = manifest(backend, key)?;
    let m = Manifest { generation: uuid::Uuid::new_v4().to_string(), chunks: value.len().div_ceil(CHUNK) };
    let result = (|| {
        for (n, chunk) in value.chunks(CHUNK).enumerate() {
            backend.write(&chunk_key(key, &m, n), chunk)?;
        }
        backend.write(key, &serde_json::to_vec(&m).map_err(|_| "Couldn't encode AI credential metadata")?)
    })();
    if result.is_err() {
        for n in 0..m.chunks {
            let _ = backend.remove(&chunk_key(key, &m, n));
        }
    } else if let Some(previous) = previous {
        for n in 0..previous.chunks {
            let _ = backend.remove(&chunk_key(key, &previous, n));
        }
    }
    result
}
pub(super) fn read(key: &str) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    read_from(&Os, key)
}
pub(super) fn write(key: &str, value: &[u8]) -> Result<(), String> {
    write_to(&Os, key, value)
}
pub(super) fn remove(key: &str) -> Result<(), String> {
    if let Some(m) = manifest(&Os, key)? {
        for n in 0..m.chunks {
            Os.remove(&chunk_key(key, &m, n))?;
        }
    }
    Os.remove(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
    };
    #[derive(Default)]
    struct Memory {
        values: RefCell<HashMap<String, Vec<u8>>>,
        refuse_manifest: Cell<bool>,
    }
    impl Backend for Memory {
        fn read(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
            Ok(self.values.borrow().get(key).cloned())
        }
        fn write(&self, key: &str, value: &[u8]) -> Result<(), String> {
            assert!(value.len() <= CHUNK);
            if key == "test" && self.refuse_manifest.get() {
                return Err("store unavailable".into());
            }
            self.values.borrow_mut().insert(key.into(), value.to_vec());
            Ok(())
        }
        fn remove(&self, key: &str) -> Result<(), String> {
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }
    #[test]
    fn a_failed_token_rotation_preserves_the_previous_record_without_plaintext_files() {
        let backend = Memory::default();
        let original = vec![7; 9000];
        write_to(&backend, "test", &original).unwrap();
        assert_eq!(read_from(&backend, "test").unwrap().unwrap().as_slice(), original);
        let count = backend.values.borrow().len();
        backend.refuse_manifest.set(true);
        assert!(write_to(&backend, "test", &vec![8; 15000]).is_err());
        assert_eq!(read_from(&backend, "test").unwrap().unwrap().as_slice(), original);
        assert_eq!(backend.values.borrow().len(), count);
        assert!(write_to(&backend, "test", &vec![0; CHUNK * MAX_CHUNKS + 1]).is_err());
    }
}
