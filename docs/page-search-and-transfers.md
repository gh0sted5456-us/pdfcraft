# Search page sets and organize across PDFs

The personal 0.13.1 build adds these workflows to the existing page editor.

## Search

Advanced Search offers **Copy page numbers** (unique physical page numbers,
comma-separated in document order) and **Extract matching pages…**. The latter
selects the matching pages and opens the existing extraction dialog. Both actions
wait until the whole document has been searched; a failed page disables them so
an incomplete search cannot quietly become an incomplete extracted PDF. Search
uses the current whole-word and case options. The source stays open and unchanged.

The headless `text_search_pages` tool shares the renderer's matching rules:

```json
{"doc":1,"query":"invoice","whole_words":true,"extract":true,"out":"invoices.pdf","open":true}
```

Without `extract`, it returns `pages`, `count` and `page_numbers`. With extraction,
`extracted` contains the normal output/new-document result. Output paths retain
the automation root restrictions and atomic writes. An unreadable page refuses
the complete search. An empty result cannot be extracted.

## Organize Pages

Select thumbnails in the grid or Pages panel, drag them over another document's
tab, pause briefly, then drop at the blue insertion marker in its grid. The
destination grid scrolls near its top and bottom edges. A normal drop **copies**;
hold **Shift** at release to **move**. Each document has its own Undo. Dropping
outside a page area cancels. A source changed during the drag is refused.

Organize Pages → Insert → **Insert selected pages from an open PDF** offers the
other open PDFs and their current selected pages (or current page), inserting
after the destination selection without needing a file picker.

The engine and `page_transfer` tool work with the documents' unsaved working
content. Page lists are deduplicated and sorted in document order. Transfers
support 1–1000 pages, honor assembly permissions and rename colliding form fields
using the existing insertion policy; scripts referring to renamed fields are
not rewritten. A move must leave at least one source page.

```json
{"doc":2,"source":1,"pages":[1,3],"at":2,"move":false}
```

Positions are 1-based for tools; `at = page count + 1` appends. Neither document
is saved automatically. The destination accepts the pages before the source is
changed. If source removal fails, both copies remain and the tool reports
`moved: false` with a warning; the UI shows that warning. This is not a combined
Undo across two documents. External drag-out to Explorer/Desktop is not included.

## Adopted fixes

- [PR #4502](https://github.com/storytold/pdfcraft/pull/4502), @Lombardi2003,
  `b97d994a`: Preferences can hide validation overlays over signatures without
  changing signature validation, the banner, the panel or saved PDF artwork.
  The setting persists and accepts `--signature-badges true|false`.
  Document tooltips also respect modal dialogs instead of appearing over Preferences.
- [PR #4475](https://github.com/storytold/pdfcraft/pull/4475), @mtlcl410,
  `278d3e39`: an added text box/image drag preview renders the item alone on
  transparency, leaving underlying page imagery in place.
- [PR #4479](https://github.com/storytold/pdfcraft/pull/4479), @marcoarnulfo,
  `112104fc` / `74da5311`: Ctrl+Shift+Left/Right (Command+Shift on macOS)
  opens the previous/next PDF in the current file's folder, in natural filename
  order. Hidden/non-PDF files are excluded; existing tabs and unsaved changes
  are retained. The folder is scanned on a worker. This adaptation reports read
  errors, caps the PDF listing at 100,000 and ignores a scan after changing tabs.

The new search/transfer labels are currently English. The adopted preference and
folder labels retain the upstream translations, which are not native-reviewed.
No new dependencies, icons or font assets are added.
