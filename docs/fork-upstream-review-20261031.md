# Upstream review, 2026-10-31

Fork main: `1ef732f5d919d1cd9ee859989b8503c5dda18507` (published Lion PDF 1.0.6).
Upstream main at review: `eea00a6186546762d11d857666249672bff3fcad`.
Git reports 537 upstream-only commits and 224 fork-only commits from their common
ancestor `1dd2737b47a9c1c9562f0af563053d1bf510e4bf`. These are ancestry counts;
they include proposals already ported into the fork and are not 537 missing fixes.
A full `git merge-tree --write-tree` simulation reports conflicts and changes no
working files. The selected integration below follows the initial review; the full upstream tree remains unmerged.

| Change | Assessment against this fork |
| --- | --- |
| [Optional page-edge shadows, #2088](https://github.com/storytold/pdfcraft/pull/2088) | Ported into the existing Appearance rail, viewer and organizer, with persistence, headless control and upstream rendering tests; native rendering/persistence/control qualification passed. |
| [Two-page command-line launch, #1433](https://github.com/storytold/pdfcraft/pull/1433) | Ported per-file display overrides and password/background handling; retained the fork's certificate prompts, session restart and parser. Password retry/cancel, background loading, Glass, WebAssembly compilation and five native CLI/help/version cases passed. |
| [Asset licensing checks, #2212](https://github.com/storytold/pdfcraft/pull/2212) | Useful validation; reconcile with the fork's approved dependency exceptions and asset inventory. |
| [Compact save after page deletion, #4279](https://github.com/storytold/pdfcraft/pull/4279) | The fork already requires a full save for unsigned page deletion, including batches. Compare tests rather than duplicating the fix. |
| [Windows CJK viewer fix, #4172](https://github.com/storytold/pdfcraft/pull/4172) | Useful rendering candidate; changed rendering/dependency/UI paths need qualification. |
| [Flatpak OCI publishing, #5526](https://github.com/storytold/pdfcraft/pull/5526) | Cherry-picked as `fedaf5ba`; fork registry defaults, stdin authentication and cleanup regression added. No native Flatpak build or publication performed. |
| [Update result dialog, #5527](https://github.com/storytold/pdfcraft/pull/5527) | Reconcile deliberately with this fork's staged installer and open-document restart flow. |
| [Parity integration, #5426](https://github.com/storytold/pdfcraft/pull/5426) | Broad 230-file rendering/editing/forms integration; unsuitable for a blind merge. |

Seven of the eight reviewed changes do not pass a direct patch-application check;
that check alone does not prove a three-way cherry-pick could never be resolved.
Detailed patch and merge checks are retained with the H: project validation evidence.

The independent printer-retirement change removes the virtual printer from future
fork releases while retaining the installed updater's filename contract. It does
not uninstall an existing printer, merge upstream UI or publish a new release.

Integration preserves @gh0sted5456-us as the owner's contributor credit. The virtual
printer retirement and updater compatibility remain in the branch. Larger CJK,
shared-updater and parity migrations require separate qualification.
