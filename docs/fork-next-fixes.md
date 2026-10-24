# Candidates for the next personal-build pass

Reviewed 2026-10-24. This is a short list of existing upstream work to evaluate,
not a claim that these changes are included or fully reviewed locally.

| Priority | Existing work | Benefit | Remaining check |
|---|---|---|---|
| 1 | [#4371: invisible annotations and popup appearances](https://github.com/storytold/pdfcraft/pull/4371), merged upstream | Prevents unknown Invisible annotations and popup appearance streams painting unwanted page content. Applies to rendering and printing. | Review/backport the vendor and print changes, reproduce the synthetic rendering/printing regressions, and check interaction with our renderer patches. |
| 2 | [#4499: rejected form values](https://github.com/storytold/pdfcraft/pull/4499), open | Reverts a validation-rejected value and lets Tab move on, instead of trapping focus in the field. | Review the forms/engine/UI boundary, preserve format-error correction and verify Save/Undo behavior. |
| 3 | [#3972: Linux launcher activation](https://github.com/storytold/pdfcraft/pull/3972), open | Clears the launcher's busy cursor and gives the first Wayland window keyboard focus. | Review the winit patch and test on native Linux; our current Windows machine cannot validate compositor behavior. |

The previous pass adopted the narrower [#4327 recovery safeguard](https://github.com/storytold/pdfcraft/pull/4327),
with a clean-copy fix for the first recovery save. Large font/shaping, renderer
architecture and JavaScript-alert changes remain separate work. The follow-up
personal pass adds search page sets and moving/inserting pages between open PDFs,
plus the signature-overlay, drag-preview and folder-navigation proposals described
in [search and transfers](page-search-and-transfers.md). The three candidates in
the table above remain separate work.

Build on these authors' proposals rather than opening competing upstream PRs.
Public reports and PR descriptions are leads; inspect their actual code and
reproduce their tests before adopting another patch.
