# Local text recognition

The personal build adapts the Latin recognition and Unicode search-layer work from [community PR #2520](https://github.com/storytold/pdfcraft/pull/2520). Recognition stays on the computer; no document or image is sent to a service.

Settings → Scan & OCR shares language, resolution and existing-text choices with Recognize Text. They persist across launches. Automatic scan recognition still asks whether to process one page or all pages and uses these choices. With the Latin model installed, a new profile starts with **Latin (accented)**; English remains available with the original recognizer. Existing API callers still default to English.

The named French, German, Spanish, Portuguese, Italian, Dutch, Danish, Norwegian, Swedish, Icelandic, Finnish, Czech and Polish choices share PP-OCRv5's Latin model. They prefer a language's letters when recognition scores are close. This does not guarantee every accent or every word. The accented model returns line-sized segments, so selection geometry is approximate within a line. OCR leaves the scan image unchanged and adds invisible searchable text. Unicode text outside WinAnsi uses generated empty glyph programs and ToUnicode maps, without bundling a display font.

Models and dictionaries are pinned by SHA-256 in `ATTRIBUTION.toml`, fetched with `cargo xtask models`, and included with their licences in release packages. The ONNX conversion comes from RapidAI's RapidOCR v3.9.2 publication; PaddleOCR supplies the dictionary and Apache-2.0 licence. Chinese recognition is not included in this build. Missing language models produce an installation error rather than silently switching languages.

Headless clients can inspect `ocr_status.languages[].available`, then pass `language` to `ocr_recognize` or `ocr_recognize_files`. GUI automation selects the OCR Settings pane with `preferences-category=ocr`.
