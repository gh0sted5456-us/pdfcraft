# Upstream review, 2026-11-15

Status: integration branch built and gated (PR #15); craft-libs cutover stacked on it (PR #16). Neither merged into fork `main` yet.

Fork main: `b53d925b` (Merge five validated upstream certificate, audit, PDF/A, XFA and batch fixes, #13).
Upstream (storytold/pdfcraft) main at review: `a11ea7ad` (2026-11-15, #7450).
Common ancestor: `1dd2737b` (2026-10-26). Upstream has 585 first-parent units since then
(138 PR merges, 447 squash/direct commits).

## Method

1. Every upstream first-parent unit since the ancestor was cherry-picked **alone** onto fork
   `main` (`git cherry-pick -n`, `-m 1` for merges): 140 clean, 12 empty (already present),
   433 conflicting.
2. Clean units were sorted into product code, tests/docs and CI/packaging. Product units already
   ported under the same upstream number (#5987) were skipped.
3. The 106 clean product units were stacked in upstream order on a branch from fork `main`;
   all 106 stacked without conflict. Then 14 units whose only conflicts were in bookkeeping files
   (`parity/acrobat-features.toml`, `ROADMAP.md`, `docs/roadmap.md`, `vendor/README.md`) were
   added, keeping the fork's side of those files.
4. The stack was built with the fork's gates and fixed until green (below).

## Gates on the final branch

| Gate | Result |
| --- | --- |
| `cargo fmt --all --check` | pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (Linux x86_64) | pass |
| `cargo clippy -p pdfcraft -p pdfcraft-cli --target x86_64-pc-windows-gnu -- -D warnings` | pass |
| `cargo xtask layers` / `assets` / `parity` | pass |
| `cargo test --workspace --no-fail-fast` (Linux) | pass after the drops below; the two Unicode find-focus tests in `ui-egui/tests/it/viewer.rs` need a CJK UI face and pass with `CRAFT_FONTS_DIR` set |

Not checked here: macOS builds (#7203 xattr saves, #5757 print job names, #4785 panel languages,
#1893 PingFang are macOS-only code), the MSVC Windows toolchain, and native Windows Glass /
ICON_BIG behaviour. The Windows check above used the GNU target and compiles only.

## What "clean" hid: fixes made on the branch

A clean text merge was not enough for six units. The branch carries small, separate commits:

- **#4201 frame guard (panic in a frame no longer closes the app).** Upstream split
  `eframe::App::logic/ui` into guarded halves inside an inherent `impl`. Applied to the fork, that
  (a) moved the fork's `clear_color` out of `eframe::App`, which would silently drop the Glass and
  pasteboard clear colour (caught by clippy's dead-code lint), and (b) dropped the
  `eframe::Frame` that the fork's Windows title-bar colours and assistant browser sync read
  (caught only by the Windows target). Both fixed: `clear_color` is back on the trait, and the
  frame is passed through to `logic_unguarded`/`ui_unguarded`.
- **#5975 script output bounds.** Uses upstream's `pdfcraft_js::Message`; the fork's alerts are
  `String`. Adapted the two signatures.
- **#6544 middle-button auto-scroll release.** Its tests call a five-argument
  `update_for_platform` (upstream added inverted scrolling); a test helper maps onto the fork's
  four-argument form.
- **#5154 test** builds a `layout::Page` in the fork-only XLSX test without the new `rules`
  field; added it.
- **#4785 macOS panel languages** adds a test that `Info.plist.in` lists every UI language; the
  fork also ships `en-GB`, now declared.
- **#7203** raises `rustix` to 1.1.6 without the lockfile change; `Cargo.lock` updated
  (rustix 1.1.5 → 1.1.6 only).

## Dropped (apply cleanly as text, but do not build or pass on the fork)

| Upstream | Why |
| --- | --- |
| #5865 write a new document's page tree once | Removes `pdfcraft_create::add_page`, which the fork's templates and XPS converter use. |
| #5981 signature preview images within a work allowance | Needs `ImageBudget` / `image_file_within` from upstream M2.6 JBIG2 budgets, which conflict. |
| #6384 Delete key removes selected content object | Needs upstream's object-editing `delete_edit` / `EditableObject` model. |
| #6125 bookmark clipboard | Needs engine `outline_tree` / `ImportBookmarks` that the fork lacks. |
| #6997 deleting a field removes ancestors it leaves without kids | Builds, but overlaps the fork's own empty-ancestor pruning in `engine/src/clear_signature.rs`: `a_failure_part_way_leaves_the_document_unchanged` fails. Reconcile the two pruners before taking it. |
| #5740 panel resize handle vs scrollbar | Builds, but its test fails on the fork's side-panel layout (the Tools scrollbar sits within 12 px of the splitter). Needs a fork-specific check of the hit area. |
| #6013 (test only) inline /Annots widget removal | Upstream's commit adds only a test; it fails on Lion. See the finding below. |

## Included (113 upstream units)

### Crash, hang and resource-limit hardening (34)

- [#5673](https://github.com/storytold/pdfcraft/pull/5673) fix(render): JBIG2 arithmetic near the end of an i32 doesn't panic (hayro-jbig2 patch 12) (`6818995a3`)
- [#5581](https://github.com/storytold/pdfcraft/pull/5581) Reject signed and empty arcs in try_oid (`fdaa502f2`)
- [#5703](https://github.com/storytold/pdfcraft/pull/5703) Fix Matrix::invert rejecting valid matrices with a small scale (`8faebd0d9`)
- [#5715](https://github.com/storytold/pdfcraft/pull/5715) Reject '+' sign in page selection numbers (`d7dd1de64`)
- [#6065](https://github.com/storytold/pdfcraft/pull/6065) fix(ui): size the whole-page raster allowance to the pages on screen (`1f8cb4535`)
- [#6132](https://github.com/storytold/pdfcraft/pull/6132) M6: forms decodes field values, renamed calculations and detected pages within shared budgets (`c0cd40f2e`)
- [#5920](https://github.com/storytold/pdfcraft/pull/5920) fix(measure): charge snapping per filter stage and bound listing work (`eaeffa748`)
- [#5967](https://github.com/storytold/pdfcraft/pull/5967) fix(print): index page labels once and cap how many pages a range selects (`afaaaa641`)
- [#5975](https://github.com/storytold/pdfcraft/pull/5975) fix(engine): coalesce script resetForm requests and bound console output per edit (`ee6ae8eac`)
- [#6040](https://github.com/storytold/pdfcraft/pull/6040) fix(sign): read each existing DSS stream once and within a decoded-size allowance (`9642ed6f8`)
- [#6049](https://github.com/storytold/pdfcraft/pull/6049) fix(a11y): heading titles decode their source strings within a shared allowance (`74e1e40e6`)
- [#6454](https://github.com/storytold/pdfcraft/pull/6454) Apply Redactions replaces the recovery snapshot at once (`3c26e2e3e`)
- [#5282](https://github.com/storytold/pdfcraft/pull/5282) fix(organize): remove uninherited crop boxes and guard disjoint or overflowing boxes (`cf07ece17`)
- [#5458](https://github.com/storytold/pdfcraft/pull/5458) fix(compare): visual compare no longer overflows on declared sizes its buffers cannot hold (`7e90730fa`)
- [#4755](https://github.com/storytold/pdfcraft/pull/4755) Reject signs in XMP numeric character references (`6e2729ae2`)
- [#4931](https://github.com/storytold/pdfcraft/pull/4931) Reject '+' sign in print page ranges (`51655eab0`)
- [#5121](https://github.com/storytold/pdfcraft/pull/5121) fix(forms): name tree nodes sharing one long /Kids array no longer exhaust memory (`ab7af7234`)
- [#1882](https://github.com/storytold/pdfcraft/pull/1882) Refuse a legacy encryption key length above 128 bits instead of panicking (`344ba9f8d`)
- [#4201](https://github.com/storytold/pdfcraft/pull/4201) UI: a panic in a frame no longer closes the app and its tabs (`28093ed4c`)
- [#6007](https://github.com/storytold/pdfcraft/pull/6007) fix(create): bound a PNG's frame buffer by what its bytes can hold; no truncating JP2 box lengths (`b3793789f`)
- [#6081](https://github.com/storytold/pdfcraft/pull/6081) fix(xfa): strip reads a shared indirect Kids array once (`01c132a6b`)
- [#6090](https://github.com/storytold/pdfcraft/pull/6090) fix(a11y): auto-tagging bounds the content it plans across all pages (`3a900b8bd`)
- [#6096](https://github.com/storytold/pdfcraft/pull/6096) fix(sign): chain building hashes a certificate once for all candidate issuers (`ce3c1ef80`)
- [#6097](https://github.com/storytold/pdfcraft/pull/6097) fix(render): the named-destination index spends key bytes from one allowance (`910b32677`)
- [#5968](https://github.com/storytold/pdfcraft/pull/5968) fix(print,a11y): read summary scans one operator at a time and borrow form resources (`3fe86a118`)
- [#5983](https://github.com/storytold/pdfcraft/pull/5983) fix(redact): read the page tree once per sanitize pass, not per page (`4cdc3d15d`)
- [#5997](https://github.com/storytold/pdfcraft/pull/5997) fix(cli): cap input reads, budget check's edit pass and escape its stderr records (`b4679deaf`)
- [#6000](https://github.com/storytold/pdfcraft/pull/6000) fix(xfa): one decoding allowance for all XFA packets (`dafb3be3d`)
- [#6585](https://github.com/storytold/pdfcraft/pull/6585) Align stream decode cap with renderer at 256 MiB (`a1ea2d376`)
- [#6046](https://github.com/storytold/pdfcraft/pull/6046) fix(optimize): bound each unused-resource decode by the remaining shared budget (`ad3e6d4b8`)
- [#6918](https://github.com/storytold/pdfcraft/pull/6918) perf(organize): decode the XMP packet once for repeated document-information reads (`6b993d973`)
- [#7047](https://github.com/storytold/pdfcraft/pull/7047) fix(forms): finding the first character a simple font cannot hold encodes the whole string once, then each distinct character once (`785e7e510`)
- [#5491](https://github.com/storytold/pdfcraft/pull/5491) fix(render): a JBIG2 page's segments are bounded in what they hold and refer to (hayro-jbig2 patches 5 to 9, hayro-syntax 29) (`2cd50b8d3`)
- [#5086](https://github.com/storytold/pdfcraft/pull/5086) fix(render): cap a JPEG 2000 codestream's SIZ size at 2^28 pixels (`1b86ace58`)

### Rendering, printing and search (11)

- [#6526](https://github.com/storytold/pdfcraft/pull/6526) M2: a wide stroke of a path with `y` curves is drawn, not dropped (vello_common patch 4) (`c02341f6c`)
- [#4788](https://github.com/storytold/pdfcraft/pull/4788) fix(render): treat combined fill-and-stroke operators as a knockout group (`5e671ec30`)
- [#5874](https://github.com/storytold/pdfcraft/pull/5874) fix(content): an inline image only the renderer reads counts as ambiguous (`743a772ee`)
- [#5951](https://github.com/storytold/pdfcraft/pull/5951) fix(redact): read text-state operands from the end, as the renderer does (`ca56433f5`)
- [#5640](https://github.com/storytold/pdfcraft/pull/5640) M2: a shrunk image on a turned or mirrored page snaps to whole pixels (`b36bdaeac`)
- [#5896](https://github.com/storytold/pdfcraft/pull/5896) M2: a soft hyphen a substitute font lacks draws its hyphen (`1849b38c5`)
- [#5432](https://github.com/storytold/pdfcraft/pull/5432) perf(render): undo PNG predictors a byte row at a time (`899f3806b`)
- [#6595](https://github.com/storytold/pdfcraft/pull/6595) M3: Find matches words divided by soft hyphens, zero width characters or a Chinese or Japanese line end (view.find) (`87bc00a99`)
- [#5002](https://github.com/storytold/pdfcraft/pull/5002) M10: preserve annotation placement precision (`debb826f2`)
- [#6855](https://github.com/storytold/pdfcraft/pull/6855) M10: retain printed artwork with indirect annotation metadata (lands #4668) (`afca38ad7`)
- [#5028](https://github.com/storytold/pdfcraft/pull/5028) M10 #5022: preserve printable Link appearances (`851d6ceca`)

### Forms, XFA and JavaScript (13)

- [#5819](https://github.com/storytold/pdfcraft/pull/5819) M12: FormCalc prints numbers to at most 15 significant digits (`97a911053`)
- [#5476](https://github.com/storytold/pdfcraft/pull/5476) M12: an XFA check box without a width is as wide as its box and caption (`83cbf8f89`)
- [#5250](https://github.com/storytold/pdfcraft/pull/5250) fix(forms): clearing a field's length limit leaves it with none, not its parent field's (`9070431ed`)
- [#6120](https://github.com/storytold/pdfcraft/pull/6120) fix(xfa): read and validate the first alternative in typed_date (`9b2c8d602`)
- [#6122](https://github.com/storytold/pdfcraft/pull/6122) fix(js): decode FormCalc Unicode string escapes and preserve chained unary not/! (`19619ad2c`)
- [#6445](https://github.com/storytold/pdfcraft/pull/6445) M6: a widget whose /Parent names a missing object is a field of its own (form.field-list) (`608ff62da`)
- [#6567](https://github.com/storytold/pdfcraft/pull/6567) M6: a product of empty fields is 0, not 1 (form.af-simple-calculate) (`b9ffb68b1`)
- [#5364](https://github.com/storytold/pdfcraft/pull/5364) fix(forms): resolve field and widget references in ResetForm and Hide actions (`a22bede98`)
- [#5109](https://github.com/storytold/pdfcraft/pull/5109) M12: an XFA caption beside its field wraps in its reserve (XFA 3.3 p. 645) (`1ec71c1bf`)
- [#6035](https://github.com/storytold/pdfcraft/pull/6035) fix(xfa): remember every picture hash written in a pass (`ce1cd8047`)
- [#6077](https://github.com/storytold/pdfcraft/pull/6077) fix(forms): comb detection ignores repeated boxes (`f64c853a3`)
- [#6080](https://github.com/storytold/pdfcraft/pull/6080) fix(xfa): existing-field traversal shares a field's name among its queued children (`237838102`)
- [#5074](https://github.com/storytold/pdfcraft/pull/5074) M12: XFA date pictures read a two-digit year with the century split of 30 (`048386eea`)

### Signing and security (8)

- [#6208](https://github.com/storytold/pdfcraft/pull/6208) Signing a form that sets NeedAppearances no longer makes Acrobat ignore the signature (`82c2009d0`)
- [#6137](https://github.com/storytold/pdfcraft/pull/6137) fix(sign): merge CMS unsignedAttrs into one SET and validate SignerIdentifier (`5ee4a8587`)
- [#4569](https://github.com/storytold/pdfcraft/pull/4569) automation: compare canonical root components exactly and test roots-only escape confinement (`a67f5bb35`)
- [#6039](https://github.com/storytold/pdfcraft/pull/6039) fix(engine): summarizing comments requires copying permission (`5f4003e56`)
- [#5965](https://github.com/storytold/pdfcraft/pull/5965) fix(automation): sign_trust deduplicates certificates with a set (`0d4074755`)
- [#5990](https://github.com/storytold/pdfcraft/pull/5990) fix(cli): render needs the copying permission, as text does (`ba47f7bbc`)
- [#7038](https://github.com/storytold/pdfcraft/pull/7038) fix(sign): every signature's VRI entry shares the store's evidence arrays instead of copying them (`b167422b2`)
- [#6398](https://github.com/storytold/pdfcraft/pull/6398) M1: a V4 RC4 file with a 40- or 48-bit key reads and saves (`e6b6216fd`)

### Organize, redact, optimize, export (14)

- [#5868](https://github.com/storytold/pdfcraft/pull/5868) fix(redact): rewritten form XObjects drop /PCAdded (`69ddcd89b`)
- [#5866](https://github.com/storytold/pdfcraft/pull/5866) fix(redact): structure titles go with the descriptions of removed content (`359ed56ff`)
- [#5766](https://github.com/storytold/pdfcraft/pull/5766) Expand header/footer tokens preceded by extra '<' characters (`872a77501`)
- [#5742](https://github.com/storytold/pdfcraft/pull/5742) M11: Optimize runs when a category it discards holds nothing (`8582c84da`)
- [#5154](https://github.com/storytold/pdfcraft/pull/5154) Keep ruled tables when exporting a PDF to Word (`f75058981`)
- [#6134](https://github.com/storytold/pdfcraft/pull/6134) fix(export): skip empty Markdown runs before merging and emphasis lookahead (`5d3f3a2e4`)
- [#5271](https://github.com/storytold/pdfcraft/pull/5271) Track XMP attribute quotes only inside tags (`475ba82fe`)
- [#4901](https://github.com/storytold/pdfcraft/pull/4901) Keep the outline intact when move_bookmark gets an invalid target (`ebcc564f1`)
- [#4951](https://github.com/storytold/pdfcraft/pull/4951) fix(optimize): size objects in compressed object streams by their compressed share (`9011276f2`)
- [#5612](https://github.com/storytold/pdfcraft/pull/5612) fix(organize): preserve numbered names, indirect resource dicts, and comment threads (`778ec3fc1`)
- [#6036](https://github.com/storytold/pdfcraft/pull/6036) fix(xfdf): FDF comment export excludes source pages by identity, not by /Type (`3a1a2272f`)
- [#2554](https://github.com/storytold/pdfcraft/pull/2554) export: the Office zip writer outgrows the classic records with ZIP64 (`e3f2f2ebf`)
- [#2799](https://github.com/storytold/pdfcraft/pull/2799) fix(organize): preserve page labels across extract, split, and combine (`938f56989`)
- [#6996](https://github.com/storytold/pdfcraft/pull/6996) redact: a covered clip-and-paint path leaves an empty clip, not its outline (`f44ea8380`)

### Viewer, UI and platform (20)

- [#5608](https://github.com/storytold/pdfcraft/pull/5608) M3: the wheel scrolls right after Alt+Tab back, before the mouse moves (`a74e3d471`)
- [#5748](https://github.com/storytold/pdfcraft/pull/5748) fix(ui): prevent wheel lockup at edge in non-continuous view (`661ded72b`)
- [#6302](https://github.com/storytold/pdfcraft/pull/6302) The PDF/A panel reports its own checks, not conformance (`9ff881e53`)
- [#5562](https://github.com/storytold/pdfcraft/pull/5562) Select the bookmark title when creating or renaming a bookmark (`04f3421c9`)
- [#5762](https://github.com/storytold/pdfcraft/pull/5762) M3: a protected file named before others at launch keeps its password prompt (`607f79e5e`)
- [#6544](https://github.com/storytold/pdfcraft/pull/6544) fix(ui-egui): stop held middle-button auto-scroll on release (`bb8fbd6b6`)
- [#6559](https://github.com/storytold/pdfcraft/pull/6559) M10: Pages per sheet takes a custom grid of columns by rows (`3e48317ae`)
- [#5260](https://github.com/storytold/pdfcraft/pull/5260) docs: a portable copy keeps its single-instance socket in %LOCALAPPDATA% (`2d3e3a653`)
- [#5278](https://github.com/storytold/pdfcraft/pull/5278) Reload: a file changed under an open dialog waits for it to close (`4fa26f4a2`)
- [#4785](https://github.com/storytold/pdfcraft/pull/4785) macOS: declare the UI languages so the open and save panels are translated (`138acb38b`)
- [#5218](https://github.com/storytold/pdfcraft/pull/5218) Edit background, watermark and header colours as sRGB (`ea292f6be`)
- [#1893](https://github.com/storytold/pdfcraft/pull/1893) Use PingFang from the reserved font directory on macOS 26 (`4b5244863`)
- [#3547](https://github.com/storytold/pdfcraft/pull/3547) Search: centre the active match in the viewport (`cb919f0d3`)
- [#4681](https://github.com/storytold/pdfcraft/pull/4681) fix(app): set Windows taskbar and Alt+Tab big icon (ICON_BIG) (`d4828fe54`)
- [#4953](https://github.com/storytold/pdfcraft/pull/4953) feat(print): add Folio and F4 paper presets (`dc9d120fc`)
- [#7203](https://github.com/storytold/pdfcraft/pull/7203) Fix macOS saves for PDFs with extended attributes (`346d80308`)
- [#7170](https://github.com/storytold/pdfcraft/pull/7170) fix(ui): distinguish insert text annotation icon (`1a5cf6b02`)
- [#5091](https://github.com/storytold/pdfcraft/pull/5091) fix(web): open document links with noopener and send COOP same-origin (`10727c520`)
- [#5757](https://github.com/storytold/pdfcraft/pull/5757) M10: macOS print jobs name their title and owner (`bf0f3c9f7`)
- [#4544](https://github.com/storytold/pdfcraft/pull/4544) M5: a double-click on a comment card's text edits it (`04cee34bf`)

### Engine, accessibility, measure and other fixes (13)

- [#5566](https://github.com/storytold/pdfcraft/pull/5566) Ignore a negative trailer /Size in incremental saves (`2289b5755`)
- [#5263](https://github.com/storytold/pdfcraft/pull/5263) fix(a11y): splice named and multi-stream marked content in mark_tag_artifact (`49d5997e1`)
- [#6486](https://github.com/storytold/pdfcraft/pull/6486) Add dry-run preview to doc_batch without writing outputs (`5bb8bbf20`)
- [#4878](https://github.com/storytold/pdfcraft/pull/4878) perf(cos): write hex strings, integers and references without the formatter (`ed71db7c0`)
- [#4874](https://github.com/storytold/pdfcraft/pull/4874) perf(cos): speed up byte searches during cross-reference reconstruction (`efe75d3a3`)
- [#6536](https://github.com/storytold/pdfcraft/pull/6536) fix(engine): map URL text line rects with view_rect_to_user and filter non-finite coordinates (`90def111c`)
- [#6158](https://github.com/storytold/pdfcraft/pull/6158) fix(fonts): a subset that gives the space no width can't encode one (`2ef2e2b82`)
- [#3275](https://github.com/storytold/pdfcraft/pull/3275) fix(edit): preserve annotation optional-content membership in marked content (`9792328d6`)
- [#5611](https://github.com/storytold/pdfcraft/pull/5611) fix(a11y): resolve indirect structure kids, parent tree keys, and table attributes (`7b4a90487`)
- [#5270](https://github.com/storytold/pdfcraft/pull/5270) fix(measure): resolve indirect scale objects and validate closed area edges (`4f0b92689`)
- [#5273](https://github.com/storytold/pdfcraft/pull/5273) fix(geom): normalize quad ordering for perspective foreshortening and scale (`676480992`)
- [#5941](https://github.com/storytold/pdfcraft/pull/5941) fix(model,oracle): read a page box only when it has four entries; skip empty images (`44a26f4b9`)
- [#5982](https://github.com/storytold/pdfcraft/pull/5982) fix(engine): open linked documents without blocking and check the handle is a regular file (`8f29ed66f`)

## Finding: signature validation after removing a widget from inline /Annots

Upstream #6013's test signs a fixture, then empties the signed page's inline `/Annots` in a later
revision, and expects the signature to read **Invalid / Disallowed(other changes)** (upstream CI
runs it; not re-run here). Lion reports it **Valid / Allowed(comments)**: a signed widget can be
stripped and the signature still shows as valid with only comment changes. The fork already has the indirect-array case
(`an_indirect_annots_array_is_judged_by_the_annotations_it_gains`). Upstream handles the inline
case in its reworked `crates/sign/src/pdf/changes.rs` (classify changes by the role each object
had when signed: ce3857c2e, de344a4bb, ddeca878d, 560c68db8 and follow-ups), which conflicts with
the fork and needs a deliberate port. This is the highest-value item not in this batch.

## Clean but left out on purpose

- CI and release workflow changes (#7026, #7040, #6921, #7218, #6200, release permissions): the
  fork has its own release pipeline and registry defaults.
- Flatpak packaging tweaks (#5490, #4641, eu-strip skips) and the portable-copy handler
  registration (#3790): packaging, not qualified natively here.
- Tests, fuzz targets and README examples only (18 units: #6296, #6301, #6303–#6305, #6308,
  #6183, #6186, #6189, #6191, #6204, #5252, #5560, #2483, #4683, #4933, #5988, XFA scale timing).
  Safe to add later as a separate test-only pass.

## craft-libs adoption (PDF and codecs), branch `lion/craft-libs-clean`

Decision (owner, 2026-11-15): adopt craft-libs for the PDF layer and codecs; keep Lion's own
translations (no craft-i18n) and updater (no craft-update), and Lion's craft-ui/quit/keymap/print pins.

- `pdfcraft-cos`, `pdfcraft-crypt` and `pdfcraft-filters` are re-exports of craft-pdf, as upstream
  made them in #7261. craft-pdf is vendored at upstream's craft-libs pin `e2dc681a3`
  (`vendor/craft-pdf`), because Lion's certificate (public-key) security needs the private
  handler constructor; it is carried as a marked `Lion patch:` (vendor/README.md). Every upstream
  cos/crypt/filters fix up to that pin comes with it.
- Also taken: #7260 (zip through craft-zip), the RTF/CSV part of #7261 (craft-rtf, craft-csv),
  #7374 (RTF export through craft-rtf's writer), and upstream's hayro-jbig2 patch level
  (craft-stream-codecs needs its decoding allowance API; Lion's copy had nothing of its own).
- Not taken yet: #7326 (craft-xml, image codecs, Office packaging). It conflicts in 45 files
  because it sits on upstream XFA, OCR and redaction work Lion lacks; it is the next stage.
- Two tests follow craft-pdf's behaviour as upstream's did (a stream with a fitting `/Length`
  reads without `endstream`; a broken object stream has no members). `xtask parity` resolves
  `craft-pdf::` evidence against the vendored copy.
- Re-sweep: against the cutover, 9 more of the 433 conflicting upstream units apply cleanly and
  15 conflict only in bookkeeping files. Excluding ones already in PR #15 or dropped there, 12
  are new; 11 are stacked on the branch (#3858, #6505, #6457, #6686, #2075, #6005, #4282, #6731,
  #4703, #5036, #7115). #6619 needs upstream's text-module rework and was left out.

## Still conflicting

433 units conflict on their own against fork `main`. The most contested files are
`parity/acrobat-features.toml` (98), `ROADMAP.md` (55), `crates/automation/src/tools.rs` (49),
`Cargo.lock`/`Cargo.toml` (47/34), `crates/ui-egui/src/canvas.rs` (40). Upstream's craft-libs
cutover (#7326 and its predecessors) is the largest structural divergence; most later upstream
fixes in codecs, XFA XML and export packaging sit on top of it and will keep conflicting until
the fork decides whether to adopt craft-libs.
