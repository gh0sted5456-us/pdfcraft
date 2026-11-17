//! Lion PDF's recognition engine, in the [`Recognizer`] frame: English through ocrs, accented
//! Latin through a PP-OCRv5 Latin model, and Japanese, Chinese and the Nordic languages through
//! the optional multilingual PaddleOCR models, with page orientation from PP-LCNet
//! ([`LionOcr::orientation`]) and deskew and paper-edge crop beside it ([`crate::scan`],
//! [`crate::crop`]). The models are those named in ATTRIBUTION.toml (`cargo xtask models`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rten_imageproc::BoundingRect;

use crate::{ALPHABET, DETECTION_MODEL, Line, OcrError, OcrImage, RECOGNITION_MODEL, RecognizeOptions, Recognizer, Word, lines, multilingual, ppocr};

/// [`Recognizer::id`] of this engine, and the [`Word::source`] of what it reads.
pub const ID: &str = "pp-ocr";

pub const LATIN_MODEL: &str = "latin-recognition.onnx";
pub const LATIN_DICTIONARY: &str = "latin-keys.txt";

/// The PP-OCR languages: (code, model file, dictionary file, output classes = CTC blank +
/// one per dictionary line + the appended space). The class count is pinned per model, so a
/// wrong or truncated dictionary fails at load instead of decoding garbage.
const PP_OCR: &[(&str, &str, &str, usize)] = &[
    ("la", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("ca", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("nb", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("fr", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("de", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("es", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("pt", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("it", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("nl", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("da", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("no", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("sv", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("is", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("fi", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("cs", LATIN_MODEL, LATIN_DICTIONARY, 504),
    ("pl", LATIN_MODEL, LATIN_DICTIONARY, 504),
];
pub const MULTILINGUAL_MODEL: &str = "multilingual-recognition.onnx";
pub const ORIENTATION_MODEL: &str = "document-orientation.onnx";
pub const DICTIONARY: &str = "multilingual-dictionary.txt";

/// The languages the models read (ISO 639-1). `la` uses the Latin dictionary; a listed letter can still be misread.
/// The named languages use that same model and, when two letters score close together, prefer
/// the ones that language writes (`å` for Danish, `à` for French) without forbidding the others.
pub const LION_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("la", "Latin (accented)"),
    ("fr", "Français"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("pt", "Português"),
    ("it", "Italiano"),
    ("nl", "Nederlands"),
    ("da", "Dansk"),
    ("no", "Norsk"),
    ("sv", "Svenska"),
    ("is", "Íslenska"),
    ("fi", "Suomi"),
    ("cs", "Čeština"),
    ("pl", "Polski"),
    ("ja", "日本語"),
    ("ca", "Català"),
    ("nb", "Norsk bokmål"),
    ("zh-Hant", "繁體中文"),
    ("zh-Hans", "简体中文"),
];

/// Letters a document language prefers when the recogniser is unsure. Empty means no preference.
pub fn preferred_letters(language: &str) -> &'static str {
    match language {
        "fr" => "àâæçéèêëîïôœùûüÿÀÂÆÇÉÈÊËÎÏÔŒÙÛÜŸ",
        "de" => "äöüßÄÖÜ",
        "es" => "áéíñóúü¿¡ÁÉÍÑÓÚÜ",
        "pt" => "áàâãçéêíóôõúÁÀÂÃÇÉÊÍÓÔÕÚ",
        "it" => "àèéìíîòóùÀÈÉÌÍÎÒÓÙ",
        "nl" => "áéëïóöüÁÉËÏÓÖÜ",
        "da" | "no" | "nb" => "åæøÅÆØ",
        "sv" | "fi" => "åäöÅÄÖ",
        "is" => "áðéíóúýþæöÁÐÉÍÓÚÝÞÆÖ",
        "cs" => "áčďéěíňóřšťúůýžÁČĎÉĚÍŇÓŘŠŤÚŮÝŽ",
        "pl" => "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ",
        _ => "",
    }
}

pub fn language_for_locale(locale: Option<&str>) -> &'static str {
    let locale = locale.unwrap_or_default().trim().split(['.', '@']).next().unwrap_or_default();
    let mut parts = locale.split(['-', '_']);
    let primary = parts.next().unwrap_or_default();
    if primary.eq_ignore_ascii_case("zh") {
        // Only language/script/region subtags matter, never a later extension or private use.
        let next = parts.next().unwrap_or_default();
        if next.eq_ignore_ascii_case("Hant") {
            return "zh-Hant";
        }
        if next.eq_ignore_ascii_case("Hans") {
            return "zh-Hans";
        }
        return if ["TW", "HK", "MO"].iter().any(|region| next.eq_ignore_ascii_case(region)) { "zh-Hant" } else { "zh-Hans" };
    }
    // Older platforms use the generic Norwegian tag for Bokmål.
    let primary = if primary.eq_ignore_ascii_case("no") { "nb" } else { primary };
    LION_LANGUAGES.iter().find(|(code, _)| code.eq_ignore_ascii_case(primary)).map_or("en", |(code, _)| *code)
}

/// Where a language's detector and recogniser are. The PP-OCR languages also require their
/// character dictionary beside the recognition model.
#[derive(Clone, Debug, PartialEq)]
pub struct LionModels {
    pub detection: PathBuf,
    pub recognition: PathBuf,
    pub language: &'static str,
}

impl LionModels {
    /// The models in `dir`, if every required file is there.
    pub fn in_dir(dir: &Path) -> Option<LionModels> {
        Self::in_dir_for(dir, "en")
    }

    pub fn in_dir_for(dir: &Path, language: &str) -> Option<LionModels> {
        let language = LION_LANGUAGES.iter().find(|l| l.0 == language)?.0;
        let multilingual = [MULTILINGUAL_MODEL, ORIENTATION_MODEL, DICTIONARY].iter().all(|f| dir.join(f).is_file());
        let recognition = if language == "en" {
            RECOGNITION_MODEL
        } else if let Some(&(_, model, dictionary, _)) = PP_OCR.iter().find(|(code, ..)| *code == language)
            && dir.join(model).is_file()
            && dir.join(dictionary).is_file()
        {
            model
        } else if multilingual {
            RECOGNITION_MODEL
        } else {
            return None;
        };
        let detection =
            [DETECTION_MODEL, "text-detection.rten"].iter().map(|f| dir.join(f)).find(|p| p.is_file()).unwrap_or_else(|| dir.join(DETECTION_MODEL));
        let recognition = if recognition == RECOGNITION_MODEL {
            [RECOGNITION_MODEL, "text-recognition.rten"]
                .iter()
                .map(|f| dir.join(f))
                .find(|p| p.is_file())
                .unwrap_or_else(|| dir.join(RECOGNITION_MODEL))
        } else {
            dir.join(recognition)
        };
        let m = LionModels { detection, recognition, language };
        (m.detection.is_file() && m.recognition.is_file()).then_some(m)
    }

    /// Look for the models: `$PDFCRAFT_MODELS`, then where the release packages install them
    /// beside the executable ([`LionModels::dirs_beside_exe`]), then the source tree's `assets/models/`.
    pub fn find() -> Option<LionModels> {
        Self::find_for("en")
    }

    pub fn find_for(language: &str) -> Option<LionModels> {
        Self::search_dirs().iter().find_map(|d| Self::in_dir_for(d, language))
    }

    /// The directories [`LionModels::find`] looks in, in order.
    pub fn search_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(d) = std::env::var_os("PDFCRAFT_MODELS") {
            dirs.push(PathBuf::from(d));
        }
        if let Some(exe) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
            dirs.extend(Self::dirs_beside_exe(&exe));
        }
        dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models"));
        dirs
    }

    /// Where the release packages put the models, relative to the directory holding the
    /// executable (packaging/*):
    /// - `models/` beside it: the Windows MSI and portable zip;
    /// - `../Resources/models`: the macOS app bundle (`Contents/MacOS` → `Contents/Resources`);
    /// - `../share/pdfcraft/models`: the FHS-style trees (`bin/` → `share/`) of the Linux
    ///   deb/rpm/tar.gz/AppImage/Flatpak and the FreeBSD tarball, wherever they are installed.
    pub fn dirs_beside_exe(exe_dir: &Path) -> Vec<PathBuf> {
        vec![exe_dir.join("models"), exe_dir.join("../Resources/models"), exe_dir.join("../share/pdfcraft/models")]
    }
}

/// A loaded recogniser. Loading takes a moment; keep one and reuse it.
pub struct LionOcr {
    engine: ocrs::OcrEngine,
    ppocr: Option<ppocr::PpOcr>,
    /// Letters this document language prefers when two readings are close. Empty for English,
    /// and the unrestricted Latin choice.
    preferred: &'static str,
    multilingual: Option<multilingual::Multilingual>,
    language: &'static str,
}

impl LionOcr {
    pub fn load(models: &LionModels) -> Result<LionOcr, OcrError> {
        let load = |p: &Path| rten::Model::load_file(p).map_err(|e| OcrError::Load(p.display().to_string(), e.to_string()));
        let name = models.recognition.file_name().and_then(|f| f.to_str()).unwrap_or_default();
        let ppocr = PP_OCR
            .iter()
            .find(|(_, model, _, _)| *model == name)
            .map(|(_, _, dictionary, classes)| ppocr::PpOcr::load(&models.recognition, &models.recognition.with_file_name(dictionary), *classes))
            .transpose()?;
        let params = ocrs::OcrEngineParams {
            detection_model: Some(load(&models.detection)?),
            recognition_model: if ppocr.is_none() { Some(load(&models.recognition)?) } else { None },
            alphabet: Some(ALPHABET.into()),
            ..Default::default()
        };
        let engine = ocrs::OcrEngine::new(params).map_err(|e| OcrError::Load("ocr engine".into(), e.to_string()))?;
        let dir = models.recognition.parent().ok_or(OcrError::NoModels)?;
        let multilingual = if [MULTILINGUAL_MODEL, ORIENTATION_MODEL, DICTIONARY].iter().all(|f| dir.join(f).is_file()) {
            Some(multilingual::Multilingual::load(dir)?)
        } else {
            None
        };
        Ok(LionOcr { engine, ppocr, preferred: preferred_letters(models.language), multilingual, language: models.language })
    }

    /// Load the models found by [`Models::find`].
    pub fn find() -> Result<LionOcr, OcrError> {
        Self::find_for("en")
    }

    pub fn find_for(language: &str) -> Result<LionOcr, OcrError> {
        if !LION_LANGUAGES.iter().any(|l| l.0 == language) {
            return Err(OcrError::Language(language.into()));
        }
        Self::load(&LionModels::find_for(language).ok_or(OcrError::NoModels)?)
    }

    /// Recognise the text in an RGBA (or RGB, or grey) image, `width` × `height` pixels.
    pub fn recognize(&self, pixels: &[u8], width: u32, height: u32) -> Result<Vec<Line>, OcrError> {
        let count = (width as usize)
            .checked_mul(height as usize)
            .filter(|n| *n <= 64 * 1024 * 1024)
            .ok_or_else(|| OcrError::Recognize("image exceeds 64 megapixels".into()))?;
        if count == 0 || pixels.is_empty() {
            return Err(OcrError::EmptyImage);
        }
        let channels = pixels.len() / count;
        if !pixels.len().is_multiple_of(count) || !matches!(channels, 1 | 3 | 4) {
            return Err(OcrError::Recognize("image must contain exactly 1, 3 or 4 channels per pixel".into()));
        }
        // ocrs wants 1 or 3 channels.
        let rgb: std::borrow::Cow<[u8]> = if channels == 4 {
            pixels.as_chunks::<4>().0.iter().flat_map(|p| [p[0], p[1], p[2]]).collect::<Vec<u8>>().into()
        } else if channels == 1 && (self.ppocr.is_some() || self.multilingual.is_some()) {
            pixels.iter().flat_map(|p| [*p; 3]).collect::<Vec<_>>().into()
        } else {
            pixels.into()
        };
        if self.uses_multilingual(self.language) {
            let image = image::RgbImage::from_raw(width, height, rgb.into_owned()).ok_or(OcrError::EmptyImage)?;
            return self.recognize_language(&image, self.language);
        }
        let err = |e: &dyn std::fmt::Display| OcrError::Recognize(e.to_string());
        let source = ocrs::ImageSource::from_bytes(&rgb, (width, height)).map_err(|e| err(&e))?;
        let input = self.engine.prepare_input(source).map_err(|e| err(&e))?;
        let found = self.engine.detect_words(&input).map_err(|e| err(&e))?;
        let lines = self.engine.find_text_lines(&input, &found);
        if let Some(ppocr) = &self.ppocr {
            if lines.len() > 4096 {
                return Err(OcrError::Recognize("page has more than 4096 text lines".into()));
            }
            let mut result = Vec::new();
            for line in lines {
                let rect = line
                    .iter()
                    .map(|r| r.bounding_rect())
                    .fold(None, |rect: Option<rten_imageproc::Rect<f32>>, r| Some(rect.map_or(r, |b| b.union(r))));
                let Some(r) = rect else { continue };
                let rect = [r.left(), r.top(), r.right(), r.bottom()];
                let text = ppocr.recognize(&rgb, width, height, rect, self.preferred)?;
                if !text.is_empty() {
                    result.push(Line { words: vec![Word::new(text, rect, ID)] });
                }
            }
            return Ok(result);
        }
        let read = self.engine.recognize_text(&input, &lines).map_err(|e| err(&e))?;
        use ocrs::TextItem;
        Ok(read
            .into_iter()
            .flatten()
            .map(|line| Line {
                words: line
                    .words()
                    .map(|w| {
                        let r = w.bounding_rect();
                        Word::new(w.to_string(), [r.left() as f32, r.top() as f32, r.right() as f32, r.bottom() as f32], ID)
                    })
                    .filter(|w| !w.text.trim().is_empty())
                    .collect(),
            })
            .filter(|l| !l.words.is_empty())
            .collect())
    }

    fn uses_multilingual(&self, language: &str) -> bool {
        // The bundled multilingual model retains Nordic rings where the Latin model misreads them.
        (self.ppocr.is_none() && language != "en") || (self.multilingual.is_some() && matches!(language, "da" | "no" | "nb" | "sv" | "fi"))
    }

    pub fn recognize_language(&self, image: &image::RgbImage, language: &str) -> Result<Vec<Line>, OcrError> {
        let pixels = u64::from(image.width()) * u64::from(image.height());
        if pixels == 0 {
            return Err(OcrError::EmptyImage);
        }
        if pixels > 64 * 1024 * 1024 {
            return Err(OcrError::Recognize("image exceeds 64 megapixels".into()));
        }
        if language == self.language && !self.uses_multilingual(language) {
            return self.recognize(image.as_raw(), image.width(), image.height());
        }
        let multilingual = self.multilingual.as_ref().ok_or(OcrError::NoModels)?;
        if !LION_LANGUAGES.iter().any(|l| l.0 == language) {
            return Err(OcrError::Recognize(format!("unsupported language: {language}")));
        }
        use rten_imageproc::BoundingRect;
        let source = ocrs::ImageSource::from_bytes(image.as_raw(), image.dimensions()).map_err(|e| OcrError::Recognize(e.to_string()))?;
        let input = self.engine.prepare_input(source).map_err(|e| OcrError::Recognize(e.to_string()))?;
        let found = self.engine.detect_words(&input).map_err(|e| OcrError::Recognize(e.to_string()))?;
        let boxes = self.engine.find_text_lines(&input, &found);
        let rects: Vec<[f32; 4]> = boxes
            .into_iter()
            .map(|line| {
                line.iter().map(|r| r.bounding_rect()).fold([f32::INFINITY, f32::INFINITY, 0.0f32, 0.0f32], |a, r| {
                    [a[0].min(r.left()), a[1].min(r.top()), a[2].max(r.right()), a[3].max(r.bottom())]
                })
            })
            .collect();
        if rects.len() > 4096 {
            return Err(OcrError::Recognize("page has more than 4096 text lines".into()));
        }
        // CJK's centred punctuation can split one horizontal line into several detections.
        // Join their geometry before padded crops reach the recognizer, avoiding duplicate ink.
        let rects = if matches!(language, "zh-Hant" | "zh-Hans") { lines::join_fragments(rects) } else { rects };
        let mut out = Vec::new();
        for rect in rects {
            let words = multilingual.read_line(image, rect, language)?;
            if !words.is_empty() {
                out.push(Line { words });
            }
        }
        Ok(out)
    }

    pub fn orientation(&self, image: &image::RgbImage) -> Result<u16, OcrError> {
        self.multilingual.as_ref().map_or(Ok(0), |m| m.orientation(image))
    }
}

/// Lion PDF's engine as a [`Recognizer`]: loads its models for the language asked for, once, and
/// keeps them.
#[derive(Default)]
pub struct LionRecognizer {
    loaded: Mutex<Option<(String, Arc<LionOcr>)>>,
}

impl LionRecognizer {
    pub fn new() -> LionRecognizer {
        LionRecognizer::default()
    }

    /// The engine for `language`, loaded once and reused while the language stays the same.
    pub fn engine(&self, language: &str) -> Result<Arc<LionOcr>, OcrError> {
        let language = if LION_LANGUAGES.iter().any(|l| l.0 == language) { language } else { "en" };
        let mut loaded = self.loaded.lock().map_err(|_| OcrError::Recognize("the recognition engine is unavailable".into()))?;
        if let Some((code, engine)) = loaded.as_ref()
            && code == language
        {
            return Ok(Arc::clone(engine));
        }
        let engine = Arc::new(LionOcr::find_for(language)?);
        *loaded = Some((language.to_string(), Arc::clone(&engine)));
        Ok(engine)
    }
}

impl Recognizer for LionRecognizer {
    fn id(&self) -> &'static str {
        ID
    }

    fn available(&self) -> bool {
        LionModels::find().is_some()
    }

    fn languages(&self) -> Vec<(String, String)> {
        LION_LANGUAGES.iter().filter(|(code, _)| LionModels::find_for(code).is_some()).map(|(c, n)| ((*c).to_string(), (*n).to_string())).collect()
    }

    fn recognize(&self, image: &OcrImage, options: &RecognizeOptions) -> Result<Vec<Line>, OcrError> {
        self.engine(&options.language)?.recognize(image.rgba(), image.width(), image.height())
    }
}
