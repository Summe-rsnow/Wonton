//! The one screen: a header, the proxy the app hands over, and the accounts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use nsh_base::{dialog, theme};
use nsh_core::Cursor;
use nsh_core::element::desktop::{
    MenuItem, WindowRef, current_window, quit, set_tray, title_bar, tray,
};
use nsh_core::time::unix_ms;
use nsh_core::prelude::*;

use crate::format;
use crate::google::{AccountTier, ClientKind};
use crate::launch::{LaunchTarget, ProxyConfig};
use crate::store::{self, CloseBehavior, ThemeMode, Tone};
use crate::ui::{
    action, card, checkbox, divider, empty_state, hint_tooltip, icon_action, icon_action_disabled,
    icon_action_with_tip, icons, notice_toast, progress_bar, quota_row, radio_option,
    segmented_icon, status_badge, text_field, tier_badge, toggle_switch, ButtonKind, IconKind,
    ValueTone,
};

#[component]
pub(crate) fn Screen() -> impl IntoElement {
    let dialog_open = signal(false);
    let pending_delete: Signal<Option<u32>> = signal(None);
    let close_prompt_open = signal(false);
    let saved = store::proxy();
    let host = signal(saved.host.clone());
    let port = signal(saved.port.to_string());
    // The clock the "3 分钟前" and reset labels read, so they age without a
    // refresh; one tick a minute is enough for minute-grained text.
    let clock = signal(unix_ms() as i64);
    set_interval(Duration::from_secs(60), move || clock.set(unix_ms() as i64));
    effect(move || {
        if let Some(notice) = store::notice() {
            let seq = store::notice_seq();
            let ttl = match notice.tone {
                Tone::Bad => Duration::from_secs(6),
                _ => Duration::from_secs(3),
            };
            let task = set_timeout(ttl, move || store::clear_notice_if_seq(seq));
            cleanup(move || task.cancel());
        }
    });

    let main_window = current_window();
    {
        let win = main_window.clone();
        main_window.on_close_press(move |_reason| match store::close_behavior() {
            CloseBehavior::Ask => {
                win.focus();
                close_prompt_open.set(true);
            }
            CloseBehavior::Tray => win.hide(),
            CloseBehavior::Exit => quit(),
        });
    }
    {
        let win_click = main_window.clone();
        let win_menu = main_window.clone();
        effect(move || {
            let behavior = store::close_behavior();
            let count = store::account_count();
            let busy = store::busy();
            let tooltip = if count > 0 {
                format!("Wonton · {count} 个账号")
            } else {
                "Wonton".to_owned()
            };
            let win_on_click = win_click.clone();
            let win_on_show = win_menu.clone();
            set_tray(
                tray()
                    .icon(icons::tray_icon_src())
                    .tooltip(tooltip)
                    .on_click(move |_| win_on_click.focus())
                    .menu([
                        MenuItem::new("显示主窗口").on_select(move || win_on_show.focus()),
                        MenuItem::new("刷新全部账号")
                            .disabled(busy)
                            .on_select(store::refresh_all),
                        MenuItem::separator(),
                        MenuItem::submenu(
                            "点击关闭按钮时",
                            [
                                MenuItem::new("每次询问")
                                    .checked(behavior == CloseBehavior::Ask)
                                    .on_select(|| store::set_close_behavior(CloseBehavior::Ask)),
                                MenuItem::new("最小化到托盘")
                                    .checked(behavior == CloseBehavior::Tray)
                                    .on_select(|| store::set_close_behavior(CloseBehavior::Tray)),
                                MenuItem::new("直接退出应用")
                                    .checked(behavior == CloseBehavior::Exit)
                                    .on_select(|| store::set_close_behavior(CloseBehavior::Exit)),
                            ],
                        ),
                        MenuItem::separator(),
                        MenuItem::new("退出 Wonton").on_select(quit),
                    ]),
            );
        });
    }
    cleanup(|| set_tray(None));

    let prompt_window = main_window.clone();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(header(dialog_open))
        .child(proxy_bar(host, port))
        .child(accounts(clock, pending_delete))
        .child(when(
            move || {
                !dialog_open.get()
                    && pending_delete.get().is_none()
                    && !close_prompt_open.get()
                    && store::notice().is_some()
            },
            notice_overlay,
        ))
        .child(when(dialog_open, move || {
            crate::add_dialog::add_dialog().on_close(move || dialog_open.set(false))
        }))
        .child(when(
            move || pending_delete.get().is_some(),
            move || confirm_delete_dialog(pending_delete),
        ))
        .child(when(close_prompt_open, move || {
            confirm_close_dialog(close_prompt_open, prompt_window.clone())
        }))
        .child(when(move || store::import_progress().is_some(), import_progress_dialog))
}

fn header(dialog_open: Signal<bool>) -> impl IntoElement {
    view()
        .window_drag(true)
        .flex_row()
        .items_center()
        .gap_3()
        .h(move || px(title_bar().top.0.max(48.0)))
        .pl(move || px(title_bar().left.0 + 16.0))
        .pr(move || px(title_bar().right.0 + 16.0))
        .border_b_1()
        .border_color(move || theme().border)
        .child(
            view()
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .gap_2()
                .child(icons::brand_mark(20.0))
                .child(
                    text("Wonton")
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().foreground),
                ),
        )
        .child(
            text("Antigravity 账号与额度")
                .min_w(px(0.0))
                .flex_shrink(1.0)
                .text_xs()
                .number_of_lines(1)
                .text_color(move || theme().muted_foreground),
        )
        .child(view().flex_1())
        .child(
            view()
                .window_drag(false)
                .flex_shrink_0()
                .flex_row()
                .items_center()
                .gap(px(6.0))
                .child(theme_switcher())
                .child(
                    view()
                        .w(px(1.0))
                        .h(px(16.0))
                        .mx(px(2.0))
                        .bg(move || theme().border),
                )
                .child(icon_action_with_tip(
                    IconKind::Secondary,
                    icons::UPLOAD,
                    "导入",
                    move || {
                        if store::busy() {
                            "正在导入…"
                        } else {
                            "导入账号 JSON"
                        }
                    },
                    store::busy,
                    store::import_accounts_action,
                ))
                .child(icon_action_with_tip(
                    IconKind::Secondary,
                    icons::DOWNLOAD,
                    "导出",
                    move || {
                        let count = store::selected_count();
                        if count > 0 {
                            format!("导出已选 {count} 个账号")
                        } else {
                            "导出全部账号".to_owned()
                        }
                    },
                    || false,
                    move || store::export_accounts_action(None),
                ))
                .child(icon_action_with_tip(
                    IconKind::Secondary,
                    icons::REFRESH,
                    "全部刷新",
                    move || {
                        if store::busy() {
                            "正在刷新…"
                        } else {
                            "刷新全部账号"
                        }
                    },
                    store::busy,
                    store::refresh_all,
                ))
                .child(icon_action(
                    IconKind::Primary,
                    icons::PLUS,
                    "添加账号…",
                    move || {
                        store::clear_notice();
                        dialog_open.set(true)
                    },
                )),
        )
}

fn theme_switcher() -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .p(px(2.0))
        .rounded(crate::ui::radius())
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().muted)
        .child(segmented_icon(
            move || store::theme_mode() == ThemeMode::System,
            icons::MONITOR,
            ThemeMode::System.label(),
            move || store::set_theme_mode(ThemeMode::System),
        ))
        .child(segmented_icon(
            move || store::theme_mode() == ThemeMode::Light,
            icons::SUN,
            ThemeMode::Light.label(),
            move || store::set_theme_mode(ThemeMode::Light),
        ))
        .child(segmented_icon(
            move || store::theme_mode() == ThemeMode::Dark,
            icons::MOON,
            ThemeMode::Dark.label(),
            move || store::set_theme_mode(ThemeMode::Dark),
        ))
}

fn proxy_bar(host: Signal<String>, port: Signal<String>) -> impl IntoElement {
    let build_config = move || ProxyConfig {
        enabled: store::proxy().enabled,
        protocol: "http".to_owned(),
        host: host.get_untracked().trim().to_owned(),
        port: port.get_untracked().trim().parse::<u16>().unwrap_or(0),
    };
    let save = move || {
        let config = build_config();
        match config.validate() {
            Ok(()) => {
                store::set_proxy(config);
                store::set_notice("代理已保存", Tone::Ok);
            }
            Err(error) => store::set_notice(error, Tone::Bad),
        }
    };
    let test_proxy = move || store::test_proxy_action(build_config());
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .flex_wrap()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(move || theme().border)
        .child(toggle_switch(
            move || store::proxy().enabled,
            "启用 HTTP 代理",
            Some("HTTP 代理"),
            store::set_proxy_enabled,
        ))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .opacity(move || if store::proxy().enabled { 1.0_f32 } else { 0.45_f32 })
                .child(
                    text_field(host, "127.0.0.1", move |text: &str| host.set(text.to_owned()))
                        .label("主机地址")
                        .w(px(156.0)),
                )
                .child(
                    text(":")
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text_field(port, "7890", move |text: &str| port.set(text.to_owned()))
                        .label("端口")
                        .w(px(74.0)),
                ),
        )
        .child(view().flex_1())
        .child(icon_action_with_tip(
            IconKind::Secondary,
            icons::ACTIVITY,
            "测试",
            move || {
                if store::proxy_testing() {
                    "正在测试代理…"
                } else {
                    "测试代理连通性"
                }
            },
            store::proxy_testing,
            test_proxy,
        ))
        .child(icon_action_with_tip(
            IconKind::Secondary,
            icons::SAVE,
            "保存",
            "保存代理设置",
            || false,
            save,
        ))
        .child(icon_action_with_tip(
            IconKind::Primary,
            icons::ROCKET,
            "启动反重力",
            "启动 Antigravity",
            || false,
            store::launch_app,
        ))
}

fn notice_overlay() -> impl IntoElement {
    view()
        .absolute()
        .bottom(px(16.0))
        .left(px(16.0))
        .right(px(16.0))
        .items_center()
        .z_index(10)
        .pointer_events(PointerEvents::BoxNone)
        .child(notice_toast(
            move || store::notice().map(|notice| notice.text).unwrap_or_default(),
            move || match store::notice().map(|notice| notice.tone) {
                Some(Tone::Bad) => ValueTone::Bad.color(),
                Some(Tone::Ok) => ValueTone::Ok.color(),
                _ => theme().muted_foreground,
            },
            store::clear_notice,
        ))
}

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
struct DragList {
    dragging_id: Signal<Option<u32>>,
    layouts: Rc<RefCell<HashMap<u32, Bounds<Points>>>>,
    motions: Rc<RefCell<HashMap<u32, CardMotion>>>,
    active: Rc<RefCell<Option<ActiveDrag>>>,
}

impl DragList {
    fn new() -> Self {
        Self {
            dragging_id: signal(None),
            layouts: Rc::new(RefCell::new(HashMap::new())),
            motions: Rc::new(RefCell::new(HashMap::new())),
            active: Rc::new(RefCell::new(None)),
        }
    }

    fn register_motion(&self, id: u32, offset_y: AnimatedValue) {
        self.motions.borrow_mut().insert(
            id,
            CardMotion {
                offset_y,
                last_shift: 0.0,
            },
        );
    }

    fn unregister(&self, id: u32) {
        self.layouts.borrow_mut().remove(&id);
        self.motions.borrow_mut().remove(&id);
        let removed_active = self.active.borrow().as_ref().is_some_and(|d| d.id == id);
        if removed_active {
            self.cancel(id);
        }
    }

    fn record_layout(&self, id: u32, bounds: Bounds<Points>) {
        self.layouts.borrow_mut().insert(id, bounds);
    }

    fn begin(&self, id: u32, page_y: f32) {
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

    fn move_to(&self, id: u32, page_y: f32) {
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

    fn finish(&self, id: u32) {
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

    fn cancel(&self, id: u32) {
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

fn accounts(clock: Signal<i64>, pending_delete: Signal<Option<u32>>) -> impl IntoElement {
    let drag = DragList::new();

    // No accounts: the empty state owns the whole region and centers in it.
    // With accounts, a fixed-height selection bar sits above the scrollable list.
    view()
        .flex_1()
        .child(when(
            move || store::account_count() == 0,
            || {
                view().flex_1().child(empty_state(
                    "还没有账号",
                    "点击右上角“+”添加账号，或使用导入图标读取 JSON 文件。",
                ))
            },
        ))
        .child(when(move || store::account_count() > 0, move || {
            let drag = drag.clone();
            view()
                .flex_1()
                .child(selection_bar())
                .child(scroll().flex_1().child(
                    view()
                        .px_4()
                        .pt_1()
                        .pb_safe_area_plus(px(12.0))
                        .gap_2()
                        .child(each(
                            store::account_ids,
                            |id: &u32| *id,
                            move |id, _index| {
                                account_card(id.get_untracked(), clock, drag.clone(), pending_delete)
                            },
                        )),
                ))
        }))
}

fn selection_bar() -> impl IntoElement {
    view()
        .h(px(34.0))
        .flex_row()
        .items_center()
        .gap_3()
        .px_4()
        .child(checkbox(
            store::all_selected,
            store::some_selected,
            "全选账号",
            Some("全选"),
            |_| store::toggle_select_all(),
        ))
        .child(
            text(move || {
                let selected = store::selected_count();
                let total = store::account_count();
                if selected > 0 {
                    format!("已选 {selected}/{total} 个（导出仅含所选）")
                } else if total >= 2 {
                    format!("共 {total} 个 · 拖拽排序")
                } else {
                    format!("共 {total} 个")
                }
            })
            .text_xs()
            .tabular_nums()
            .text_color(move || theme().muted_foreground),
        )
        .child(view().flex_1())
        .child(when(move || store::selected_count() > 0, || {
            icon_action(IconKind::Subtle, icons::CLOSE, "取消选择", store::clear_selection)
        }))
}

/// One account's card. Every value is bound to the store, so a refresh updates
/// the same card in place. Supports smooth compositor-driven drag-to-reorder.
fn account_card(
    id: u32,
    clock: Signal<i64>,
    drag: DragList,
    pending_delete: Signal<Option<u32>>,
) -> impl IntoElement {
    let offset_y = animated_value(0.0);
    drag.register_motion(id, offset_y);
    {
        let drag = drag.clone();
        cleanup(move || drag.unregister(id));
    }

    let now_ms = move || clock.get();
    let account = computed(move || store::account_by_id(id));
    let email = move || account.get().map(|account| account.email).unwrap_or_default();
    let select_label = move || {
        SharedString::from(format!(
            "选择 {}",
            account.get().map(|account| account.email).unwrap_or_default()
        ))
    };
    let kind = move || {
        match account.get().map(|account| account.kind) {
            Some(ClientKind::Enterprise) => "企业版",
            _ => "个人版",
        }
    };
    let age = move || {
        account
            .get()
            .map(|account| format::age_label(account.fetched_at, now_ms()))
            .unwrap_or_default()
    };
    let models = move || {
        account
            .get()
            .map(|account| crate::google::summarize_quotas(&account.models))
            .unwrap_or_default()
    };
    let error = move || account.get().and_then(|account| account.error);

    let dragging_id = drag.dragging_id;
    let is_dragging = move || dragging_id.get() == Some(id);
    let grip_hovered = signal(false);

    let drag_layout = drag.clone();
    let drag_card_grant = drag.clone();
    let drag_card_move = drag.clone();
    let drag_card_release = drag.clone();
    let drag_card_terminate = drag.clone();
    let drag_grip_grant = drag.clone();
    let drag_grip_move = drag.clone();
    let drag_grip_release = drag.clone();
    let drag_grip_terminate = drag.clone();

    let grip = view()
        .w(px(22.0))
        .h(px(24.0))
        .rounded(px(6.0))
        .items_center()
        .justify_center()
        .cursor(Cursor::Grab)
        .style_when(move || grip_hovered.get() && !is_dragging(), |s| {
            s.bg(move || theme().muted)
        })
        .style_when(is_dragging, |s| {
            s.cursor(Cursor::Grabbing).bg(move || theme().muted)
        })
        .accessible(true)
        .label("拖动排序")
        .tooltip(hint_tooltip("拖拽调整顺序"))
        .on_mouse_enter(move |_: &MouseEvent| grip_hovered.set(true))
        .on_mouse_leave(move |_: &MouseEvent| grip_hovered.set(false))
        .claim_touches(TouchClaim::on_start().ask(|request: &ClaimRequest| {
            if store::account_count() >= 2 {
                request.claim();
            }
        }))
        .keep_touches(true)
        .on_responder_grant(move |event: &ResponderEvent| {
            drag_grip_grant.begin(id, event.page.y.0);
        })
        .on_responder_move(move |event: &ResponderEvent| {
            drag_grip_move.move_to(id, event.page.y.0);
        })
        .on_responder_release(move |_: &ResponderEvent| {
            drag_grip_release.finish(id);
        })
        .on_responder_terminate(move |_: &ResponderEvent| {
            drag_grip_terminate.cancel(id);
        })
        .child(icons::svg_icon(icons::GRIP, 14.0, move || {
            if is_dragging() || grip_hovered.get() {
                theme().foreground
            } else {
                theme().muted_foreground
            }
        }));

    card()
        .bind_translate_y(&offset_y)
        .z_index(move || if is_dragging() { 20 } else { 0 })
        .style_when(is_dragging, |s| {
            s.cursor(Cursor::Grabbing)
                .border_color(move || theme().primary)
                .shadow_lg()
        })
        .on_layout(move |bounds: &Bounds<Points>| drag_layout.record_layout(id, *bounds))
        .claim_touches(
            TouchClaim::on_pan(PanActivation {
                active_offset_y: Some(px(8.0)),
                active_offset_x: None,
                fail_offset_x: None,
                fail_offset_y: None,
            })
            .ask(|request: &ClaimRequest| {
                if store::account_count() >= 2 {
                    request.claim();
                }
            }),
        )
        .keep_touches(true)
        .on_responder_grant(move |event: &ResponderEvent| {
            drag_card_grant.begin(id, event.page.y.0);
        })
        .on_responder_move(move |event: &ResponderEvent| {
            drag_card_move.move_to(id, event.page.y.0);
        })
        .on_responder_release(move |_: &ResponderEvent| {
            drag_card_release.finish(id);
        })
        .on_responder_terminate(move |_: &ResponderEvent| {
            drag_card_terminate.cancel(id);
        })
        .child(
            view()
                .h(px(28.0))
                .flex_row()
                .items_center()
                .gap_2()
                .child(grip)
                .child(checkbox(
                    move || store::is_selected(id),
                    || false,
                    select_label,
                    None,
                    move |_| store::toggle_select(id),
                ))
                .child(
                    text(email)
                        .min_w(px(60.0))
                        .flex_shrink(1.0)
                        .number_of_lines(1)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().foreground),
                )
                .child(
                    text(kind)
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(when(
                    move || account.get().and_then(|account| account.tier).is_some(),
                    move || {
                        tier_badge(move || {
                            account
                                .get()
                                .and_then(|account| account.tier)
                                .unwrap_or(AccountTier::Free)
                        })
                    },
                ))
                .child(when(move || account.get().is_some_and(|account| account.needs_login), || {
                    status_badge("需重新登录", ValueTone::Bad)
                }))
                .child(view().flex_1())
                .child(
                    text(age)
                        .flex_shrink_0()
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_1()
                        .children(client_launch_buttons(id))
                        .child(
                            view()
                                .w(px(1.0))
                                .h(px(14.0))
                                .mx(px(2.0))
                                .bg(move || theme().border),
                        )
                        .child(icon_action(
                            IconKind::Subtle,
                            icons::DOWNLOAD,
                            "导出这个账号",
                            move || store::export_accounts_action(Some(id)),
                        ))
                        .child(icon_action(
                            IconKind::Subtle,
                            icons::REFRESH,
                            "刷新这个账号",
                            move || crate::store::refresh_account_now(id),
                        ))
                        .child(icon_action(
                            IconKind::Danger,
                            icons::TRASH,
                            "删除这个账号",
                            move || pending_delete.set(Some(id)),
                        )),
                ),
        )
        .child(divider())
        .child(view().gap_1().child(each(
            models,
            |model: &crate::google::ModelQuota| model.name.clone(),
            move |model, _index| quota_row(move || model.get(), now_ms),
        )))
        .child(when(move || models().is_empty(), move || {
            text("正在读取额度…")
                .text_xs()
                .text_color(move || theme().muted_foreground)
        }))
        .child(when(move || error().is_some(), move || {
            text(move || error().map(|error| format!("刷新失败：{error}")).unwrap_or_default())
                .text_xs()
                .text_color(move || ValueTone::Bad.color())
        }))
}

fn client_launch_buttons(id: u32) -> Vec<AnyElement> {
    let targets = [
        (
            icons::ROCKET,
            "切换至该账号并启动 Antigravity 经典版",
            LaunchTarget::Classic,
        ),
        (
            icons::CODE_IDE,
            "切换至该账号并启动 Antigravity IDE",
            LaunchTarget::Ide,
        ),
        (
            icons::TERMINAL,
            "切换至该账号并启动 Antigravity CLI (agy)",
            LaunchTarget::Cli,
        ),
    ];
    targets
        .into_iter()
        .map(move |(markup, tip, target)| {
            icon_action_disabled(
                IconKind::Launch,
                markup,
                tip,
                move || store::switching_account_id().is_some(),
                move || store::launch_account_target(id, target),
            )
            .into_element()
        })
        .collect()
}

fn confirm_delete_dialog(pending_delete: Signal<Option<u32>>) -> impl IntoElement {
    let email = move || {
        pending_delete
            .get()
            .and_then(store::account_by_id)
            .map(|account| account.email)
            .unwrap_or_default()
    };
    let close = move || pending_delete.set(None);
    let confirm = move || {
        if let Some(id) = pending_delete.get_untracked() {
            pending_delete.set(None);
            store::remove_account(id);
        }
    };

    dialog()
        .on_cancel(close)
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || crate::ui::color("scrim"))
        .child(
            view()
                .w_full()
                .max_w(px(380.0))
                .mx_4()
                .gap_4()
                .p_6()
                .rounded(theme().radius)
                .border_1()
                .border_color(move || theme().border)
                .bg(move || theme().card)
                .shadow_lg()
                .child(
                    view()
                        .gap_1()
                        .child(
                            text("确认删除账号")
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text(move || {
                                let addr = email();
                                if addr.is_empty() {
                                    "删除后将从本地列表移除，需重新授权或导入才能恢复。".to_owned()
                                } else {
                                    format!("将从列表移除 {addr}，仅删除本地记录，需重新导入才能恢复。")
                                }
                            })
                            .text_xs()
                            .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(action(ButtonKind::Secondary, "取消", close))
                        .child(action(ButtonKind::Destructive, "确认删除", confirm)),
                ),
        )
}

fn confirm_close_dialog(close_prompt_open: Signal<bool>, window: WindowRef) -> impl IntoElement {
    let chosen_tray = signal(true);
    let remember = signal(false);
    let cancel = move || close_prompt_open.set(false);
    let confirm = move || {
        let to_tray = chosen_tray.get_untracked();
        let save_choice = remember.get_untracked();
        close_prompt_open.set(false);
        if save_choice {
            store::set_close_behavior(if to_tray {
                CloseBehavior::Tray
            } else {
                CloseBehavior::Exit
            });
        }
        if to_tray {
            window.hide();
        } else {
            quit();
        }
    };

    dialog()
        .on_cancel(cancel)
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || crate::ui::color("scrim"))
        .child(
            view()
                .w_full()
                .max_w(px(400.0))
                .mx_4()
                .gap_4()
                .p_6()
                .rounded(theme().radius)
                .border_1()
                .border_color(move || theme().border)
                .bg(move || theme().card)
                .shadow_lg()
                .child(
                    view()
                        .gap_1()
                        .child(
                            text("关闭 Wonton")
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text("点击关闭按钮时希望如何处理？")
                                .text_xs()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(
                    view()
                        .gap_2()
                        .child(radio_option(
                            move || chosen_tray.get(),
                            "最小化到系统托盘",
                            "隐藏主窗口，可从右下角托盘图标随时恢复",
                            move || chosen_tray.set(true),
                        ))
                        .child(radio_option(
                            move || !chosen_tray.get(),
                            "退出应用",
                            "完全关闭 Wonton 主窗口及系统托盘",
                            move || chosen_tray.set(false),
                        )),
                )
                .child(checkbox(
                    move || remember.get(),
                    || false,
                    "不再提示",
                    Some("不再提示（可在托盘右键菜单随时修改）"),
                    move |next| remember.set(next),
                ))
                .child(
                    view()
                        .flex_row()
                        .justify_end()
                        .gap_2()
                        .child(action(ButtonKind::Secondary, "取消", cancel))
                        .child(action(ButtonKind::Primary, "确定", confirm)),
                ),
        )
}

fn import_progress_dialog() -> impl IntoElement {
    let summary = move || {
        store::import_progress()
            .map(|progress| format!("已完成 {} / {}", progress.current, progress.total))
            .unwrap_or_default()
    };
    let detail = move || {
        store::import_progress()
            .map(|progress| format!("成功 {}  失败 {}", progress.success, progress.failed))
            .unwrap_or_default()
    };
    let current_email = move || {
        store::import_progress()
            .map(|progress| progress.current_email)
            .unwrap_or_default()
    };
    let percent = move || {
        let Some(progress) = store::import_progress() else {
            return 0.0;
        };
        if progress.total == 0 {
            0.0
        } else {
            (progress.current as f32 / progress.total as f32) * 100.0
        }
    };

    dialog()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || crate::ui::color("scrim"))
        .child(
            view()
                .w_full()
                .max_w(px(420.0))
                .mx_4()
                .gap_4()
                .p_6()
                .rounded(theme().radius)
                .border_1()
                .border_color(move || theme().border)
                .bg(move || theme().card)
                .shadow_lg()
                .child(
                    view()
                        .gap_1()
                        .child(
                            text("正在导入账号")
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text(current_email)
                                .text_xs()
                                .number_of_lines(1)
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(progress_bar(percent, "账号导入进度"))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .child(
                            text(summary)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text(detail)
                                .text_xs()
                                .tabular_nums()
                                .text_color(move || theme().muted_foreground),
                        ),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_slots() -> Vec<CardSlot> {
        // Mixed heights (e.g. a taller card with an Argon pool).
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
        // Dragging tall card 0 across slots [0, 1, 2] places it cleanly.
        assert_eq!(simulated_slot_y(0, 0, &slots, gap), 8.0);
        assert_eq!(simulated_slot_y(0, 1, &slots, gap), 140.0);
        assert_eq!(simulated_slot_y(0, 2, &slots, gap), 272.0);
        // Dragging short card 2 up to slot 0 shifts tall card 0 down.
        assert_eq!(simulated_slot_y(2, 0, &slots, gap), 8.0);
        assert_eq!(simulated_slot_y(2, 1, &slots, gap), 200.0);
    }

    #[test]
    fn resolves_target_index_and_sibling_shifts() {
        let slots = sample_slots();
        let gap = 12.0;
        // Center near slot 0 vs clearly past midpoint toward slot 1 (164.0).
        assert_eq!(target_index_for_center(0, 0, 100.0, &slots, gap), 0);
        assert_eq!(target_index_for_center(0, 0, 180.0, &slots, gap), 1);
        assert_eq!(target_index_for_center(0, 1, 310.0, &slots, gap), 2);

        // Moving card 0 -> 2 shifts cards 1 and 2 up by dragged height + gap.
        let span = 180.0 + gap;
        assert_eq!(sibling_shift_for(1, 0, 2, span), -span);
        assert_eq!(sibling_shift_for(2, 0, 2, span), -span);

        // Moving card 2 -> 0 shifts cards 0 and 1 down by dragged height + gap.
        let short_span = 120.0 + gap;
        assert_eq!(sibling_shift_for(0, 2, 0, short_span), short_span);
        assert_eq!(sibling_shift_for(1, 2, 0, short_span), short_span);
    }
}

