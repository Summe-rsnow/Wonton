//! The one screen: a header with version & about, the proxy/action toolbar,
//! and the draggable account cards.

use std::time::Duration;

use bento::drag_list::DragList;
use bento::shell::{
    about_icon_button, bind_close_handler, close_behavior_submenu, confirm_close_dialog,
    theme_mode_segmented, title_bar_brand, NoticeCenter,
};
use nsh_base::theme;
use nsh_core::Cursor;
use nsh_core::element::desktop::{
    current_window, quit, set_tray, title_bar, tray, MenuItem,
};
use nsh_core::prelude::*;
use nsh_core::time::unix_ms;

use crate::dialogs::{confirm_delete_dialog, import_progress_dialog};
use crate::format;
use crate::google::AccountTier;
use crate::launch::{LaunchTarget, ProxyConfig};
use crate::store::{self, Tone};
use crate::ui::{
    card, checkbox, divider, empty_state, hint_tooltip, icon_action, icon_action_disabled,
    icon_action_spinning_with_tip, icon_action_with_tip, icons, quota_row, status_badge,
    text_field, tier_badge, toggle_switch, toolbar_separator, IconKind, ValueTone,
};

#[component]
pub(crate) fn Screen() -> impl IntoElement {
    let add_dialog_open = signal(false);
    let about_dialog_open = signal(false);
    let pending_delete: Signal<Option<Vec<u32>>> = signal(None);
    let close_prompt_open = signal(false);
    let saved = store::proxy();
    let host = signal(saved.host.clone());
    let port = signal(saved.port.to_string());
    // The clock the "3 分钟前" and reset labels read, so they age without a
    // refresh; one tick a minute is enough for minute-grained text.
    let clock = signal(unix_ms() as i64);
    set_interval(Duration::from_secs(60), move || clock.set(unix_ms() as i64));
    let notices = NoticeCenter::global();
    notices.install_auto_dismiss();

    let main_window = current_window();
    bind_close_handler(main_window.clone(), store::close_behavior, move || {
        close_prompt_open.set(true);
    });
    {
        let win_click = main_window.clone();
        let win_menu = main_window.clone();
        let win_about = main_window.clone();
        effect(move || {
            let behavior = store::close_behavior();
            let count = store::account_count();
            let busy = store::busy();
            let tooltip = if count > 0 {
                format!("Wonton v{} · {count} 个账号", crate::APP_VERSION)
            } else {
                format!("Wonton v{}", crate::APP_VERSION)
            };
            let win_on_click = win_click.clone();
            let win_on_show = win_menu.clone();
            let win_on_about = win_about.clone();
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
                        MenuItem::new("关于与检查更新…").on_select(move || {
                            win_on_about.focus();
                            about_dialog_open.set(true);
                        }),
                        MenuItem::separator(),
                        close_behavior_submenu(behavior, store::set_close_behavior),
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
        .child(header(about_dialog_open))
        .child(proxy_bar(host, port, add_dialog_open, pending_delete))
        .child(accounts(clock, pending_delete))
        .child(notices.overlay(move || {
            !add_dialog_open.get()
                && !about_dialog_open.get()
                && pending_delete.get().is_none()
                && !close_prompt_open.get()
        }))
        .child(when(add_dialog_open, move || {
            crate::add_dialog::add_dialog().on_close(move || add_dialog_open.set(false))
        }))
        .child(when(about_dialog_open, move || {
            bento::updater::about_dialog(
                crate::APP_META,
                store::update_status,
                store::check_for_updates_action,
                move || about_dialog_open.set(false),
            )
        }))
        .child(when(
            move || pending_delete.get().is_some(),
            move || confirm_delete_dialog(pending_delete),
        ))
        .child(when(close_prompt_open, move || {
            confirm_close_dialog(
                close_prompt_open,
                prompt_window.clone(),
                crate::APP_META.name,
                store::set_close_behavior,
            )
        }))
        .child(when(move || store::import_progress().is_some(), import_progress_dialog))
}

fn header(about_dialog_open: Signal<bool>) -> impl IntoElement {
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
        .child(title_bar_brand(crate::APP_META))
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
                .child(theme_mode_segmented(
                    store::theme_mode,
                    store::set_theme_mode,
                ))
                .child(toolbar_separator())
                .child(about_icon_button(
                    crate::APP_META,
                    move || store::available_update().map(|u| u.version),
                    move || {
                        store::clear_notice();
                        about_dialog_open.set(true);
                    },
                )),
        )
}

fn proxy_bar(
    host: Signal<String>,
    port: Signal<String>,
    add_dialog_open: Signal<bool>,
    pending_delete: Signal<Option<Vec<u32>>>,
) -> impl IntoElement {
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
                .gap(px(6.0))
                .opacity(move || if store::proxy().enabled { 1.0_f32 } else { 0.45_f32 })
                .child(
                    text_field(host, "127.0.0.1", move |text: &str| host.set(text.to_owned()))
                        .label("主机地址")
                        .w(px(136.0)),
                )
                .child(
                    text(":")
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                )
                .child(
                    text_field(port, "7890", move |text: &str| port.set(text.to_owned()))
                        .label("端口")
                        .w(px(64.0)),
                ),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
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
                )),
        )
        .child(view().flex_1())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(px(4.0))
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
                .child(icon_action_spinning_with_tip(
                    IconKind::Secondary,
                    icons::REFRESH,
                    move || {
                        if store::selected_count() > 0 {
                            SharedString::from("刷新选中账号")
                        } else {
                            SharedString::from("全部刷新")
                        }
                    },
                    move || {
                        if store::is_any_refreshing() || store::busy() {
                            "正在刷新…".to_owned()
                        } else {
                            let count = store::selected_count();
                            if count > 0 {
                                format!("刷新已选 {count} 个账号")
                            } else {
                                "刷新全部账号".to_owned()
                            }
                        }
                    },
                    store::busy,
                    store::is_any_refreshing,
                    store::refresh_selected_or_all,
                ))
                .child(icon_action_with_tip(
                    IconKind::ToolbarDanger,
                    icons::TRASH,
                    "删除",
                    move || {
                        let count = store::selected_count();
                        if count > 0 {
                            format!("删除已选 {count} 个账号")
                        } else {
                            "删除已选账号（请先勾选）".to_owned()
                        }
                    },
                    move || store::selected_count() == 0 || store::busy(),
                    move || {
                        let ids = store::selected_account_ids();
                        if !ids.is_empty() {
                            pending_delete.set(Some(ids));
                        }
                    },
                ))
                .child(icon_action(
                    IconKind::Primary,
                    icons::PLUS,
                    "添加账号…",
                    move || {
                        store::clear_notice();
                        add_dialog_open.set(true);
                    },
                ))
                .child(toolbar_separator())
                .child(icon_action_with_tip(
                    IconKind::ToolbarLaunch,
                    icons::ROCKET,
                    "启动反重力",
                    "启动 Antigravity 经典版",
                    || false,
                    move || store::launch_target_app(LaunchTarget::Classic),
                ))
                .child(icon_action_with_tip(
                    IconKind::ToolbarLaunch,
                    icons::CODE_IDE,
                    "启动 Antigravity IDE",
                    "启动 Antigravity IDE",
                    || false,
                    move || store::launch_target_app(LaunchTarget::Ide),
                ))
                .child(icon_action_with_tip(
                    IconKind::ToolbarLaunch,
                    icons::TERMINAL,
                    "启动 Antigravity CLI",
                    "启动 Antigravity CLI (agy)",
                    || false,
                    move || store::launch_target_app(LaunchTarget::Cli),
                )),
        )
}

fn accounts(clock: Signal<i64>, pending_delete: Signal<Option<Vec<u32>>>) -> impl IntoElement {
    let drag = DragList::new(store::account_ids, store::reorder_account);

    view()
        .flex_1()
        .child(when(
            move || store::account_count() == 0,
            || {
                view().flex_1().child(empty_state(
                    "还没有账号",
                    "点击上方“+”添加账号，或使用导入图标读取 JSON 文件。",
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
                    format!("已选 {selected}/{total} 个（导出 / 刷新 / 删除仅含所选）")
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

/// One account's card. Every value is bound via memoized UI-only view projection
/// so token renewals or unrelated store updates do not cascade re-evaluations.
fn account_card(
    id: u32,
    clock: Signal<i64>,
    drag: DragList,
    pending_delete: Signal<Option<Vec<u32>>>,
) -> impl IntoElement {
    let offset_y = animated_value(0.0);
    drag.register_motion(id, offset_y);
    {
        let drag = drag.clone();
        cleanup(move || drag.unregister(id));
    }

    let now_ms = move || clock.get();
    let account = computed(move || store::account_card_view(id));
    let email = move || account.get().map(|account| account.email).unwrap_or_default();
    let select_label = move || {
        SharedString::from(format!(
            "选择 {}",
            account.get().map(|account| account.email).unwrap_or_default()
        ))
    };
    let age = move || {
        account
            .get()
            .map(|account| format::age_label(account.fetched_at, now_ms()))
            .unwrap_or_default()
    };
    let models = computed(move || account.get().map(|account| account.models).unwrap_or_default());
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
                        .child(icon_action_spinning_with_tip(
                            IconKind::Subtle,
                            icons::REFRESH,
                            "刷新这个账号",
                            move || {
                                if account.get().is_some_and(|account| account.refreshing) {
                                    "正在刷新…"
                                } else {
                                    "刷新这个账号"
                                }
                            },
                            move || account.get().is_some_and(|account| account.refreshing),
                            move || account.get().is_some_and(|account| account.refreshing),
                            move || crate::store::refresh_account_now(id),
                        ))
                        .child(icon_action(
                            IconKind::Danger,
                            icons::TRASH,
                            "删除这个账号",
                            move || pending_delete.set(Some(vec![id])),
                        )),
                ),
        )
        .child(divider())
        .child(view().gap_1().child(each(
            move || models.get(),
            |model: &crate::google::ModelQuota| model.name.clone(),
            move |model, _index| quota_row(move || model.get(), now_ms),
        )))
        .child(when(move || models.with(|m| m.is_empty()), move || {
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nsh_core::platform::{Idiom, Os};
    use nsh_core::prelude::{px, size};
    use nsh_core::time::unix_ms;
    use nsh_engine::{EngineConfig, Root};
    use nsh_testing::{Headless, MemoryStorage};

    use crate::google::{AccountTier, ModelQuota};
    use crate::store::{seed_storage_entry_for_test, ThemeMode};

    const GONE: Duration = Duration::from_secs(5);

    #[test]
    fn selecting_accounts_updates_refresh_button_and_allows_batch_delete() {
        let now = unix_ms() as i64;
        let sample_models = || {
            vec![ModelQuota {
                name: "Gemini".to_owned(),
                remaining: 0.9,
                reset: None,
                weekly_remaining: Some(0.95),
                weekly_reset: None,
                is_weekly_only: false,
            }]
        };
        let entry = seed_storage_entry_for_test(
            vec![
                ("a@b.c", AccountTier::Ultra, sample_models(), Some(now)),
                ("d@e.f", AccountTier::Pro, sample_models(), Some(now)),
                ("g@h.i", AccountTier::Free, sample_models(), Some(now)),
            ],
            ThemeMode::Light,
        );

        let mut config = EngineConfig::new(Root::of::<crate::App>());
        config.viewport = size(px(860.0), px(640.0));
        config.environment.os = Os::Windows;
        config.environment.idiom = Idiom::Desktop;
        config.storage = Some(MemoryStorage::new([entry]));
        let mut app = Headless::start(config).unwrap();

        app.wait_for("a@b.c").unwrap();
        assert!(app.screen().matches("role:button[全部刷新]").unwrap());
        assert!(!app.screen().matches("role:button[刷新选中账号]").unwrap());

        // Select two accounts -> refresh button becomes selection-scoped and delete opens multi-delete dialog.
        app.press("role:checkbox[选择 a@b.c]").unwrap();
        app.wait_for("已选 1/3 个（导出 / 刷新 / 删除仅含所选）").unwrap();
        assert!(app.screen().matches("role:button[刷新选中账号]").unwrap());
        assert!(!app.screen().matches("role:button[全部刷新]").unwrap());

        app.press("role:checkbox[选择 d@e.f]").unwrap();
        app.wait_for("已选 2/3 个（导出 / 刷新 / 删除仅含所选）").unwrap();

        app.press("role:button[删除]").unwrap();
        app.wait_for("确认删除 2 个账号").unwrap();
        app.press("role:button[确认删除]").unwrap();
        app.wait_for_gone("确认删除 2 个账号", GONE).unwrap();

        assert!(!app.screen().shows("a@b.c"));
        assert!(!app.screen().shows("d@e.f"));
        assert!(app.screen().shows("g@h.i"));
        assert!(app.screen().matches("role:button[全部刷新]").unwrap());
    }
}
