//! The About & Update Check dialog: shows app version, repository info,
//! GitHub Releases update status, and direct links to GitHub Releases.

use nsh_base::theme;
use nsh_core::prelude::*;

use crate::store::{self, UpdateStatus};
use crate::ui::{
    action, action_disabled, external_link, external_link_button, icons, modal_dialog, radius,
    ButtonKind, ValueTone,
};
use crate::update::{GITHUB_RELEASES_URL, GITHUB_REPO_URL};

/// What the dialog reports: the user asked to close it.
#[derive(Debug, Clone, PartialEq, Event)]
pub(crate) enum AboutDialogEvent {
    Close,
}

/// Modal dialog presenting application version metadata and GitHub Releases update checks.
#[component]
pub(crate) fn AboutDialog(cx: Scope<AboutDialogEvent>) -> impl IntoElement {
    let close = move || cx.emit(AboutDialogEvent::Close);
    let status = computed(store::update_status);

    let status_text = move || match status.get() {
        UpdateStatus::Idle => "点击“检测更新”，从 GitHub Releases 获取最新版本。".to_owned(),
        UpdateStatus::Checking => "正在检查 GitHub Releases…".to_owned(),
        UpdateStatus::UpToDate { latest } => format!("当前已是最新版本（v{latest}）"),
        UpdateStatus::Available(info) => match info.published_date {
            Some(date) => format!("发现新版本 v{}（发布于 {date}）", info.version),
            None => format!("发现新版本 v{}", info.version),
        },
        UpdateStatus::NoRelease => {
            "暂未检测到已发布的正式 Release，可前往 GitHub Releases 查看。".to_owned()
        }
        UpdateStatus::Error(err) => format!("检测失败：{err}"),
    };

    let status_color = move || match status.get() {
        UpdateStatus::UpToDate { .. } | UpdateStatus::Available(_) => ValueTone::Ok.color(),
        UpdateStatus::Error(_) => ValueTone::Bad.color(),
        UpdateStatus::Idle | UpdateStatus::Checking | UpdateStatus::NoRelease => {
            theme().muted_foreground
        }
    };

    let release_notes = move || match status.get() {
        UpdateStatus::Available(info) => info.notes,
        _ => None,
    };

    let release_url = move || match status.get() {
        UpdateStatus::Available(info) => SharedString::from(info.html_url),
        _ => SharedString::from(GITHUB_RELEASES_URL),
    };

    let has_update = move || matches!(status.get(), UpdateStatus::Available(_));

    modal_dialog(
        440.0,
        close,
        view()
            .gap_4()
            .child(
                view()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(icons::brand_mark(40.0))
                    .child(
                        view()
                            .flex_1()
                            .gap_1()
                            .child(
                                view()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        text("Wonton")
                                            .text_base()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(move || theme().foreground),
                                    )
                                    .child(
                                        view()
                                            .px_2()
                                            .py(px(2.0))
                                            .rounded(px(6.0))
                                            .bg(move || theme().muted)
                                            .child(
                                                text(format!("v{}", crate::APP_VERSION))
                                                    .text_xs()
                                                    .tabular_nums()
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(move || theme().muted_foreground),
                                            ),
                                    ),
                            )
                            .child(
                                text("轻量、原生的 Antigravity 多账号与额度管理工具")
                                    .text_xs()
                                    .text_color(move || theme().muted_foreground),
                            ),
                    ),
            )
            .child(
                view()
                    .p_3()
                    .gap_2()
                    .rounded(radius)
                    .border_1()
                    .border_color(move || theme().border)
                    .bg(move || theme().muted)
                    .child(info_row(
                        "当前版本",
                        text(format!("v{}", crate::APP_VERSION))
                            .text_xs()
                            .tabular_nums()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(move || theme().foreground),
                    ))
                    .child(info_row(
                        "开源协议",
                        text("Apache-2.0")
                            .text_xs()
                            .text_color(move || theme().foreground),
                    ))
                    .child(info_row(
                        "项目仓库",
                        external_link(
                            "Summe-rsnow/Wonton",
                            Some(SharedString::from(GITHUB_REPO_URL)),
                        ),
                    )),
            )
            .child(
                view()
                    .p_3()
                    .gap_2()
                    .rounded(radius)
                    .border_1()
                    .border_color(move || theme().border)
                    .bg(move || theme().background)
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                text("版本更新")
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(move || theme().foreground),
                            )
                            .child(action_disabled(
                                ButtonKind::Secondary,
                                move || {
                                    if store::update_checking() {
                                        "正在检测…"
                                    } else {
                                        "检测更新"
                                    }
                                },
                                store::update_checking,
                                store::check_for_updates_action,
                            )),
                    )
                    .child(
                        text(status_text)
                            .text_xs()
                            .text_color(status_color),
                    )
                    .child(when(move || release_notes().is_some(), move || {
                        text(move || release_notes().unwrap_or_default())
                            .text_xs()
                            .number_of_lines(3)
                            .text_color(move || theme().muted_foreground)
                    }))
                    .child(
                        view()
                            .pt_1()
                            .flex_row()
                            .items_center()
                            .child(when(has_update, move || {
                                external_link_button(
                                    ButtonKind::Primary,
                                    "前往 GitHub Release 下载",
                                    move || Some(release_url()),
                                )
                            }))
                            .child(when(move || !has_update(), move || {
                                external_link(
                                    "打开发布页 (GitHub Releases)",
                                    Some(SharedString::from(GITHUB_RELEASES_URL)),
                                )
                            })),
                    ),
            )
            .child(
                view()
                    .flex_row()
                    .justify_end()
                    .child(action(ButtonKind::Ghost, "关闭", close)),
            ),
    )
}

fn info_row(label: &'static str, value: impl IntoElement) -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .child(
            text(label)
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(value)
}
