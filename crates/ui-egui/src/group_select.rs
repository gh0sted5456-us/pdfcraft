//! Select additions without catching the original page or a scanned background.

use std::collections::BTreeSet;

use egui::{Color32, Key, Modifiers, Pos2, Rect, Stroke};
use pdfcraft_engine::{Document, Edit};

use crate::{canvas::DocView, comments::PageCx};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Annotation(usize),
    Added(usize),
}

struct Gesture {
    start: Pos2,
    now: Pos2,
    additive: bool,
    moving: bool,
}

#[derive(Default)]
pub(crate) struct GroupSelection {
    page: Option<usize>,
    selected: BTreeSet<Target>,
    gesture: Option<Gesture>,
    generation: Option<u64>,
    blocked: bool,
    pending_move: bool,
    pub notice: Option<String>,
}

impl GroupSelection {
    pub fn count(&self) -> usize {
        self.selected.len()
    }

    pub fn set_input_blocked(&mut self, blocked: bool) {
        self.blocked = blocked;
        if blocked {
            self.gesture = None;
        }
    }

    pub fn prepare(&mut self, doc: &Document, enabled: bool, ctx: &egui::Context) {
        self.pending_move = false;
        if !enabled || self.generation != Some(doc.edit_generation()) {
            self.selected.clear();
            self.page = None;
            self.gesture = None;
        }
        self.generation = Some(doc.edit_generation());
        if !ctx.input(|i| i.pointer.primary_down() || i.pointer.primary_released()) {
            self.gesture = None;
        }
    }

    pub fn committed(&mut self, edit: &Edit, generation: u64) {
        if self.pending_move && matches!(edit, Edit::Batch { label, .. } if label == "Move additions") {
            // Moves replace existing streams/dictionaries in place, so selected indices stay valid.
            self.generation = Some(generation);
        }
        self.pending_move = false;
    }

    fn bounds(&self, items: &[(Target, Rect)]) -> Option<Rect> {
        items.iter().filter(|(t, _)| self.selected.contains(t)).map(|(_, r)| *r).reduce(Rect::union)
    }

    fn preview(&self, items: &[(Target, Rect)]) -> BTreeSet<Target> {
        let Some(g) = &self.gesture else { return self.selected.clone() };
        if g.moving {
            return self.selected.clone();
        }
        let mut selected = if g.additive { self.selected.clone() } else { BTreeSet::new() };
        if g.start.distance(g.now) >= 3.0 {
            let marquee = Rect::from_two_pos(g.start, g.now);
            selected.extend(items.iter().filter(|(_, rect)| marquee.intersects(*rect)).map(|(target, _)| *target));
        } else if let Some((target, _)) = items.iter().rev().find(|(_, rect)| rect.expand(3.0).contains(g.now))
            && !selected.remove(target)
        {
            selected.insert(*target);
        }
        selected
    }

    fn delete_edit(&self) -> Result<Edit, &'static str> {
        let page = self.page.ok_or("Select additions first")?;
        if self.selected.is_empty() || self.selected.len() > 1_000 {
            return Err("Select between 1 and 1000 additions to delete");
        }
        let indices: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|t| match t {
                Target::Annotation(i) => Some(*i),
                _ => None,
            })
            .collect();
        let mut edits = Vec::new();
        if !indices.is_empty() {
            edits.push(Edit::DeleteAnnotations { page, indices });
        }
        // Added-content indices are page-local; remove from the end to keep earlier ones stable.
        for target in self.selected.iter().rev() {
            if let Target::Added(index) = target {
                edits.push(Edit::DeleteContent { page, index: *index });
            }
        }
        Ok(Edit::Batch { label: "Delete additions".into(), edits })
    }
}

fn items(pcx: &PageCx<'_>, doc: &Document) -> Vec<(Target, Rect)> {
    let mut out: Vec<_> = pcx.comments().flat_map(|a| pcx.screen_rects(a).map(move |r| (Target::Annotation(a.index), r))).collect();
    out.extend(
        doc.added()
            .iter()
            .filter(|a| a.page == pcx.page)
            .enumerate()
            .map(|(index, a)| (Target::Added(index), crate::content_ui::screen_rect(pcx.xf, pcx.info, pcx.page, a.content.rect()))),
    );
    out
}

pub(crate) fn page_input(ui: &egui::Ui, pcx: &PageCx<'_>, doc: &Document, view: &mut DocView) -> bool {
    if view.group.blocked {
        return true;
    }
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else { return true };
    let items = items(pcx, doc);
    if ui.input(|i| i.pointer.primary_pressed()) && pcx.xf.rect.contains(pointer) && ui.clip_rect().contains(pointer) {
        if view.group.page != Some(pcx.page) {
            view.group.selected.clear();
        }
        view.group.page = Some(pcx.page);
        let additive = ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl);
        let hit = items.iter().rev().find(|(_, r)| r.expand(3.0).contains(pointer)).map(|(t, _)| *t);
        let moving = !additive && (hit.is_some() || view.group.bounds(&items).is_some_and(|r| r.expand(6.0).contains(pointer)));
        if moving
            && let Some(target) = hit
            && !view.group.selected.contains(&target)
        {
            view.group.selected.clear();
            view.group.selected.insert(target);
        }
        view.group.gesture = Some(Gesture { start: pointer, now: pointer, additive, moving });
        view.comments.selected = None;
        view.content.selected = None;
        view.picked_image = None;
    }
    if view.group.page == Some(pcx.page)
        && let Some(g) = &mut view.group.gesture
    {
        g.now = pointer.clamp(pcx.xf.rect.min, pcx.xf.rect.max);
        if ui.input(|i| i.pointer.primary_released()) {
            if g.moving && g.start.distance(g.now) >= 3.0 {
                let (a, b) = (pcx.xf.screen_to_view(g.start), pcx.xf.screen_to_view(g.now));
                let annotations: Vec<_> =
                    view.group.selected.iter().filter_map(|t| if let Target::Annotation(i) = t { Some(*i) } else { None }).collect();
                let content: Vec<_> = view.group.selected.iter().filter_map(|t| if let Target::Added(i) = t { Some(*i) } else { None }).collect();
                match doc.move_additions(pcx.page, &annotations, &content, [(b.0 - a.0) as f64, (b.1 - a.1) as f64]) {
                    Ok(edit) => {
                        view.pending_edit = Some(edit);
                        view.group.pending_move = true;
                    }
                    Err(why) => view.group.notice = Some(why),
                }
            } else {
                view.group.selected = view.group.preview(&items);
            }
            view.group.gesture = None;
        }
    }
    if pcx.xf.rect.contains(pointer) && ui.clip_rect().contains(pointer) {
        let moving = view.group.gesture.as_ref().is_some_and(|g| g.moving);
        let grab = view.group.page == Some(pcx.page) && view.group.bounds(&items).is_some_and(|r| r.expand(6.0).contains(pointer));
        ui.ctx().set_cursor_icon(if moving {
            egui::CursorIcon::Grabbing
        } else if grab {
            egui::CursorIcon::Grab
        } else {
            egui::CursorIcon::Crosshair
        });
    }
    true
}

pub(crate) fn paint(painter: &egui::Painter, pcx: &PageCx<'_>, doc: &Document, group: &GroupSelection) {
    if group.page != Some(pcx.page) {
        return;
    }
    let items = items(pcx, doc);
    let selected = group.preview(&items);
    let delta = group.gesture.as_ref().filter(|g| g.moving).map_or(egui::Vec2::ZERO, |g| g.now - g.start);
    let blue = Color32::from_rgb(65, 140, 250);
    for (target, rect) in &items {
        if selected.contains(target) {
            let rect = rect.translate(delta);
            painter.rect_filled(rect.expand(2.0), 2, blue.gamma_multiply(0.16));
            painter.rect_stroke(rect.expand(2.0), 2, Stroke::new(1.5, blue), egui::StrokeKind::Outside);
        }
    }
    if selected.len() > 1
        && let Some(rect) = group.bounds(&items)
    {
        painter.rect_stroke(rect.translate(delta).expand(5.0), 2, Stroke::new(1.0, blue), egui::StrokeKind::Outside);
    }
    if let Some(g) = &group.gesture
        && !g.moving
        && g.start.distance(g.now) >= 3.0
    {
        let rect = Rect::from_two_pos(g.start, g.now);
        painter.rect_filled(rect, 0, blue.gamma_multiply(0.2));
        painter.rect_stroke(rect, 0, Stroke::new(1.5, blue), egui::StrokeKind::Inside);
    }
}

pub(crate) fn keys(ctx: &egui::Context, view: &mut DocView) {
    if view.group.blocked || ctx.egui_wants_keyboard_input() {
        return;
    }
    if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
        view.group.gesture = None;
        view.group.selected.clear();
    } else if view.group.gesture.is_none()
        && view.group.count() > 0
        && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace))
    {
        match view.group.delete_edit() {
            Ok(edit) => view.pending_edit = Some(edit),
            Err(why) => view.group.notice = Some(why.into()),
        }
    }
}
