//! Select additions, or all editable objects in Edit mode, and move them together.

use std::collections::BTreeSet;

use egui::{Color32, Key, Modifiers, Pos2, Rect, Stroke};
use pdfcraft_engine::{Document, Edit, EditableObject, ObjectKind, ObjectTarget};

use crate::{
    canvas::DocView,
    comments::PageCx,
    page_pointer::{Edge, PagePointer},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Annotation(usize),
    Added(usize),
    Object(ObjectTarget),
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
    pending_objects: Option<Vec<EditableObject>>,
    include_original: bool,
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

    pub fn prepare(&mut self, doc: &Document, enabled: bool, include_original: bool, edit_queued: bool, ptr: &PagePointer) {
        // Keyboard moves are queued before the canvas; keep their remapping until commit.
        if !edit_queued {
            self.pending_move = false;
            self.pending_objects = None;
        }
        if !enabled || self.generation != Some(doc.edit_generation()) || self.include_original != include_original {
            self.selected.clear();
            self.page = None;
            self.gesture = None;
        }
        self.include_original = include_original;
        self.generation = Some(doc.edit_generation());
        if !ptr.primary_down && !ptr.edges.iter().any(|e| matches!(e, Edge::Release(_))) {
            self.gesture = None;
        }
    }

    pub fn committed(&mut self, edit: &Edit, doc: &Document) {
        if self.pending_move && matches!(edit, Edit::Batch { label, .. } if label == "Move additions" || label == "Move objects") {
            if let Some(expected) = self.pending_objects.take() {
                let moved = self.page.and_then(|page| crate::object_ui::remap_objects(doc, page, &expected));
                if let Some(objects) = moved {
                    self.selected.retain(|target| !matches!(target, Target::Object(_)));
                    self.selected.extend(objects.into_iter().map(|o| Target::Object(o.target)));
                } else {
                    // Regrouped or ambiguous content must be selected again, never moved by a stale index.
                    self.selected.clear();
                }
            }
            self.generation = Some(doc.edit_generation());
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

    pub(crate) fn selected_page(&self) -> Option<usize> {
        self.page.filter(|_| !self.blocked && self.gesture.is_none() && !self.selected.is_empty())
    }

    pub(crate) fn move_by(&mut self, doc: &Document, offset: [f64; 2]) -> Result<Edit, String> {
        let page = self.page.ok_or("Select objects first")?;
        if self.generation != Some(doc.edit_generation()) {
            return Err("The document changed; select the objects again".into());
        }
        let annotations: Vec<_> = self.selected.iter().filter_map(|t| if let Target::Annotation(i) = t { Some(*i) } else { None }).collect();
        let content: Vec<_> = self.selected.iter().filter_map(|t| if let Target::Added(i) = t { Some(*i) } else { None }).collect();
        let objects: Vec<_> = self.selected.iter().filter_map(|t| if let Target::Object(o) = t { Some(*o) } else { None }).collect();
        let edit = doc.move_selection(page, &annotations, &content, &objects, offset)?;
        self.pending_objects = if objects.is_empty() {
            None
        } else {
            let user_offset = doc.object_move_offset(page, offset)?;
            let selected: Vec<_> = doc.editable_objects(page)?.into_iter().filter(|o| objects.contains(&o.target)).collect();
            Some(crate::object_ui::moved_objects(&selected, user_offset))
        };
        self.pending_move = true;
        Ok(edit)
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
        let objects: Vec<_> = self
            .selected
            .iter()
            .filter_map(|target| match target {
                Target::Added(index) => Some(ObjectTarget { kind: ObjectKind::Added, index: *index }),
                Target::Object(target) => Some(*target),
                _ => None,
            })
            .collect();
        let originals = self.selected.iter().any(|t| matches!(t, Target::Object(_)));
        if originals {
            edits.push(Edit::DeleteObjects { page, objects });
        } else {
            // Added-only deletion must not depend on parsing unrelated original artwork.
            edits.extend(objects.into_iter().rev().map(|o| Edit::DeleteContent { page, index: o.index }));
        }
        let label = if originals { "Delete objects" } else { "Delete additions" };
        Ok(Edit::Batch { label: label.into(), edits })
    }
}

fn items(pcx: &PageCx<'_>, doc: &Document, originals: &[EditableObject]) -> Vec<(Target, Rect)> {
    let mut out: Vec<_> = originals
        .iter()
        .filter(|o| o.target.kind != ObjectKind::Added)
        .map(|o| (Target::Object(o.target), pcx.xf.user_rect(pcx.info, pcx.page, o.rect.map(|v| v as f32))))
        .collect();
    out.extend(pcx.comments().flat_map(|a| pcx.screen_rects(a).map(move |r| (Target::Annotation(a.index), r))));
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
    let Some(pointer) = pcx.ptr.pos else { return true };
    let originals = if view.group.include_original {
        match view.objects.inventory(doc, pcx.page) {
            Ok(objects) => objects,
            Err(why) => {
                if pcx.ptr.pressed().is_some() {
                    view.group.notice = Some(why);
                }
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let items = items(pcx, doc, &originals);
    if pcx.ptr.pressed().is_some() && pcx.xf.rect.contains(pointer) && ui.clip_rect().contains(pointer) {
        if view.group.page != Some(pcx.page) {
            view.group.selected.clear();
        }
        view.group.page = Some(pcx.page);
        let additive = ui.input(|i| i.modifiers.shift || i.modifiers.command || i.modifiers.ctrl);
        let hit = items.iter().rev().find(|(_, r)| r.expand(3.0).contains(pointer)).map(|(t, _)| *t);
        let moving = !additive
            && (hit.is_some_and(|target| view.group.selected.contains(&target))
                || view.group.bounds(&items).is_some_and(|r| r.expand(6.0).contains(pointer)));
        view.group.gesture = Some(Gesture { start: pointer, now: pointer, additive, moving });
        view.comments.selected = None;
        view.content.selected = None;
        view.picked_image = None;
    }
    if view.group.page == Some(pcx.page)
        && let Some(g) = &mut view.group.gesture
    {
        g.now = pointer.clamp(pcx.xf.rect.min, pcx.xf.rect.max);
        if pcx.ptr.edges.iter().any(|e| matches!(e, Edge::Release(_))) {
            if g.moving && g.start.distance(g.now) >= 3.0 {
                let (a, b) = (pcx.xf.screen_to_view(g.start), pcx.xf.screen_to_view(g.now));
                let offset = [f64::from(b.0 - a.0), f64::from(b.1 - a.1)];
                match view.group.move_by(doc, offset) {
                    Ok(edit) => view.pending_edit = Some(edit),
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

pub(crate) fn paint(painter: &egui::Painter, pcx: &PageCx<'_>, doc: &Document, view: &DocView) {
    let group = &view.group;
    if group.page != Some(pcx.page) {
        return;
    }
    let originals = if group.include_original { view.objects.cached_objects(pcx.page) } else { &[] };
    let items = items(pcx, doc, originals);
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
