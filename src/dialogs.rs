//! Wonton-specific modal dialogs: delete confirmation and batch import progress.
//! Window-close confirmation and the About dialog are provided by `bento`.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::store;
use crate::ui::{action, modal_dialog, progress_bar, progress_modal_dialog, ButtonKind};

pub(crate) fn confirm_delete_dialog(pending_delete: Signal<Option<Vec<u32>>>) -> impl IntoElement {
    let title = move || {
        let count = pending_delete.with(|ids| ids.as_ref().map_or(0, Vec::len));
        if count > 1 {
            format!("确认删除 {count} 个账号")
        } else {
            "确认删除账号".to_owned()
        }
    };
    let description = move || {
        let ids = pending_delete.get().unwrap_or_default();
        if ids.len() <= 1 {
            let addr = ids
                .first()
                .copied()
                .and_then(store::account_by_id)
                .map(|account| account.email)
                .unwrap_or_default();
            if addr.is_empty() {
                "删除后将从本地列表移除，需重新授权或导入才能恢复。".to_owned()
            } else {
                format!("将从列表移除 {addr}，仅删除本地记录，需重新导入才能恢复。")
            }
        } else {
            format!(
                "将从列表移除已选的 {} 个账号，仅删除本地记录，需重新导入才能恢复。",
                ids.len()
            )
        }
    };
    let close = move || pending_delete.set(None);
    let confirm = move || {
        if let Some(ids) = pending_delete.get_untracked() {
            pending_delete.set(None);
            store::remove_accounts(&ids);
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
                        text(title)
                            .text_base()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(move || theme().foreground),
                    )
                    .child(
                        text(description)
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
