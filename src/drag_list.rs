//! Compositor-driven drag-to-reorder controller and slot geometry math for account cards.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use nsh_core::prelude::*;

use crate::store;

const SLOT_DEFAULT_GAP: f32 = 8.0;
const SLOT_DRAG_OVERSHOOT: f32 = 10.0;
const SLOT_HYSTERESIS: f32 = 2.0;
const REORDER_ANIM_MS: f32 = 140.0;

#[derive(Debug, Clone, PartialEq)]
struct CardSlot {
    id: u32,
    y: f32,
    height: f32,
}

#[derive(Clone)]
struct CardMotion {
    offset_y: AnimatedValue,
    last_shift: f32,
}

#[derive(Debug, Clone)]
struct ActiveDrag {
    id: u32,
    start_page_y: f32,
    dy: f32,
    from_index: usize,
    to_index: usize,
    dragged_span: f32,
    gap: f32,
    slots: Vec<CardSlot>,
    min_dy: f32,
    max_dy: f32,
}

#[derive(Clone)]
pub(crate) struct DragList {
    pub(crate) dragging_id: Signal<Option<u32>>,
    layouts: Rc<RefCell<HashMap<u32, Bounds<Points>>>>,
    motions: Rc<RefCell<HashMap<u32, CardMotion>>>,
    active: Rc<RefCell<Option<ActiveDrag>>>,
}

impl DragList {
    pub(crate) fn new() -> Self {
        Self {
            dragging_id: signal(None),
            layouts: Rc::new(RefCell::new(HashMap::new())),
            motions: Rc::new(RefCell::new(HashMap::new())),
            active: Rc::new(RefCell::new(None)),
        }
    }

    pub(crate) fn register_motion(&self, id: u32, offset_y: AnimatedValue) {
        self.motions.borrow_mut().insert(
            id,
            CardMotion {
                offset_y,
                last_shift: 0.0,
            },
        );
    }

    pub(crate) fn unregister(&self, id: u32) {
        self.layouts.borrow_mut().remove(&id);
        self.motions.borrow_mut().remove(&id);
        let removed_active = self.active.borrow().as_ref().is_some_and(|d| d.id == id);
        if removed_active {
            self.cancel(id);
        }
    }

    pub(crate) fn record_layout(&self, id: u32, bounds: Bounds<Points>) {
        self.layouts.borrow_mut().insert(id, bounds);
    }

    pub(crate) fn begin(&self, id: u32, page_y: f32) {
        if store::account_count() < 2 {
            return;
        }
        let ids = store::account_ids();
        let layouts = self.layouts.borrow();
        let slots: Vec<CardSlot> = ids
            .iter()
            .filter_map(|&item_id| {
                layouts.get(&item_id).map(|b| CardSlot {
                    id: item_id,
                    y: b.origin.y.0,
                    height: b.size.height.0,
                })
            })
            .collect();
        drop(layouts);
        let Some(from_index) = slots.iter().position(|slot| slot.id == id) else {
            return;
        };
        if slots.len() < 2 {
            return;
        }
        let gap = infer_slot_gap(&slots);
        let dragged = &slots[from_index];
        let first_top = slots.first().map(|s| s.y).unwrap_or(dragged.y);
        let last_bottom = slots
            .last()
            .map(|s| s.y + s.height)
            .unwrap_or(dragged.y + dragged.height);
        let min_dy = (first_top - dragged.y - SLOT_DRAG_OVERSHOOT).min(0.0);
        let max_dy = (last_bottom - (dragged.y + dragged.height) + SLOT_DRAG_OVERSHOOT).max(0.0);

        for motion in self.motions.borrow_mut().values_mut() {
            motion.last_shift = 0.0;
            motion.offset_y.set(0.0);
        }

        *self.active.borrow_mut() = Some(ActiveDrag {
            id,
            start_page_y: page_y,
            dy: 0.0,
            from_index,
            to_index: from_index,
            dragged_span: dragged.height + gap,
            gap,
            slots,
            min_dy,
            max_dy,
        });
        self.dragging_id.set(Some(id));
    }

    pub(crate) fn move_to(&self, id: u32, page_y: f32) {
        let mut active_ref = self.active.borrow_mut();
        let Some(drag) = active_ref.as_mut() else {
            return;
        };
        if drag.id != id {
            return;
        }
        let dy = (page_y - drag.start_page_y).clamp(drag.min_dy, drag.max_dy);
        drag.dy = dy;

        if let Some(motion) = self.motions.borrow().get(&id) {
            motion.offset_y.set(dy);
        }

        let drag_center = drag.slots[drag.from_index].y
            + drag.slots[drag.from_index].height * 0.5
            + dy;
        let next_to = target_index_for_center(
            drag.from_index,
            drag.to_index,
            drag_center,
            &drag.slots,
            drag.gap,
        );
        if next_to == drag.to_index {
            return;
        }
        drag.to_index = next_to;

        let instant = untracked(reduce_motion);
        let mut motions = self.motions.borrow_mut();
        for (idx, slot) in drag.slots.iter().enumerate() {
            if slot.id == id {
                continue;
            }
            let target_shift =
                sibling_shift_for(idx, drag.from_index, next_to, drag.dragged_span);
            if let Some(motion) = motions.get_mut(&slot.id)
                && (motion.last_shift - target_shift).abs() > 0.5
            {
                motion.last_shift = target_shift;
                animate_offset(&motion.offset_y, target_shift, instant);
            }
        }
    }

    pub(crate) fn finish(&self, id: u32) {
        let taken = {
            let mut active = self.active.borrow_mut();
            if active.as_ref().is_some_and(|drag| drag.id == id) {
                active.take()
            } else {
                None
            }
        };
        let Some(drag) = taken else {
            return;
        };
        let instant = untracked(reduce_motion);
        let reordered = drag.to_index != drag.from_index;

        if reordered {
            let start_slot_y = drag.slots[drag.from_index].y;
            let final_slot_y =
                simulated_slot_y(drag.from_index, drag.to_index, &drag.slots, drag.gap);
            let residual = drag.dy - (final_slot_y - start_slot_y);

            let mut motions = self.motions.borrow_mut();
            for (slot_id, motion) in motions.iter_mut() {
                motion.last_shift = 0.0;
                if *slot_id == drag.id {
                    if !instant && residual.abs() > 0.5 {
                        motion.offset_y.set(residual);
                        animate_offset(&motion.offset_y, 0.0, false);
                    } else {
                        motion.offset_y.set(0.0);
                    }
                } else {
                    motion.offset_y.set(0.0);
                }
            }
            drop(motions);
            store::reorder_account(drag.id, drag.to_index);
        } else {
            let mut motions = self.motions.borrow_mut();
            for motion in motions.values_mut() {
                motion.last_shift = 0.0;
                animate_offset(&motion.offset_y, 0.0, instant);
            }
        }
        self.dragging_id.set(None);
    }

    pub(crate) fn cancel(&self, id: u32) {
        let taken = {
            let mut active = self.active.borrow_mut();
            if active.as_ref().is_some_and(|drag| drag.id == id) {
                active.take()
            } else {
                None
            }
        };
        if taken.is_none() {
            return;
        }
        let instant = untracked(reduce_motion);
        let mut motions = self.motions.borrow_mut();
        for motion in motions.values_mut() {
            motion.last_shift = 0.0;
            animate_offset(&motion.offset_y, 0.0, instant);
        }
        drop(motions);
        self.dragging_id.set(None);
    }
}

fn animate_offset(value: &AnimatedValue, to: f32, instant: bool) {
    if instant {
        value.set(to);
    } else {
        value.timing(to, REORDER_ANIM_MS, Easing::EaseOut);
    }
}

fn infer_slot_gap(slots: &[CardSlot]) -> f32 {
    slots
        .windows(2)
        .next()
        .map(|pair| (pair[1].y - (pair[0].y + pair[0].height)).max(0.0))
        .unwrap_or(SLOT_DEFAULT_GAP)
}

fn simulated_slot_y(from_index: usize, to_index: usize, slots: &[CardSlot], gap: f32) -> f32 {
    let mut cursor_y = slots.first().map(|s| s.y).unwrap_or(0.0);
    let mut placed = 0usize;
    for (idx, slot) in slots.iter().enumerate() {
        if idx == from_index {
            continue;
        }
        if placed == to_index {
            return cursor_y;
        }
        cursor_y += slot.height + gap;
        placed += 1;
    }
    cursor_y
}

fn target_index_for_center(
    from_index: usize,
    current_to: usize,
    drag_center: f32,
    slots: &[CardSlot],
    gap: f32,
) -> usize {
    if slots.is_empty() {
        return 0;
    }
    let dragged_half = slots[from_index].height * 0.5;
    let center_at = |k: usize| simulated_slot_y(from_index, k, slots, gap) + dragged_half;
    let mut best_index = current_to.min(slots.len() - 1);
    let mut best_dist = (drag_center - center_at(best_index)).abs();
    let current_dist = best_dist;

    for k in 0..slots.len() {
        let dist = (drag_center - center_at(k)).abs();
        if dist < best_dist {
            best_dist = dist;
            best_index = k;
        }
    }

    if best_index != current_to && best_dist + SLOT_HYSTERESIS < current_dist {
        best_index
    } else {
        current_to
    }
}

fn sibling_shift_for(index: usize, from_index: usize, to_index: usize, dragged_span: f32) -> f32 {
    if from_index < index && index <= to_index {
        -dragged_span
    } else if to_index <= index && index < from_index {
        dragged_span
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_slots() -> Vec<CardSlot> {
        vec![
            CardSlot { id: 1, y: 8.0, height: 180.0 },
            CardSlot { id: 2, y: 200.0, height: 120.0 },
            CardSlot { id: 3, y: 332.0, height: 120.0 },
        ]
    }

    #[test]
    fn infers_gap_and_simulates_variable_height_slots() {
        let slots = sample_slots();
        let gap = infer_slot_gap(&slots);
        assert_eq!(gap, 12.0);
        assert_eq!(simulated_slot_y(0, 0, &slots, gap), 8.0);
        assert_eq!(simulated_slot_y(0, 1, &slots, gap), 140.0);
        assert_eq!(simulated_slot_y(0, 2, &slots, gap), 272.0);
        assert_eq!(simulated_slot_y(2, 0, &slots, gap), 8.0);
        assert_eq!(simulated_slot_y(2, 1, &slots, gap), 200.0);
    }

    #[test]
    fn resolves_target_index_and_sibling_shifts() {
        let slots = sample_slots();
        let gap = 12.0;
        assert_eq!(target_index_for_center(0, 0, 100.0, &slots, gap), 0);
        assert_eq!(target_index_for_center(0, 0, 180.0, &slots, gap), 1);
        assert_eq!(target_index_for_center(0, 1, 310.0, &slots, gap), 2);

        let span = 180.0 + gap;
        assert_eq!(sibling_shift_for(1, 0, 2, span), -span);
        assert_eq!(sibling_shift_for(2, 0, 2, span), -span);

        let short_span = 120.0 + gap;
        assert_eq!(sibling_shift_for(0, 2, 0, short_span), short_span);
        assert_eq!(sibling_shift_for(1, 2, 0, short_span), short_span);
    }
}
