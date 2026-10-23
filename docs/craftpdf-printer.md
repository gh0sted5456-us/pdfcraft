# CraftPDF Printer (planned; excluded from 0.13.1)

The personal 0.13.1 build does not install or offer a CraftPDF printer. The user requested
a separate conversion engine instead of a renamed Microsoft Print to PDF queue.

The requirement is to retain text as text, with Unicode mappings and accurate word positions,
so printed PDFs support selection, searching and highlighting without OCR. Vector artwork
should remain vector artwork. Unsupported input must return a clear error instead of silently
rasterizing a page. Image-only source material cannot provide original text without OCR.

Windows printer integration and the PDF conversion engine are separate pieces. The engine
should accept a vector/text print intermediate, preserve embedded font data and glyph placement,
and create the PDF using PdfCraft's own writer. Printer registration must not change the
system default, overwrite an unrelated queue, or start an agent/network service automatically.

Local QA on 2026-10-23 confirmed that a synthetic Word document printed through Microsoft's
existing driver retained words that PdfCraft could find, highlight, save and reopen without OCR.
That only tested the old prototype; it is not evidence for the planned independent engine.
