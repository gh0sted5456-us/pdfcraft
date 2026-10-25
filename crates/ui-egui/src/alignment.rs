//! Screen-space object guides: a fixed pointer reach at every PDF zoom.

use egui::{Pos2, Rect, Vec2};

const REACH: f32 = 6.0;
const MAX_TARGETS: usize = 10_000;

#[derive(Clone, Copy, Default)]
pub(crate) struct Snap {
    pub delta: Vec2,
    pub guides: [Option<[Pos2; 2]>; 2],
}

/// Align either edge or centre independently on each axis. References are already filtered
/// to exclude the moving selection; a page's editable inventory is capped at MAX_TARGETS.
pub(crate) fn snap(moving: Rect, delta: Vec2, targets: impl Iterator<Item = Rect>) -> Snap {
    if !delta.is_finite() {
        return Snap::default();
    }
    let mut result = Snap { delta, ..Default::default() };
    if !moving.is_finite() || !moving.is_positive() {
        return result;
    }
    let proposed = moving.translate(delta);
    let mut best = [REACH + 0.01; 2];
    let mut chosen: [Option<(f32, Rect)>; 2] = [None; 2];
    let anchors = |r: Rect| [[r.left(), r.center().x, r.right()], [r.top(), r.center().y, r.bottom()]];
    for target in targets.take(MAX_TARGETS).filter(|r| r.is_finite() && r.is_positive()) {
        for (axis, (source, dest)) in anchors(proposed).into_iter().zip(anchors(target)).enumerate() {
            for a in source {
                for b in dest {
                    let distance = (b - a).abs();
                    let Some(reach) = best.get_mut(axis) else { continue };
                    if distance <= REACH && distance < *reach {
                        *reach = distance;
                        if axis == 0 {
                            result.delta.x = delta.x + b - a;
                        } else {
                            result.delta.y = delta.y + b - a;
                        }
                        if let Some(slot) = chosen.get_mut(axis) {
                            *slot = Some((b, target));
                        }
                    }
                }
            }
        }
    }
    let placed = moving.translate(result.delta);
    for (axis, selection) in chosen.into_iter().enumerate() {
        if let Some((at, target)) = selection {
            let points = if axis == 0 {
                [Pos2::new(at, placed.top().min(target.top()) - 8.0), Pos2::new(at, placed.bottom().max(target.bottom()) + 8.0)]
            } else {
                [Pos2::new(placed.left().min(target.left()) - 8.0, at), Pos2::new(placed.right().max(target.right()) + 8.0, at)]
            };
            if let Some(slot) = result.guides.get_mut(axis) {
                *slot = Some(points);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    fn rect(x: f32, y: f32) -> Rect {
        Rect::from_min_size(pos2(x, y), vec2(20.0, 20.0))
    }

    #[test]
    fn edges_and_centres_snap_with_visible_guides_on_both_axes() {
        let a = rect(10.0, 10.0);
        let r = snap(a, vec2(87.0, 86.0), [rect(100.0, 100.0)].into_iter());
        assert_eq!(r.delta, vec2(90.0, 90.0));
        assert!(r.guides.iter().all(Option::is_some));
        let r = snap(a, vec2(48.0, 1.0), [Rect::from_min_size(pos2(60.0, 100.0), vec2(40.0, 20.0))].into_iter());
        assert_eq!(r.delta.x, 50.0);
        assert!(r.guides[0].is_some());
    }

    #[test]
    fn nearest_alignment_wins_and_distant_targets_do_not_pull() {
        let r = snap(rect(0.0, 0.0), vec2(95.0, 200.0), [rect(100.0, 0.0), rect(96.0, 0.0)].into_iter());
        assert_eq!(r.delta, vec2(96.0, 200.0));
        let r = snap(rect(0.0, 0.0), vec2(43.0, 43.0), [rect(100.0, 100.0)].into_iter());
        assert_eq!(r.delta, vec2(43.0, 43.0));
        assert!(r.guides.iter().all(Option::is_none));
    }

    #[test]
    fn invalid_geometry_and_empty_targets_are_safe() {
        let r = snap(rect(0.0, 0.0), vec2(3.0, 2.0), [Rect::NOTHING].into_iter());
        assert_eq!(r.delta, vec2(3.0, 2.0));
        assert!(r.guides.iter().all(Option::is_none));
        assert_eq!(snap(rect(0.0, 0.0), vec2(f32::NAN, 0.0), std::iter::empty()).delta, Vec2::ZERO);
    }
}
