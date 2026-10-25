# Starter PDFs and reusable templates

> **Last reviewed:** 2026-10-25 · **Last updated:** 2026-10-25 · **Status:** Personal 0.13.1; starter gallery and version-1 template files. Native Linux/macOS dialogs and browser interactions still need runtime checks.

Choose **File > Create a PDF** or press **Ctrl+N** (Cmd+N on macOS). The centered gallery uses most of the window width for a scrollable grid of real PDF previews. Filter by category or search, select a tile, and review its name, page count and physical size below the grid. Folder and download buttons at the top import and export templates. Previews load locally on one background worker, prioritizing the current selection. **Create** opens a new unsaved document in Edit PDF. **New blank PDF** remains available as a direct action.

The 35 built-in starters cover:

| Category | Starters |
|---|---|
| Essentials | Blank page |
| Business | Business card, business letter, invoice, quotation, proposal, project brief, meeting agenda, meeting minutes, business report |
| Writing | Editorial article, booklet, essay, manuscript |
| Marketing | Flyer, event poster, tri-fold brochure, newsletter, product sheet, press release |
| Education | Lesson plan, worksheet, study guide |
| Personal | Resume, cover letter |
| Planning | Weekly, monthly and daily planners; checklist, budget planner, project timeline |
| Forms | Client intake, feedback, order form, attendance sheet |

Designed starters use US Letter or A4. Business cards use finished 3.5 x 2 inch (Letter) or 85 x 55 mm (A4) pages, without bleed or imposition. Blank pages use Settings > Paper and print defaults; Custom page allows exact dimensions in inches, millimetres, centimetres or points. The booklet uses half-sheet pages (Half Letter or A5), in reading order; it does not impose printer spreads. Brochures, calendar planners and timelines use landscape pages. Review fold and print settings for brochure production. Calendar layouts are undated.

Text and artwork remain editable PDF objects. Form starters contain real AcroForm text fields; checklists have interactive checkboxes. Use Select to fill fields and Prepare a Form to change them. Starter wording is English placeholder content. These are fixed page layouts rather than word-processing documents that repaginate themselves.

## Paper defaults and actual-size viewing

**Settings > Paper and print** separates the default for new blank documents from print paper. It offers US Letter/Legal/Tabloid, ISO A0-A6, architectural and ANSI sheets, labels, an envelope, US/European/Japanese business cards, width/height and orientation swapping. Dimensions are kept in PDF points (72 per inch), independent of preview pixels or image DPI. Each side must be finite and between 3 and 14,400 points. Defaults persist; changing them does not resize existing or imported PDFs. Designed templates keep their Letter/A4 layouts.

The on-screen 50 mm ruler calibrates **View > Actual size**, the page-display menu and Ctrl/Cmd+1. Measure the drawn line with a physical ruler and enter its actual length; the factor is 50 divided by that measurement. Repeat after changing monitors, system scaling or interface size. This setting changes zoom only. Print at **Actual size** to preserve design dimensions, with a supported printer sheet and enough printable area; Fit/Shrink can change the scale. Card starters are finished card pages; arrange multiple cards and bleed separately for production.

Headless creation uses `doc_create {"from":"blank","width":252,"height":144}` for a US business card. UI control supports `default-paper` (a displayed preset name), `default-page-width`/`default-page-height` (points), and `actual-size-zoom` (0.25-4). The paper preference category is `paper`.

## Export to other applications

**File > Export to** opens a centered picker for WordCraft/Microsoft Word DOCX, PowerPoint PPTX, RTF, HTML, plain text, images or a reusable PdfCraft template. [WordCraft uses DOCX as its primary document format](https://github.com/storytold/wordcraft#why-wordcraft), so no invented native extension is needed. Existing headless tools (`doc_export_office`, `doc_export_text`, `doc_export_images`) remain the conversion API. The source PDF remains open and unchanged; Office export refuses a destination matching an open document and refuses an incomplete conversion when traversal limits are exceeded. OCR is needed for text from scans. Fonts, vector artwork and complex layouts have conversion limits; exports are not guaranteed to reproduce a PDF exactly. No XLSX or PostScript writer is advertised by this picker.

## Export and reuse

- **Export template...** in the gallery saves the selected starter.
- **File > Save as template** snapshots the current PDF with a name, description and category. It includes current content and form values, and does not mark the source document saved. Choose a destination separate from any open document.
- **Import template...** previews an exported file in the gallery. **Create** makes a working copy.
- Opening or dropping a `.pdfcraft-template` file also creates an unsaved PDF with no adopted source path. Saving the working PDF asks for its own destination.

Existing unsigned page locks survive template export/import. Signed and password-protected documents must be unsigned/unprotected before becoming reusable templates. Exported files are at most 64 MiB.

## Version 1 format

The `.pdfcraft-template` file is an ordinary PDF with this catalog entry:

```text
/PdfCraftTemplate <<
  /Version 1
  /Title (My template)
  /Description (What this document is for)
  /Category (Business)
>>
```

The strings use PDF text-string encoding, so metadata can contain Unicode. Titles are required and limited to 120 characters; descriptions to 1,000; categories to 64. Unsupported versions and incomplete metadata are refused. Import removes the template marker from the working copy, preserving pages, artwork, field definitions and values. Other readers can display the PDF, but their editors may discard the custom metadata.

This is a starter-document format, separate from named page templates and JavaScript page spawning requested in upstream #755.

## Headless tools

`template_list` returns the built-ins. Create one with `doc_create {"from":"starter","template":"article","paper":"a4"}`. Export with `template_export {"doc":1,"path":"my.pdfcraft-template","title":"My article"}`. Load a working copy with `doc_create {"from":"template","path":"my.pdfcraft-template"}` or `doc_open`. Reads and writes honor automation roots. Previews and templates use local rendering; no template service is contacted.

## Revision history

- **2026-10-24:** Initial gallery, 35 contributor-original layouts, editable fields, portable template files, source-preserving import/export and headless tools. Booklet imposition, translations of starter content and native Linux/macOS/browser runtime checks remain separate work.

- **Verification (2026-10-24):** 1,089 UI integration tests passed (4 existing ignored), 267 automation integration tests passed, and 1,022 scoped library tests passed (3 existing ignored). Strict scoped clippy, wasm, assets, layers and parity checks pass. All 35 starters reopen in Letter/A4; independent parsers and Poppler metadata checks cover 70 PDFs. The app rendered all 61 Letter pages for visual inspection, and canonical filled values/appearances and business-card DOCX/PPTX page dimensions were checked separately. No physical printer or actual-monitor measurement was performed.
