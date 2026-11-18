---
name: lion-pdf
description: Build, fill in or edit PDF files with the Lion PDF MCP server (tools such as doc_create, page_add_text, form_add_field, doc_save). Use whenever a PDF is to be made or changed and the lion-pdf server is connected.
---

# Building PDFs with Lion PDF

Lion PDF's MCP server (`lion-pdf`, started by your client as `pdfcraft-cli mcp`) edits PDFs in
memory and writes them when you call `doc_save`. It can only read and write inside the folder the
person chose (Help ▸ Connect AI assistants… in Lion PDF). A path outside it fails with
`automation path rejected: outside the … root`: use a relative path or one inside that folder.

## Ground rules

- Pages are 1-based. Rectangles are `[left, top, right, bottom]` in points (1/72 in) from the
  **top-left** of the page as displayed. US Letter is 612 × 792, A4 is 595 × 842.
- Keep a 54–72 pt margin. On Letter with 72 pt margins the text area is x 72–540, y 72–720.
- Every edit is undoable (`edit_undo`) and nothing is written until `doc_save` with a `path`.
- Unknown arguments are refused. If a call fails, read the message: it says what is allowed.
  `command_list` and the schemas in `tools/list` are the source of truth.

## Workflow

1. **Start a document.**
   - `doc_create {"from": "blank", "paper": "letter"|"a4", "pages": N}` for an empty document,
     or `width`/`height` in points for another size.
   - `template_list`, then `doc_create {"from": "starter", "template": "<id>", "paper": "letter"}`
     for a ready-made layout (invoice, letter, resume, flyer, report, order form, planner…).
     Its placeholder words ("Item or service description", "YOUR ORGANIZATION") are page text:
     change them with `text_replace {"doc", "find", "replace"}` or `text_edit`. Starters already
     carry a footer, so check the preview before adding `doc_header_footer`.
   - `doc_create {"from": "text", "text": "…"}` to typeset plain text into pages quickly.
   - `doc_create {"from": "images", "paths": [...]}` for one page per picture.
   - `doc_open {"path": "…"}` to work on an existing PDF.
   Each returns `doc`, the id the other tools take.
2. **Lay out content, top to bottom.**
   - Text: `page_add_text {"doc", "page", "at": [x, y], "width": w, "text", "size", "bold",
     "italic", "font": "helvetica"|"times"|"courier", "color": "#RRGGBB", "align"}`.
     Always give `width` (or a `rect`): without it, long lines wrap at a default width. `\n`
     starts a new line and a blank line a new paragraph. `aligns` sets each line's alignment.
   - After adding text, `content_list {"doc"}` reports each item's `rect`; start the next block a
     few points below the previous block's bottom (`rect[3]`) instead of guessing heights.
   - Pictures: `page_add_image {"doc", "page", "path", "rect"}` (PNG, JPEG, TIFF, GIF, BMP).
   - More pages: `page_insert_blank {"doc", "at"}`; move or remove pages with `page_move`,
     `page_delete`.
3. **Add structure when it helps.**
   - Fillable forms: `form_add_field {"doc", "page", "type": "text"|"date"|"checkbox"|"radio"|
     "combo"|"list"|"button"|"signature", "rect", "name", "multiline", "options"}`. Put a label
     with `page_add_text` beside each field. Fill values with `form_fill {"doc", "values": {...}}`.
   - Bookmarks: `bookmark_add {"doc", "title", "page"}`. Links: `link_add {"doc", "page", "rect",
     "url"}` or `"to_page"`.
   - Header, footer and page numbers on every page: `doc_header_footer {"doc", "footer_center":
     "Page <<1 of n>>"}`. Watermark: `doc_watermark {"doc", "text": "DRAFT"}`. Page colour or a
     background picture: `doc_background`.
   - Title and author: `doc_set_info {"doc", "key": "Title", "value": "…"}` (also `Author`,
     `Subject`, `Keywords`).
4. **Check what you made.** `render_preview {"doc", "page", "max_side": 1024}` returns a PNG of the
   page. Look at every page for overlapping text, text running off the page and empty space, and
   fix it (`content_update` moves or restyles added text and images; `content_delete` removes
   them) before saving.
5. **Save.** `doc_save {"doc", "path": "name.pdf"}`. Tell the person the file name and folder.
   `doc_export_office` also writes Word, Excel, PowerPoint, HTML, RTF or Markdown copies.

## Editing an existing PDF

- Read it first: `doc_info`, `text_extract {"doc", "pages"}`, `text_find {"doc", "query"}`,
  `render_preview`.
- Change existing words in place with `text_replace` or `text_edit`; add new content with the
  tools above. Rotate, reorder, extract, split and combine with `page_rotate`, `page_move`,
  `page_extract`, `doc_split`, `doc_combine`.
- Remove sensitive content with `redact_mark` and `redact_apply`, never by covering it with a
  white box.
- Save to a new path unless the person asked to overwrite the original.

## Good habits

- Plan the page on a grid before calling tools: title (22–28 pt bold), headings (14–16 pt bold),
  body (10–12 pt), 4–8 pt between paragraphs and 18–24 pt between sections.
- Helvetica, Times and Courier cover Western European text. For other scripts, check
  `text_fonts`, or use `doc_create {"from": "text"}`, which picks a bundled face for Greek and
  Cyrillic.
- Work on one document at a time and keep its `doc` id. `doc_list` lists what is open.
- Close what you no longer need with `doc_close` (it refuses to drop unsaved changes unless you
  pass `discard_changes: true`).
