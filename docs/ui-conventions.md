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
