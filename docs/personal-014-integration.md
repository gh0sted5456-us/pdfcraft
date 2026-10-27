# Personal fork 0.14.0 integration

> Last updated: 2026-10-25 · Change: major (upstream integration and Glass backdrop correction).

Upstream baseline: `storytold/pdfcraft` commit `d764a8eb` (0.14.0). This merges its history into the personal fork based on `316bcc3c`, retaining the custom feature branch at `0e959b61`.

The upstream changes include faster large XFA forms, corrected image and text drag previews, attachment context actions, page tools, Persian localization, form and rendering fixes, and improved Word export. The personal printer, thumbnail provider, page locks, signature protection, automatic OCR page-choice prompt, group selection, templates, Assistant setup and centered dialog conventions remain.

Glass paints its tint and opacity once in the native window backdrop. Main panes, cards and fields are transparent above that backdrop; nesting them no longer compounds opacity. Menus, dialogs and floating panels retain a tinted backing for readability over documents. PDF pages and their content retain their original opacity. Windows Glass uses the app header; changing back to Light, Dark or System restores the selected window frame.

The personal movable and floating workspace is retained alongside upstream's docked task panes for Read mode and the expanded All tools view. Both UI control APIs remain: `ui.dock` for layout operations and `ui.panel` for pane visibility and width. Saved floating layouts are preserved rather than silently migrated to dock-only panels.

The upstream single-item deletion batch helper is superseded by the fork's atomic `DeleteObjects` engine operation. Its two helper-only unit tests are covered by the end-to-end deletion tests in `tests/it/object_selection.rs`, including mixed selections, shapes, images, typing, undo and reopen behavior. Upstream scan recognition retains the personal current-page/all-pages prompt and document-generation guard.

ArtCraft trademark graphics and the screenshot containing them remain excluded from this modified fork. Original project credits and license terms are retained.

## Validation

Validation: 2,130 backend tests, 548 UI library tests and 1,234 UI interaction tests passed (19 tests ignored). Strict Clippy passed across the affected core/UI crates. Formatting, asset provenance, dependency layering and parity checks passed. WebAssembly UI/engine and Linux printer compile checks passed; the WebAssembly check retains existing desktop-only/dead-code warnings.

An isolated native Windows run checked Home at 35%, 67% and 100% opacity, document display, settings, floating panels and maximized mode. Pixel samples across the main header, tools, canvas and nested Home cards shared the requested alpha; PDF content remained opaque. Properties, marks and optimizer dialogs retain bounded scrolling and centered headings/actions in the small-window layout tests.
 Native checks use an isolated portable app and synthetic documents; they do not install the printer or change the default printer. Linux/macOS desktop compositing and printer runtime behavior require their respective systems. Glass requires a compositor supporting per-pixel alpha; no blur effect is claimed.

## egui 0.37 candidate — 2026-10-27

The isolated `codex/egui-037-integration` branch integrates upstream through
`1dd2737b`: egui 0.37, shared menus and the directory-layout merge. It preserves
the personal workflows and manual updater. All 17,591 original definitions are
accounted for after the move. Combined validation passed 5,033 tests (25 ignored),
strict lint, formatting, assets/layering/parity and web/WASM compilation.

Native Windows checks cover header restoration in all themes, Assistant theme
retention and organizer locks. A follow-up preserves normal client size when Windows
changes frame decorations; repeated normal restart now retains 1000×700 instead of
growing by 16×39. Its focused regression and refreshed MSI/PE checks passed. Initial
Linux printer CI passed; ARM64 packaging hit upstream OCR model HTTP 403. Native marquee selection of original text was verified. The owner confirmed native group dragging and arrow-key nudging both work in the
synthetic preview. Linux/macOS and CAC hardware remain unqualified; see [the migration record](egui-037-migration-plan.md).
The inline editor no longer redraws selected unchanged glyphs over the PDF. A
translucent selection preserves the original ink; typed text and undo are covered
by the rendering regression. All 187 editing tests pass (4 ignored).
The Windows preview installer passed MSI and GUI/CLI subsystem validation; it has
not been installed. Main promotion is tracked in fork PR #1; the daily installation
remains separate from preview qualification.

## Revision history

- 2026-10-27: Integrate the egui 0.37/menu and module-layout candidate on a separate branch; record combined qualification and remaining native checks.

- 2026-10-25: Integrated upstream 0.14.0 and corrected accumulation of Glass tint across nested main-window surfaces.

## Maintaining the personal fork

Use an integration branch from `fork/main` for each upstream update. Fetch
`origin/main`, inspect its changes, and merge its history normally; do not squash
upstream integration commits. Preserve personal workflow regressions when resolving
conflicts. Merge into the fork only after its CI and platform checks pass.

The fork release workflow runs after successful main-branch CI. It selects the next
fork version, packages the desktop and optional printer, and publishes verified
GitHub release assets. The manual updater in Preferences reads these fork releases;
it never silently switches users to upstream installers. Keep its `craft-fonts`
revision aligned with CI and keep model downloads pinned by revision and SHA-256.
Shared CI workflows are pinned to `86bc2ce4`; update that pin deliberately during
integration. The original OCR model bytes are preserved in the fork's
`build-inputs-v1` prerelease, with the author's CC-BY-SA-4.0 licences and unchanged
checksums. This build-input release is excluded from the normal app updater.

For each update, check the native frame in Light/Dark/Glass, a small-window dialog,
original/addition group dragging and nudging, organizer locks, save/reopen, OCR,
and Assistant restore without changing appearance preferences. CAC and non-Windows
runtime checks require their respective hardware or systems. A platform CI failure
is a release blocker, not a reason to disable the gate.
