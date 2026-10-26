# Select, align, move and delete page objects

## Group selection

The square **Group select** button in the floating tool strip draws a translucent blue
rectangle. It selects visible top-level comments (including sticky notes, strikeouts,
text boxes and stamps) and PdfCraft-added text/images that the rectangle touches. Original
page artwork and a scanned background are left alone outside Edit mode. In **Edit** mode
(or with the Edit a PDF panel chosen), it also selects original editable paragraphs, pictures
and vector paths. Click selects one item; Shift/Ctrl/Command
adds to or toggles the selection. Escape clears it. Delete/Backspace removes the group in
one undo step. Hidden comments are excluded; locked or missing targets refuse the entire edit.
Drag any selected item or the group frame to move the whole selection; blue outlines preview
the new positions. One move is one Undo, and the selection remains for another move when
objects can be identified unambiguously after text regrouping. Unsupported or clipped original
content refuses the entire mixed edit. Selecting the tool first commits open text; refused or
undrawable drafts stay open, and the tool does not change.
Arrow keys nudge the selection by one displayed-page point; Shift plus an arrow moves ten.
Directions follow the screen at any zoom or view rotation. Each press is one Undo step.
Text fields and modal dialogs keep their normal keyboard handling.
Selections stay on one page and clear after an unrelated edit, undo or tool change. Interrupted
drags are cancelled on tab/focus changes and dialogs. Group resizing is not included.
The control channel accepts `set quick=group-select` and reports `group_selection_count`.
Headless callers use `comment_delete` or `additions_move` with `indices` and/or `content_indices`.
`additions_move.offset` is [right, down] in displayed-page points, before view-only rotation.
Both tools also accept `objects` from `object_list` for original text/images/paths, with an
optional `generation` to refuse stale references. Added items use `content_indices`.

In **Edit a PDF → Edit text & images**, select several items on the same page:

- **Shift-click**, **Ctrl-click**, or **Command-click** toggles an item. An ordinary selected
  paragraph, image, or added item becomes the first member when you extend its selection.
- Drag through empty page space to draw a selection rectangle. Items intersecting the rectangle
  are selected; hold one of those modifiers to add to the selection.
- Press **Delete** or **Backspace** to remove the selected objects as one undo step. Text being
  typed keeps its normal character-deletion keys; select the paragraph frame to delete the box.
  A selected image can be deleted even after the pointer moves away from it.
- Drag the selection to move all its items by the same distance. Blue outlines preview the move;
  releasing commits it as **one undo step**. Escape cancels the gesture or clears the selection.
- Magenta guides appear when a moving box's edge or centre lines up with another object. The
  box snaps within six screen points, at every zoom. Hold **Alt** while dragging to bypass snapping.
- Right-click an object or selection for **Align**, **Distribute**, and **Delete**. Alignment uses
  the selection's outer edges or centre. Distribution keeps the outermost objects and makes the
  gaps equal. Select two objects to align, or three to distribute. Each action has one undo step.
- Switching tabs, opening Home or a modal dialog, or losing window focus cancels a drag preview.
  Returning to the document keeps the selection without committing a release from elsewhere.
- Modifier-clicks prefer added items and text over background artwork, so labels remain selectable.
- Clear the selection or double-click an item to return to ordinary text editing and image resizing.
  Finish or discard a text draft before starting a multi-selection.

While editing an original paragraph or an added text box, **Ctrl+L**, **Ctrl+E** and **Ctrl+R**
set its alignment to left, centre and right (Command on macOS). They also work on a selected
added text box. These use the existing whole-paragraph/whole-box formatting; highlighting a
few characters does not give those characters a separate alignment. Drafts stay open, and the
style is saved when the text is finished. Formatting committed text is undoable. Search, form
fields and other inputs keep their keys; outside document text, Ctrl+L still toggles full screen.

The supported items are the existing paragraph boxes, raster Image XObjects, whole Form artwork,
vector paths painted into the page, and text/images added by PdfCraft. A Form remains one item, including its nested artwork and
resources. Added items appear once in the selection inventory. Selections stay on one page and
contain at most 1,000 items; pages with more than 10,000 recognised objects are refused for group
selection. Content streams must decode completely within 64 MiB each and 128 MiB per page, with
a 1,024-level graphics-state limit and 100,000 content operations. These limits apply to both the
original page and the moved result; unreadable or missing streams are refused. Moving between pages,
inline images, group resizing,
and vertical or clipping text are outside this operation's scope. A refusal changes nothing.

Added streams move by changing their outer placement, preserving drawing bytes, fonts, crop paths,
and image transforms even if the page was rotated after the item was added. Their original local
coordinate basis is retained in `/PCAdded/MoveOrigin` so metadata and source placement agree after
repeated moves. Ordinary content updates replace that stream and reset the basis.

Existing text moves without retyping, font substitution, or reflow: its original encoded glyph
strings and kerning are replayed with temporary numeric `TJ` adjustments and text rise. The reader
performs the original glyph advances, so following unselected text keeps its position. Paragraph
indices can change when detection groups lines differently after a move. The UI remaps its own
selection only when the new geometry matches uniquely; other document edits clear stale selection.

## Engine

`Edit::DeleteObjects { page, objects }` removes a mixed selection atomically. Missing, repeated,
oversized or unsupported references refuse the whole edit. A refusal preserves the selection
and document; undo restores all deleted objects together. Selected paragraphs are cleared together from the original
source inventory, preserving the text advances of unselected neighbours. This is ordinary editing, not secure redaction: earlier
revisions and unused resources can retain deleted data.

`Document::editable_objects(page)` returns a mixed inventory with zero-based `ObjectTarget`
references and user-space rectangles. Apply `Edit::MoveObjects { page, objects, offset }` through
`Session`. Targets resolve together before mutation. Permission checks, dirty state, history,
incremental saving, and refreshing use the same transaction as other edits.

`Document::object_move_offset(page, [dx, dy])` converts a displayed-page vector (points, right/down)
to the user-space vector required by the edit, accounting for the crop box, `/Rotate`, and `/UserUnit`.
A frontend with an additional view rotation converts its pointer positions to displayed page space
first. Duplicate, missing, non-finite, oversized, or unsupported targets fail atomically. Engine callers
re-list after changes; the automation generation guard detects stale references before the edit.

`Edit::AlignObjects { page, objects, how }` plans a separate translation for each selected object
and commits them together. The headless `object_align` tool takes the same references and optional
generation as `object_move`, plus `alignment`: `left`, `center_horizontal`, `right`, `top`,
`center_vertical`, `bottom`, `distribute_horizontal`, or `distribute_vertical`. Clipped vector
paths are refused; source glyph bytes and object sizes are retained. Detection can regroup source
paragraphs after movement, so re-list references after every edit.

The alignment core builds on [PR #1990](https://github.com/storytold/pdfcraft/pull/1990),
commit `c20efc57`, by @pauliusiv123-rgb, adapted to this branch's mixed-object inventory.

## CLI and MCP

Both frontends use the same tools. `object_list` returns a document `generation` and items with
`kind` (`added`, `text`, `image`, or `path`), 1-based `index`, `type`, displayed `rect`, and optional `text`.
Treat those references as belonging to that generation and pass it back when moving or deleting:

```json
[
  {"tool":"doc_open","args":{"path":"input.pdf"}},
  {"tool":"object_list","args":{"doc":1,"page":1}}
]
```

After choosing references from the returned list, use a move call like this in the same MCP
session. For a CLI script, include `doc_open` in that script before the move:

```json
[
  {"tool":"object_move","args":{
    "doc":1,"page":1,"generation":0,
    "objects":[{"kind":"text","index":1},{"kind":"image","index":1}],
    "offset":[24,18]
  }},
  {"tool":"doc_save","args":{"doc":1,"path":"moved.pdf"}}
]
```

To delete the same selection, use `object_delete` with `doc`, `page`, `generation` and `objects`
(the same references), without `offset`. It returns `deleted` and creates one undo step.

Use the actual generation and references from your document, rather than assuming these example
values. `offset` is in displayed points: positive x moves right, positive y moves down. The optional
generation guard rejects a changed document before interpreting references. Re-list after any edit,
undo, redo, or save; there is no claim that an index persists across generations. Run a script with
`pdfcraft-cli run --script steps.json --root DIR` or call these tools through the opt-in MCP server.

Regression coverage: `crates/edit/src/tests.rs` (glyph bytes, neighbours, split streams, shared Forms,
rotated/cropped/UserUnit pages and refusal), `crates/engine/src/tests.rs` (permissions, history,
rendering and save/reopen), `crates/automation/tests/it/automation.rs` (generation guards and hostile
references), and `crates/ui-egui/tests/it/object_selection.rs` (real-shell selection and gestures).


## Saving after cutting pages

Cut or Delete in Organize Pages removes the chosen pages. For an unsigned PDF, ordinary Save
rewrites the remaining document and reclaims unreachable page content and unused shared image
resources, without downsampling or rasterizing the retained pages. Undo history and the page
clipboard remain available in the current session. The saved file has one current revision.

Signed PDFs retain incremental saving to preserve their signed bytes; older signed revisions can
still contain removed pages. Cutting pages is not secure redaction: text may also occur in retained
bookmarks, metadata or attachments. Existing shared resources are kept when their use is uncertain.

The full-save policy builds on @srt8hellcat2-ui’s upstream PR #4279. Regression coverage includes
ordinary Cut → Save, shared-image reclamation, undo/save/reopen, and preservation of signatures.
