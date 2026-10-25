//! Offline proofing. Dictionaries belong to the OS/user and are never downloaded implicitly.

use std::ops::Range;

pub const MAX_TEXT_BYTES: usize = 16_384;
pub const LANGUAGES: [(&str, &str); 6] = [
    ("en-US", "English (US)"),
    ("en-GB", "English (UK)"),
    ("es-ES", "Español (España)"),
    ("es-MX", "Español (México)"),
    ("de-DE", "Deutsch"),
    ("fr-FR", "Français"),
];

/// Normalize a BCP 47 dictionary tag; filesystem separators and oversized inputs are refused.
pub fn locale(tag: &str) -> Result<String, String> {
    if tag.len() > 40 || tag.is_empty() || !tag.bytes().all(|b| b.is_ascii_alphabetic() || b.is_ascii_digit() || b == b'-' || b == b'_') {
        return Err("Use a language tag such as en-US, en-GB, es-ES or de-DE".into());
    }
    let parts: Vec<_> = tag.split(['-', '_']).collect();
    if parts.iter().any(|p| p.is_empty()) || !parts.first().is_some_and(|p| (2..=3).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphabetic()))
    {
        return Err("Invalid spelling language tag".into());
    }
    Ok(parts.iter().enumerate().map(|(i, p)| if i == 0 { p.to_ascii_lowercase() } else { p.to_ascii_uppercase() }).collect::<Vec<_>>().join("-"))
}

pub fn for_interface(tag: &str) -> String {
    match tag {
        "en" | "en-us" => "en-US".into(),
        "en-gb" => "en-GB".into(),
        "es" => "es-ES".into(),
        "de" => "de-DE".into(),
        "fr" => "fr-FR".into(),
        other => locale(other).unwrap_or_else(|_| "en-US".into()),
    }
}

/// A checker stays on the thread that created it (native APIs have thread affinity).
pub struct Checker {
    #[cfg(any(windows, target_os = "macos"))]
    native: spellkit::Checker,
    #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
    dictionary: spellbook::Dictionary,
}

impl Checker {
    pub fn new(tag: &str) -> Result<Self, String> {
        let tag = locale(tag)?;
        #[cfg(any(windows, target_os = "macos"))]
        return crate::guard(|| spellkit::Checker::with_locale(&tag))
            .map_err(|e| format!("Spelling service failed: {e}"))?
            .map(|native| Self { native })
            .map_err(|e| e.to_string());
        #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
        return load_dictionary(&tag).map(|dictionary| Self { dictionary });
        #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_arch = "wasm32")))))]
        {
            let _ = tag;
            Err("Spelling is unavailable on this platform".into())
        }
    }

    pub fn available_languages() -> Vec<String> {
        #[cfg(any(windows, target_os = "macos"))]
        return crate::guard(spellkit::Checker::available_locales)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|l| locale(&l).ok())
            .take(100)
            .collect();
        #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
        return LANGUAGES.iter().filter(|(tag, _)| dictionary_paths(tag).is_some()).map(|(tag, _)| (*tag).into()).collect();
        #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_arch = "wasm32")))))]
        Vec::new()
    }

    /// UTF-8 byte ranges; the caller must explicitly split longer documents into snippets.
    pub fn check(&self, text: &str) -> Result<Vec<Range<usize>>, String> {
        if text.len() > MAX_TEXT_BYTES || text.contains('\0') {
            return Err("Check at most 16 KiB of text at once, without NUL characters".into());
        }
        #[cfg(any(windows, target_os = "macos"))]
        return crate::guard(|| self.native.check(text).take(256).map(|e| e.range()).collect::<Vec<_>>())
            .map(|ranges| ranges.into_iter().filter(|r| r.start < r.end && text.get(r.clone()).is_some()).collect());
        #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
        return Ok(unicode_segmentation::UnicodeSegmentation::unicode_word_indices(text)
            .filter(|(_, w)| w.chars().all(|c| c.is_alphabetic() || c == '\'' || c == '’'))
            .filter(|(_, w)| !self.dictionary.check(w))
            .take(256)
            .map(|(at, w)| at..at + w.len())
            .collect());
        #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_arch = "wasm32")))))]
        Err("Spelling is unavailable on this platform".into())
    }

    pub fn suggest(&self, word: &str) -> Result<Vec<String>, String> {
        if word.len() > 128 || word.contains('\0') {
            return Err("Spelling suggestions accept one word of at most 128 bytes".into());
        }
        #[cfg(any(windows, target_os = "macos"))]
        return crate::guard(|| self.native.suggest(word));
        #[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
        {
            let mut out = Vec::new();
            self.dictionary.suggest(word, &mut out);
            out.truncate(10);
            Ok(out)
        }
        #[cfg(not(any(windows, target_os = "macos", all(unix, not(target_arch = "wasm32")))))]
        Err("Spelling is unavailable on this platform".into())
    }
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
fn dictionary_paths(tag: &str) -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let name = tag.replace('-', "_");
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("PDFCRAFT_DICTIONARIES") {
        dirs.push(std::path::PathBuf::from(dir));
    }
    dirs.extend(["/usr/share/hunspell", "/usr/share/myspell", "/usr/share/myspell/dicts", "/usr/local/share/hunspell"].map(std::path::PathBuf::from));
    dirs.into_iter().find_map(|dir| {
        let aff = dir.join(format!("{name}.aff"));
        let dic = dir.join(format!("{name}.dic"));
        (aff.is_file() && dic.is_file()).then_some((aff, dic))
    })
}

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
fn load_dictionary(tag: &str) -> Result<spellbook::Dictionary, String> {
    use std::io::Read;
    let (aff, dic) =
        dictionary_paths(tag).ok_or_else(|| format!("No {tag} dictionary installed. Install the matching Hunspell language package."))?;
    let read = |path: &std::path::Path| -> Result<Vec<u8>, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path).map_err(|e| e.to_string())?.take(16_777_217).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > 16_777_216 {
            return Err("Spelling dictionary exceeds 16 MiB".into());
        }
        Ok(bytes)
    };
    decode_dictionary(read(&aff)?, read(&dic)?)
}

#[cfg(any(test, all(unix, not(target_os = "macos"), not(target_arch = "wasm32"))))]
fn decode_dictionary(aff: Vec<u8>, dic: Vec<u8>) -> Result<spellbook::Dictionary, String> {
    let encoding = String::from_utf8_lossy(&aff)
        .lines()
        .find_map(|l| l.strip_prefix("SET ").map(str::trim).map(str::to_owned))
        .unwrap_or_else(|| "UTF-8".into());
    let encoding = encoding_rs::Encoding::for_label(encoding.as_bytes()).ok_or("Unsupported dictionary encoding")?;
    let decode = |bytes: Vec<u8>| -> Result<String, String> {
        let (s, _, errors) = encoding.decode(&bytes);
        if errors {
            return Err("Spelling dictionary contains invalid encoded text".into());
        }
        Ok(s.into_owned())
    };
    let aff = decode(aff)?.lines().map(|l| if l.starts_with("SET ") { "SET UTF-8" } else { l }).collect::<Vec<_>>().join("\n");
    let dic = decode(dic)?;
    spellbook::Dictionary::new(&aff, &dic).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dictionary_tags_keep_regions_and_refuse_paths() {
        assert_eq!(locale("EN_gb").unwrap(), "en-GB");
        for tag in ["", "../en-US", "en--US", "en/US", "a", "en\0US"] {
            assert!(locale(tag).is_err(), "{tag}");
        }
        assert_eq!(for_interface("en-gb"), "en-GB");
        assert_eq!(for_interface("es"), "es-ES");
    }

    #[test]
    fn linux_dictionary_reader_preserves_accents_and_decodes_legacy_encodings() {
        let dict = decode_dictionary(b"SET ISO8859-1\n".to_vec(), b"3\namigo\nHaus\nM\xfcller\n".to_vec()).unwrap();
        assert!(dict.check("amigo"));
        assert!(dict.check("Haus"));
        assert!(dict.check("Müller"));
        assert!(!dict.check("amiggo"));
        let mut suggestions = Vec::new();
        dict.suggest("amiggo", &mut suggestions);
        assert!(suggestions.iter().any(|s| s == "amigo"));
        assert!(decode_dictionary(b"SET UnknownEncoding\n".to_vec(), b"1\namigo\n".to_vec()).is_err());
    }
    #[cfg(windows)]
    #[test]
    fn native_spelling_checks_unicode_ranges_and_suggests_without_changing_text() {
        let checker = Checker::new("en-US").unwrap();
        let text = "café 😀 beleeve";
        let ranges = checker.check(text).unwrap();
        assert!(ranges.iter().any(|r| text.get(r.clone()) == Some("beleeve")), "{ranges:?}");
        assert!(!checker.suggest("beleeve").unwrap().is_empty());
        assert!(checker.check(&"a".repeat(MAX_TEXT_BYTES + 1)).is_err());
        assert!(Checker::new("zz-ZZ").is_err());
    }
}
