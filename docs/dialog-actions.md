# Dialog actions

In-app dialog actions share one matte pill style, a 28-point height, at least
80 points of width, consistent spacing and a visible keyboard-focus ring. Long
translated labels expand the button instead of being clipped.

- Back is neutral and returns to the previous step.
- Cancel is blue and leaves the operation without applying it.
- Confirm actions such as Save, Continue, OK, Timestamp and Sign are green.
- Destructive actions such as Exit without saving, Discard, Remove and Revert are red.

The theme supplies the idle fill and readable text; the action color appears on
hover, press or keyboard focus. Disabled actions retain a subdued label and cannot
be clicked. Mode switches keep their existing selection appearance.

Confirmation sits at the right, with Cancel immediately before it. Back remains
at the left of a multi-step dialog. The Save changes prompt keeps its approved
centered Exit / Cancel / Save row and its document-specific tooltips, using the
same button widget and dimensions. Native operating-system file pickers retain
their platform controls.
