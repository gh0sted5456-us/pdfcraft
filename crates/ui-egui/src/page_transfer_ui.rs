//! A page drag survives switching tabs; releasing outside a page target cancels it.

use crate::PdfCraftApp;
use pdfcraft_engine::DocId;

#[derive(Clone)]
struct Drag {
    source: DocId,
    generation: u64,
    pages: Vec<usize>,
    hovered: Option<(DocId, f64)>,
}

fn key() -> egui::Id {
    egui::Id::unique("cross-document-pages")
}

pub(crate) fn incoming(ctx: &egui::Context, target: DocId) -> bool {
    ctx.data(|d| d.get_temp::<Drag>(key())).is_some_and(|d| d.source != target)
}

/// Hover a destination tab for a moment to open its page grid without releasing the drag.
pub(crate) fn hover_tab(ctx: &egui::Context, target: DocId, rect: egui::Rect) -> bool {
    let Some(mut drag) = ctx.data(|d| d.get_temp::<Drag>(key())) else { return false };
    if target == drag.source || !ctx.input(|i| i.pointer.primary_down() && i.pointer.hover_pos().is_some_and(|p| rect.contains(p))) {
        return false;
    }
    let now = ctx.input(|i| i.time);
    let since = match drag.hovered {
        Some((id, since)) if id == target => since,
        _ => now,
    };
    drag.hovered = Some((target, since));
    ctx.data_mut(|d| d.insert_temp(key(), drag));
    ctx.request_repaint_after(std::time::Duration::from_millis(30));
    now - since >= 0.35
}

impl PdfCraftApp {
    pub(crate) fn finish_page_transfer(&mut self, ctx: &egui::Context) {
        let drag = ctx.data(|d| d.get_temp::<Drag>(key()));
        let down = ctx.input(|i| i.pointer.primary_down());
        if drag.is_none() && down && self.dialog.is_none() {
            if let Some(view) = self.active.and_then(|i| self.views.get(i))
                && let Some(pages) = view.org_drag.as_ref().or(view.panel_drag.as_ref())
                && let Some(doc) = self.session.get(view.id)
            {
                ctx.data_mut(|d| {
                    d.insert_temp(key(), Drag { source: view.id, generation: doc.edit_generation(), pages: pages.clone(), hovered: None })
                });
            }
            return;
        }
        let Some(drag) = drag else { return };
        let cancelled = ctx.input(|i| i.key_pressed(egui::Key::Escape)) || self.dialog.is_some();
        if cancelled || !down {
            ctx.data_mut(|d| d.remove::<Drag>(key()));
            for view in &mut self.views {
                view.org_drag = None;
                view.panel_drag = None;
            }
            if !ctx.input(|i| i.pointer.primary_released()) || cancelled {
                return;
            }
            let Some((index, target)) = self.active_ids().filter(|(_, id)| *id != drag.source) else { return };
            let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) else { return };
            let Some(view) = self.views.get(index) else { return };
            let gap = if view.organize && view.viewport_rect().contains(pointer) {
                view.grid_gap
            } else if self.right == Some(crate::RightPanel::Pages) && view.pages_panel.is_some_and(|r| r.contains(pointer)) {
                view.panel_gap
            } else {
                None
            };
            let Some(gap) = gap else { return };
            if !self.session.get(drag.source).is_some_and(|doc| doc.edit_generation() == drag.generation) {
                self.notify("The source document changed during the drag. Select the pages again.");
                return;
            }
            self.transfer_open_pages(drag.source, target, &drag.pages, gap, ctx.input(|i| i.modifiers.shift));
        }
    }

    /// Also used by Organize's explicit insertion from another open PDF.
    pub fn transfer_open_pages(&mut self, source: DocId, target: DocId, pages: &[usize], at: usize, moving: bool) -> bool {
        let previous = self.active;
        // Include form text still being typed in either document. A rejected value stays focused.
        for id in [source, target] {
            if let Some(index) = self.views.iter().position(|v| v.id == id) {
                self.active = Some(index);
                if !self.commit_form_typing() {
                    return false;
                }
            }
        }
        self.active = previous;
        let pages: Vec<usize> = pages.iter().copied().collect::<std::collections::BTreeSet<_>>().into_iter().collect();
        match self.session.transfer_pages(source, target, &pages, at, moving) {
            Ok(warning) => {
                let removed = moving && warning.is_none();
                for view in &mut self.views {
                    if (view.id == target || (removed && view.id == source))
                        && let Some(doc) = self.session.get(view.id)
                    {
                        view.document_changed(&doc.info);
                    }
                    if view.id == target {
                        view.select_pages(&(at..at.saturating_add(pages.len())).collect::<Vec<_>>());
                    }
                }
                self.notify(warning.unwrap_or_else(|| {
                    if moving { "Pages moved. Each document has its own Undo.".into() } else { "Pages copied. The source document was kept.".into() }
                }));
                true
            }
            Err(error) => {
                self.notify(format!("Couldn't transfer pages: {error}"));
                false
            }
        }
    }
}
