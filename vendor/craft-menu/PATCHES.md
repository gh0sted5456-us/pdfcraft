# PdfCraft manifest compatibility patch

Source: https://github.com/storytold/craft-libs/tree/edc41f68fa24c46651c3b5117d10f0ead75ec9cb/crates/menu
License: MIT OR Apache-2.0; license texts retained. Rust sources are unchanged.

The upstream manifest disables inherited craft-keymap defaults while its workspace
dependency enables them implicitly. Cargo 1.95 rejects that declaration. This copy
spells out the same pinned dependencies so that disabling defaults is valid.
Remove this copy when a corrected shared-library revision is published and qualified.
