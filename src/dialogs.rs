//! Auxiliary modal dialogs used by the main screen: delete confirmation,
//! window-close confirmation, and batch import progress.

use nsh_base::theme;
use nsh_core::element::desktop::{quit, WindowRef};
use nsh_core::prelude::*;

use crate::store::{self, CloseBehavior};
use crate::ui::{
    action, checkbox, modal_dialog, progress_bar, progress_modal_dialog, radio_option, ButtonKind,
};

pub(crate) fn confirm_delete_dialog(pending_delete: Signal<Option<u32>>) -> impl IntoElement {
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

    modal_dialog(
        380.0,
        close,
        view()
            .gap_4()
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
                                format!(
                                    "将从列表移除 {addr}，仅删除本地记录，需重新导入才能恢复。"
                                )
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

pub(crate) fn confirm_close_dialog(
    close_prompt_open: Signal<bool>,
    window: WindowRef,
) -> impl IntoElement {
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

    modal_dialog(
        400.0,
        cancel,
        view()
            .gap_4()
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

pub(crate) fn import_progress_dialog() -> impl IntoElement {
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

    progress_modal_dialog(
        420.0,
        view()
            .gap_4()
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
