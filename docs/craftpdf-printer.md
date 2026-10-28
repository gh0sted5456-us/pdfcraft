# PdfCraft Printer

> **Last updated:** 2026-10-28 · Lion PDF 1.0.2 Windows registration, retained payload and update/restart qualification. Native Linux printing and broader application interoperability need their own runtime qualification.

The personal Windows installer includes PdfCraft Printer, a Windows Print Support App virtual printer for Windows 11 24H2 or newer. Its bounded XPS/OXPS converter uses PdfCraft's PDF writer and preserves supported Unicode text and vector content without a raster fallback. Unsupported jobs fail explicitly. Image-only source material still needs OCR for text selection.

The desktop MSI owns `Printer/PdfCraft.Printer.msix`, `Printer/pdfcraft_printer.dll` and `Printer/Printer.pdc` under the PdfCraft installation directory. These files remain installed for repair and updates. Windows manages the registered runtime copy in its package store.

The bootstrapper and restarted application run with normal user privileges. Only printer package registration requests administrator approval through Windows. This personal unsigned MSIX requires that approval; setup does not change certificate stores or Developer Mode. An already healthy matching printer version stays registered, and setup preserves the user's default printer.

Preferences → Updates downloads and verifies the combined installer from this fork's GitHub releases. Open PDFs can remain open while downloading. Save/Discard/Cancel resolves edits before the staged installer acknowledges readiness and waits for the app to exit. After installation, the app restarts with saved PDF paths/pages restored. Other app windows and minimized document drafts must close first. Portable builds use the portable ZIP.

Qualification for 1.0.2 includes synthetic Unicode conversion tests, real package registration, a ready printer queue, installed payload checks, and a complete install/restart with a synthetic PDF restored and the EPSON default unchanged. The original native test exposed missing registration elevation before it was fixed. This run did not submit a new Word or physical printer job.

Linux uses the separate CUPS integration in `apps/pdfcraft-printer`; the Windows MSIX is not used there.
