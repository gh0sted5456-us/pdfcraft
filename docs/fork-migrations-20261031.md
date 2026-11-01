# Lion PDF: reviewed feature migrations, 2026-10-31

Status: integration candidate. Base fork revision `244a4a31b2a4ff5a1b263ef8afef9827b1cbfaf2`.
The requested installed CJK font-fallback migration is excluded. Existing translations and
optional bundled-font support are retained.

## Sources and adaptations

All source commits are in storytold/pdfcraft and retain the repository's MIT/Apache-2.0
licensing. The table identifies the actual selected work rather than claiming a full upstream
merge. The source Git history remains available for provenance.

| Change | Upstream source | Fork adaptation |
| --- | --- | --- |
| Paragraph and added-text Undo/Redo | #4179, `dcf33ed1`, reviewed follow-ups through `296b9300`; RiceDee | Keep per-document added-text IDs, spellchecking, paragraph overlay, formatting and rejected drafts. Local typing history/caret survives tab switching; menu actions use the same ownership as keys. |
| Blank-line paragraph boundaries | #2529, `e228e0ed`; ihsan mokhlisse | Preserve previous footnote/superscript grouping safeguards; include an automation regression. |
| Markdown export | #5208, `bcaead54`, `20d396d1`; Adrian | Add native Export to/menu/picker command as well as engine/automation output. Preserve Excel, copy permissions and confined writes. |
| Individual reply actions | #4719, `42da7f24`, safety follow-ups `28a31aeb`, `6a178359`; alehel | Keep house Save/Cancel buttons and spelling marks. Share locked-descendant preflight with the fork's bulk deletion; lift annotations only on preview copies. |
| Calculation diagnostics | #4325, `4cfd5927` through `2055f48f`; Grzegorz Martens | Keep NoRotate widget geometry and the existing canvas architecture; focus by field object when malformed fields share names. Missing/built field names are diagnosed and rejected formulas stay editable. |
| Monochrome images | #4384 stack `f175beb2` through `1ad951e2`; Grzegorz Martens; later reviewed images/regressions from integration #5426 (`fa5f9f8c`, FredApps and contributors), plus `af03f084` (Combiler01) | Group 4/Retain, optional downsampling, safe 1-bit packing, Group 3 handling and shared bounded image decoding across Create/Export/Redaction. Unknown palette polarity, local halftones and unsafe resource colour-space conversion retain content. JBIG2 encoding is removed from the reviewed stack. |
| Offline licensing validator | #2212, `74eef8cc`, `19c96011`; JM and review by FredApps/Combiler01 | Keep fork icon/Discord attribution and the approved spellchecker exception. Re-audit NOTICE/icon text hashes and map existing CC0 documentation. No asset or font downloads. |
| Update-result presentation | Equivalent to the result/retry UX of #5527 | Reuse the existing Lion dialog/icon/buttons. Preserve the fork's verified download, staging, installer activation and restart backend; no shared-library repin or provider replacement. |

The monochrome codecs `fax` and `hayro-ccitt` were already present in the locked owned
source inputs. Only dependency edges changed; no locked package version/checksum changed.
Lucide's existing ISC and Feather MIT notices are represented as `ISC AND MIT` in attribution.

## Qualification and limits

Synthetic fixtures only. Builds, Cargo inputs, screenshots, test files and caches are routed
through the project helper to F:. No installed app settings, printer defaults or personal
PDFs are changed by these checks.

- The paragraph Undo and blank-line regressions were observed failing with their old behavior,
  then passing after restoration of the fixes. The initial UI build also exposed the locked
  parent/reply menu case; the reviewed lock preflight addresses it.
- Core COS/Create/Edit/Export/Filters/Forms/Redaction unit suites: 970 passed, one pre-existing
  ignored test. Optimizer: 11 unit and 58 integration tests, including exact render/save/reopen
  comparisons, palette-order/decode tests, local halftones and bounded wrapped-fax decoding.
- Annotation suite: 78 passed, including locked-descendant refusal and preview lifting.
- Scoped all-target strict Clippy passes. Layering and offline licensing validation pass.
- WebAssembly UI/automation compilation passes with 11 existing native-only/dead-code warnings;
  this is compilation coverage, not browser runtime qualification.
- Native UI: 1,315 passed, 10 existing ignored. Automation: 328 passed. Engine: 445 passed,
  two existing ignored. Comment unit tests: 17 passed; calculation UI units: 11 passed; licensing
  units: 11 passed. The licensing check covers all seven existing OCR model/dictionary inputs.
- Desktop and CLI builds pass. A confined CLI script created a synthetic PDF, exported Markdown
  and rendered a page. The real portable desktop accepted the export command; its native
  inspection contains Markdown and its screenshot at 1000x780 shows a centered, legible dialog.
  Synthetic previews were visually inspected. The rebuilt optimizer was also inspected and captured
  at 1100x900; its monochrome controls fit and remain legible. The preview exited after the control
  channel quit request (the final acknowledgement reported a connection reset during teardown).

Markdown reconstructs headings, paragraph emphasis and detected tables; it omits images,
list/link reconstruction and text colour. Monochrome downsampling is lossy when enabled;
conservative guards may retain resolution and cost file size. Eight-bit inputs can also follow
the separate colour/grayscale settings, including their lossy compression. Existing JBIG2 reading stays.
The licensing validator checks source documentation/metadata, not completed installer
compliance or legal sufficiency. Native Linux/macOS runtime, live GitHub update installation,
physical CAC and independent JBIG2 encoder qualification are not claimed.

No CJK fallback, JBIG2 encoder, selected-line paragraph alignment or unrelated UI framework
migration is included. Whole upstream integration remains deliberately selective.

Revision history: 2026-10-31 — source selection, fork adaptations and local qualification.
FreeBSD qualification exposed a codec-specific assertion in the monochrome retention test.
The later upstream fix `af03f084` accepts either Flate or Group 4 while checking decoded samples;
the fork also checks unchanged dimensions and non-increasing compressed size. Runtime selection
remains unchanged. The full platform jobs are rerun for this follow-up.

Linux CI ran 5,885 tests and exposed a missing accessible name on the monochrome compression
selector. Its selector and resolution controls now reference their visible labels, matching
the colour/grayscale rows, and wrap at narrow widths. A focused light/dark regression checks
the resolved accessible name and confirms that monochrome changes preserve colour settings;
it failed on the unnamed selector before the fix. Accessibility checks remain enabled.
