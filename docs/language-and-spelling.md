# Language and spelling

Personal 0.13.1 adds an Edit menu entry for **Language and spelling** and groups
these settings above interface/appearance controls in Preferences.

The interface picker lists **English (US)** and **English (UK)** together, then
existing languages by native name, including Español and Deutsch. Changes apply
immediately and persist. `en` remains the US preference for compatibility;
`en-US` is accepted as an alias. `en-gb` preserves the regional choice, including
system/browser locale detection and English month/weekday names. British menus
use the corresponding organise/optimise/colour/centre variants; untranslated
wording uses the existing English source.

The **Spelling language** is independent, or follows the interface. US/UK English,
Spanish (Spain/Mexico), German and French are offered, together with additional
installed native dictionaries. An unsupported language is explicitly reported;
PdfCraft never silently substitutes English for a missing dictionary.

## Editing

**Check spelling while typing** marks misspellings in original and added text,
comment/note/reply editors, and eligible form fields. Password and file-selection
fields and fields with the PDF `DoNotSpellCheck` flag are excluded. Right-click an
underlined word to choose a suggestion or add it to the personal dictionary.
Suggestions respect form character limits. Text changes only after an explicit
choice, through the editor's existing commit and Undo path. Spelling marks are
viewer chrome and never enter the saved PDF. The personal dictionary persists
with app preferences (up to 1,000 words, each at most 128 UTF-8 bytes) and can be
cleared in Preferences.

Checks are local, with no service, account, text upload or dictionary download.
Windows and macOS use their installed proofing tools through the MIT/Apache
[Spellkit](https://github.com/rtmongold/spellkit) wrapper. Install the desired
proofing language in the operating system if unavailable, then restart PdfCraft.
Linux/BSD read installed Hunspell `.aff`/`.dic` data using the Rust
[Spellbook](https://github.com/helix-editor/spellbook) parser; no native Hunspell
engine is linked. Install the distribution's language dictionary packages, or
set `PDFCRAFT_DICTIONARIES` to a folder containing pairs such as `en_GB.aff` and
`en_GB.dic`. Dictionaries remain on the user's machine and are not redistributed.
UTF-8 and encodings recognized by encoding_rs are decoded; unreadable/oversized
dictionaries report an error. Dictionary files are limited to 16 MiB each.

Automatic checks accept fields up to 16 KiB and show at most 256 issues. Longer
fields are not automatically checked. Suggestions are requested only for the
word whose context menu is open. This is spelling, without grammar checking or
automatic replacement. The web build keeps interface selection and preferences,
but reports native spelling unavailable. New setting translations are
contributor-original and AI-assisted, not native-reviewed.

## Automation

- `spelling_languages {}` lists installed dictionary tags.
- `text_spellcheck {"text":"I beleeve this.","language":"en-GB","suggestions":true}`
  returns UTF-8 byte ranges, words and optional corrections. Text is required,
  limited to 16 KiB; at most 256 issues are returned, with suggestions for the
  first 32 when requested. It does not open or modify a document.
- `ui.set` / CLI options `spell-check=true|false` and
  `spelling-language=auto|en-US|en-GB|es-ES|es-MX|de-DE|…` control the GUI setting.
  `ui.state` reports the settings and personal-word count, not the words.

This builds on the request in [issue #4262](https://github.com/storytold/pdfcraft/issues/4262).
No dictionary assets, new unsafe code, font files or external proofing service
are added. Native Windows is checked here; Linux/macOS runtime verification
remains separate.
