# Personal fork: unfinished-tool integration

Status: 2026-10-27. Feature proposals are reviewed against the personal fork before enabling their controls. A public proposal or a removed milestone badge does not establish a working tool.

| Tool | Upstream work | Integration status |
| --- | --- | --- |
| Excel export | [#2389](https://github.com/storytold/pdfcraft/pull/2389) | Integrated in the Lion PDF candidate; native XLSX output, range/export chooser, independent-reader tests. |
| Printer marks | [#1918](https://github.com/storytold/pdfcraft/pull/1918) | Integrated in the candidate; selected pages, removable marks, physical page-unit sizes and resource-preservation tests. |
| Page Properties | [#4516](https://github.com/storytold/pdfcraft/pull/4516) | Tab-order controls integrated in the candidate; page actions remain separate unfinished work. |
| Fix hairlines | [#1906](https://github.com/storytold/pdfcraft/pull/1906) | Integrated in a separate candidate; selected-resource copies, atomic refusal and save/Undo tests. See the parity entry for scope limits. |
| Convert colours | [#1998](https://github.com/storytold/pdfcraft/pull/1998) | Located; unsupported colour spaces and incomplete content must be refused rather than approximated silently before integration. |
| Reading order / tags | [#2775](https://github.com/storytold/pdfcraft/pull/2775) | Located; structural edits require review for cyclic trees, atomicity, permissions and the current UI layout. |
| PS/EPS export | [#1785](https://github.com/storytold/pdfcraft/pull/1785), [#1994](https://github.com/storytold/pdfcraft/pull/1994) | Located; the second proposal extends fonts. Independent PostScript rendering and export-path safeguards are still required. |
| PDF/X conversion | [#3028](https://github.com/storytold/pdfcraft/pull/3028) | Located; conversion, profiles and independent conformance qualification remain open. |
| PDF/UA conversion | [#3026](https://github.com/storytold/pdfcraft/pull/3026) | This is validator work, not a complete accessible-PDF converter. It cannot enable Save as PDF/UA by itself. |
| Geospatial tools | [#467](https://github.com/storytold/pdfcraft/pull/467) | Located; broad proposal also includes 3D and new dependencies, requiring separate review. |
| Paragraph spacing | [#1487](https://github.com/storytold/pdfcraft/pull/1487) | Located; proposal also changes older pointer/OCR handlers. A focused spacing port must retain current glyph/OCR safeguards and qualify soft wraps, repeated spacing changes and preview/save agreement. |
| Page transitions | #2083 backend already integrated | Creation controls and full presentation playback remain incomplete. |
| Make accessible wizard | No complete proposal established | Keep unavailable until an end-to-end workflow is qualified. |
| Archive/PDF-A action preset | No complete proposal established | A generic action preset is not proof of PDF/A conversion. |
| Search index manager | No complete proposal established | Discarding an existing index does not provide an index manager. |
| Page templates | No complete proposal established | Distinct from the working New PDF starter-template gallery. |
| Bullets, numbering, superscript/subscript | No complete editing proposal established | Text-extraction fixes alone do not implement these editing controls. |

Technical installer names, settings storage, update repository and PdfCraft Printer identities remain compatible with previous personal builds. Visible application branding is Lion PDF; upstream attribution remains PdfCraft.

Revision history: 2026-10-27 — initial inventory and qualification status. The machine-readable feature scope and test evidence remain in `parity/acrobat-features.toml`.
