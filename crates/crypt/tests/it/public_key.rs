//! Lion PDF's public-key (certificate) security, carried as a patch in `vendor/craft-pdf`.

use pdfcraft_crypt::{
    Algorithm, Auth, CryptError, PUBSEC_FILTER, PUBSEC_SUBFILTER, PublicKeyEncryption, SecurityHandler, create_public_key, permission_bytes,
    permissions_from_envelope,
};

#[test]
fn public_key_security_derives_the_file_key_from_seed_and_envelopes() {
    let recipients = vec![vec![0x30, 0x82, 1, 2, 3], vec![0x30, 0x82, 9, 9, 9, 9]];
    let seed = [7u8; 20];
    let params =
        PublicKeyEncryption { algorithm: Algorithm::Aes256, recipients: recipients.clone(), seed, permissions: -3904, encrypt_metadata: true };
    let h = create_public_key(&params).unwrap();
    assert_eq!(h.dict().filter, PUBSEC_FILTER);
    assert_eq!(h.dict().sub_filter, PUBSEC_SUBFILTER);
    assert_eq!((h.dict().v, h.dict().length_bits, h.file_key().len()), (5, 256, 32));
    let secret = h.encrypt_string(12, 0, b"for your eyes");
    // Read back from the dictionary a file would carry, with the seed an envelope gave up.
    let reader = SecurityHandler::open_public_key(h.dict().clone(), &seed, -3904).unwrap();
    assert_eq!(reader.file_key(), h.file_key());
    assert_eq!(reader.decrypt_string(12, 0, &secret), b"for your eyes");
    assert!(!reader.permissions().print() && matches!(reader.auth(), Auth::User));
    assert!(matches!(SecurityHandler::open_public_key(h.dict().clone(), &[1; 20], -1).unwrap().auth(), Auth::Owner));
    assert_ne!(SecurityHandler::open_public_key(h.dict().clone(), &[1; 20], -1).unwrap().file_key(), h.file_key(), "another seed, another key");
    // AES-128 keys are 16 bytes of SHA-1; leaving metadata in clear changes the key.
    let aes128 = create_public_key(&PublicKeyEncryption { algorithm: Algorithm::Aes128, encrypt_metadata: false, ..params.clone() }).unwrap();
    assert_eq!((aes128.dict().v, aes128.file_key().len()), (4, 16));
    assert_ne!(aes128.file_key(), create_public_key(&PublicKeyEncryption { algorithm: Algorithm::Aes128, ..params.clone() }).unwrap().file_key());
    assert!(create_public_key(&PublicKeyEncryption { recipients: vec![], ..params.clone() }).is_err());
    assert!(create_public_key(&PublicKeyEncryption { algorithm: Algorithm::Rc4_128, ..params.clone() }).is_err());
    let (s2, p2) = permissions_from_envelope(&[seed.as_slice(), &permission_bytes(-3904)].concat()).unwrap();
    assert_eq!((s2, p2), (seed, -3904));
    assert_eq!(permissions_from_envelope(&[0; 23]), None, "a short envelope content holds no permissions");
    // The standard opener says what this file needs.
    assert!(matches!(SecurityHandler::open(h.dict().clone(), b"", None), Err(CryptError::NeedsCertificate)));
}

/// Public-key permissions (ISO 32000-2 Table 24) read like the standard handler's revision 3 and
/// later: bit 11 alone grants assembly, and high-quality printing needs bits 3 and 12.
#[test]
fn public_key_permissions_read_as_revision_3_and_later() {
    let params = PublicKeyEncryption {
        algorithm: Algorithm::Aes128,
        recipients: vec![vec![0x30, 0x80]],
        seed: [3; 20],
        permissions: -1,
        encrypt_metadata: true,
    };
    let h = create_public_key(&params).unwrap();
    // Print (bit 3) and assemble (bit 11), but neither modify (bit 4) nor high-quality print (bit 12).
    let p = !0xFFF | 0xC0 | (1 << 2) | (1 << 10);
    let reader = SecurityHandler::open_public_key(h.dict().clone(), &[3; 20], p).unwrap();
    let perms = reader.permissions();
    assert!(perms.print() && perms.assemble(), "{perms:?}");
    assert!(!perms.modify() && !perms.print_high_quality(), "{perms:?}");
}
