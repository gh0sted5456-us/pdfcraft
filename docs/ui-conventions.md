# Personal build UI conventions

> Updated 2026-10-24: centered, bounded dialogs and shared semantic action styling.

Modal dialogs and dialog-style prompts are centered in the application content rectangle. Use `widgets::show_modal`: it provides theme-aware padding, a centered card and bounded scrolling so long or translated content stays inside the window. A movable field-table window opens with its center pivot at the content center.

Use `widgets::dialog_footer` for action rows so they occupy their measured height instead of vertically centering in unused scroll space.

Use `widgets::dialog_button` for action buttons, including comment editors and progress cancellation. Roles are semantic and independent of the displayed wording:

| Role | Appearance | Examples |
| --- | --- | --- |
| Back | Neutral | Back, Browse, open another configuration step |
| Cancel | Blue | Cancel, Close, return to the document |
| Confirm | Green | Save, Enter/OK, Continue, Sign, Post, apply selected pages |
| Destructive | Red | Exit without saving, Discard, Delete, Remove |

All roles use the shared matte theme fill at rest, matching dimensions and focus outlines; hover/press introduce the subdued semantic color. Preserve the same keyboard actions, disabled state and accessible labels when replacing a button. Shared layout tests cover both axes, changing content size and smaller windows; rendered light/dark checks cover appearance and action colors. These conventions apply to new dialogs too.

Labelled interface checkboxes support dragging across adjacent boxes to apply the starting box’s new value. Disabled, clipped and covered boxes are skipped. Organize Pages thumbnail checkboxes retain page dragging; PDF form checkboxes retain their document behavior. Adapted from [upstream #4552](https://github.com/storytold/pdfcraft/pull/4552) by @Lcoding15.

Find and Advanced Search select the complete current query only on an explicit focus request, so reopening search replaces the old query while normal typing and cursor placement still work. Adapted from [upstream #4533](https://github.com/storytold/pdfcraft/pull/4533) by @urbanij.

Settings uses a left category rail and right settings pane, each with independent bounded scrolling. The title and green confirmation row stay in place as categories change. Existing settings apply immediately; OK closes. Category navigation never changes PDF contents. `preferences-category` selects a pane through UI automation; the language/spelling shortcut opens its category directly.

Personal Settings stays about 800 × 515 points with twelve compact navigation entries and solid scrollbars in both panes. Pane headings are centered; related controls share section spacing. OCR uses the same controls as Recognize Text, and Security reuses the timestamp selector without making requests. New-text style is session-only; page preview sizes apply to the current document. AI connections open the existing dedicated setup.
