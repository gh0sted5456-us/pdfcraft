//! Prove live access to the exact signer's private key without changing or re-signing a PDF.
use crate::{
    Certificate, DigitalId, SignError,
    keys::{DigestAlg, Scheme, StoreKey},
};

/// A fresh, domain-separated challenge bound to the caller's operation/document.
/// A cached certificate, a matching name, or a different key is never enough.
pub fn prove(id: &DigitalId, expected: &Certificate, binding: &[u8]) -> Result<(), SignError> {
    if id.certificate.raw != expected.raw || id.key.public_key().spki() != expected.public_key.spki() {
        return Err(SignError::Crypto("The digital ID does not match this signature's certificate".into()));
    }
    let scheme = match expected.public_key.store_signing_key() {
        Some(StoreKey::Rsa) => Scheme::RsaPkcs1,
        Some(StoreKey::Ecdsa(_)) => Scheme::Ecdsa,
        None => return Err(SignError::Unsupported("This certificate's key cannot prove ownership".into())),
    };
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce).map_err(|e| SignError::Crypto(format!("Couldn't create an ownership challenge: {e}")))?;
    let alg = DigestAlg::Sha256;
    let mut challenge = b"PdfCraft signature removal key proof v1\0".to_vec();
    challenge.extend_from_slice(&nonce);
    challenge.extend_from_slice(&alg.digest(&[binding]));
    let value = id.key.sign(alg, &challenge)?;
    if !expected.public_key.verify(scheme, alg, &alg.digest(&[&challenge]), &value)? {
        return Err(SignError::Crypto("The private key did not verify against this signature's certificate".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{ExternalKey, PrivateKey};
    use std::sync::{Arc, Mutex};
    struct Card {
        key: PrivateKey,
        available: bool,
        replay: Mutex<Option<Vec<u8>>>,
    }
    impl ExternalKey for Card {
        fn sign(&self, alg: DigestAlg, msg: &[u8]) -> Result<Vec<u8>, SignError> {
            if !self.available {
                return Err(SignError::Crypto("card removed".into()));
            }
            let mut previous = self.replay.lock().unwrap();
            if let Some(value) = previous.as_ref() {
                return Ok(value.clone());
            }
            let value = self.key.sign(alg, msg)?;
            *previous = Some(value.clone());
            Ok(value)
        }
    }
    fn id() -> DigitalId {
        crate::pkcs12::open(include_bytes!("../tests/data/ec-p256.p12"), "test").unwrap()
    }
    #[test]
    fn ownership_needs_the_exact_certificate_and_a_fresh_live_private_key_operation() {
        let id = id();
        let certificate = id.certificate.clone();
        assert!(prove(&id, &certificate, b"document/field").is_ok());
        let other = crate::pkcs12::open(include_bytes!("../tests/data/rsa-aes.p12"), "test").unwrap();
        assert!(prove(&other, &certificate, b"document/field").is_err());
        let id = DigitalId {
            key: PrivateKey::external(certificate.public_key.clone(), Arc::new(Card { key: id.key, available: true, replay: Mutex::new(None) })),
            certificate: certificate.clone(),
            chain: Vec::new(),
            friendly_name: None,
        };
        assert!(prove(&id, &certificate, b"document/field").is_ok());
        assert!(prove(&id, &certificate, b"document/field").is_err(), "a replayed signature cannot prove current access");
        let original = self::id();
        let absent = DigitalId {
            key: PrivateKey::external(
                certificate.public_key.clone(),
                Arc::new(Card { key: original.key, available: false, replay: Mutex::new(None) }),
            ),
            certificate: certificate.clone(),
            chain: Vec::new(),
            friendly_name: None,
        };
        assert!(prove(&absent, &certificate, b"document/field").is_err(), "cached certificate alone is insufficient");
    }
}
