//! Left tool panel (All tools + tool sub-panels, generated from the engine catalogue) and the
//! right-hand panels (Comments, Bookmarks, Pages, Fields, Layers, Attachments).

use egui::{Align, Align2, Color32, CornerRadius, Layout, Rect, Sense, Stroke, pos2, vec2};
use pdfcraft_engine::catalog::{self, Availability, TOOL_GROUPS, ToolGroup, ToolItem};
use pdfcraft_render::{DocInfo, FieldKind, OutlineItem};

use crate::theme::{self, Tokens};
use crate::{LeftPanel, PdfCraftApp, RightPanel, icons, widgets};

pub(super) mod all_tools_ui;
mod bookmarks;
pub(super) mod comments_panel;
mod edit;
pub(super) mod format_text_ui;
pub(super) mod organize_panel;
pub(super) mod page_selection_ui;
mod pages;
mod right;
pub(super) mod search_ui;
pub mod side_panels;
mod tools;

pub use bookmarks::BmAction;
pub(crate) use bookmarks::ellipsized_prefix;
pub use right::right_panel;
pub use tools::left_panel;
pub(crate) use tools::panel_header;

// ───────────────────────────────────────────────────────────────────────────── right panels

pub(crate) enum Nav {
    Page(usize),
    /// A bookmark's destination: its page and where on it.
    Dest(usize, pdfcraft_render::DestView),
    Flash(usize, [f32; 4]),
}

pub fn human_size(n: usize) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MB", n as f64 / (1u64 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KB", n as f64 / (1u64 << 10) as f64),
        n => byte_count(n),
    }
}

/// An exact size in bytes, with the count's own plural form.
pub fn byte_count(n: usize) -> String {
    crate::i18n::trn(crate::i18n::current(), n as u64, "{n} byte", "{n} bytes")
}

impl PdfCraftApp {
    /// Text alignment owns these keys before the document-wide shortcuts (Ctrl+L is full screen).
    pub(crate) fn text_alignment_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        use pdfcraft_engine::{AddedContent, Edit, TextAlign};
        let Some((i, id)) = self.active_ids() else { return };
        let view = &self.views[i];
        let selected = self.session.get(id).and_then(|doc| {
            let (page, index) = view.content.selected?;
            let item = doc.added().iter().filter(|a| a.page == page).nth(index)?;
            match &item.content {
                AddedContent::Text(text) => Some((page, index, text.clone())),
                _ => None,
            }
        });
        if view.line_editor.is_none() && view.content.draft.is_none() && selected.is_none() {
            return;
        }
        let focused = ctx.memory(|m| m.focused());
        let editing = view.line_editor.is_some() && focused == Some(egui::Id::unique("edit-text-line-input"))
            || view.content.draft.is_some() && focused == Some(crate::content_ui::editor_id(id));
        if ctx.text_edit_focused() && !editing || ctx.input(|input| input.modifiers.shift || input.modifiers.alt) {
            return;
        }
        let align = ctx.input_mut(|input| {
            [(Key::L, TextAlign::Left), (Key::E, TextAlign::Center), (Key::R, TextAlign::Right)]
                .into_iter()
                .find_map(|(key, align)| input.consume_key(Modifiers::COMMAND, key).then_some(align))
        });
        let Some(align) = align else { return };
        if let Some(mut editor) = self.views[i].line_editor.clone() {
            if editor.look.align != align {
                editor.look.align = align;
                self.apply_line_format(i, editor, Some(ctx));
            }
        } else if let Some(draft) = self.views[i].content.draft.as_mut() {
            draft.style.align = align;
            self.text_style.align = align;
        } else if let Some((page, index, mut text)) = selected
            && text.align != align
        {
            text.align = align;
            if self.apply_edit(Edit::UpdateContent { page, index, content: AddedContent::Text(text) }) {
                self.text_style.align = align;
            }
        }
    }
}
