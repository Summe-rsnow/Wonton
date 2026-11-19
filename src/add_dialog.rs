//! The add-account dialog: the three ways to add an account, as tabs.
//!
//! Whichever tab is used, the result lands in the same list, so the dialog
//! reports when to close and the screen owns that state. Its fields are
//! signals of this component, disposed with it when the dialog closes.

use nsh_base::{dialog, input, tab, tabs, theme};
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

use crate::google::ClientKind;
use crate::store;
use crate::ui::{action, action_disabled, toggle_switch, ButtonKind, ValueTone};

/// What the dialog reports: the user asked to close it.
#[derive(Debug, Clone, PartialEq, Event)]
pub(crate) enum AddDialogEvent {
    /// Close the dialog; nothing else changed.
    Close,
}

/// Which way of adding is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Oauth,
    Token,
    Import,
}

/// The dialog. It is created while open, so its fields start empty each time.
#[component]
pub(crate) fn AddDialog(cx: Scope<AddDialogEvent>) -> impl IntoElement {
    let tab_now = signal(Tab::Oauth);
    let enterprise = signal(false);
    let tokens = signal(String::new());
    let kind = move || {
        if enterprise.get() {
            ClientKind::Enterprise
        } else {
            ClientKind::Personal
        }
    };

    // The scrim is the dialog's, since `dialog()` paints nothing itself.
    dialog()
        .on_cancel(move || {
            cx.emit(AddDialogEvent::Close);
        })
        .flex_1()
        .items_center()
        .justify_center()
        // A dialog paints nothing itself; the scrim is the app's, from a token
        // that carries its own alpha.
        .bg(move || crate::ui::color("scrim"))
        .child(
            view()
                .w_full()
                .max_w(px(520.0))
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
                            text("添加账号")
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(move || theme().foreground),
                        )
                        .child(
                            text("网页授权最省事；也可以粘贴 refresh_token，或直接读本机已登录的账号。")
                                .text_xs()
                                .text_color(move || theme().muted_foreground),
                        ),
                )
                .child(tab_row(tab_now))
                .child(
                    view()
                        .min_h(px(132.0))
                        .gap_3()
                        .child(when(
                            move || tab_now.get() == Tab::Oauth,
                            move || oauth_panel(kind),
                        ))
                        .child(when(
                            move || tab_now.get() == Tab::Token,
                            move || token_panel(tokens, kind),
                        ))
                        .child(when(move || tab_now.get() == Tab::Import, import_panel)),
                )
                .child(enterprise_switch(enterprise))
                .child(notice_line())
                .child(
                    view().flex_row().justify_end().child(action(
                        ButtonKind::Ghost,
                        "关闭",
                        move || {
                            cx.emit(AddDialogEvent::Close);
                        },
                    )),
                ),
        )
}

fn tab_row(current: Signal<Tab>) -> impl IntoElement {
    let item = move |value: Tab, label: &'static str| {
        let selected = move || current.get() == value;
        tab()
            .selected(selected)
            .on_press(move |_: &()| current.set(value))
            .px_3()
            .h(px(28.0))
            .items_center()
            .justify_center()
            .rounded(theme().radius)
            .child(text(label).text_xs().text_color(move || {
                if selected() {
                    theme().foreground
                } else {
                    theme().muted_foreground
                }
            }))
    };
    tabs()
        .flex_row()
        .self_start()
        .gap_1()
        .p_1()
        .rounded(theme().radius)
        .bg(move || theme().muted)
        .indicator_style(|s| {
            s.top_1()
                .bottom_1()
                .rounded(theme().radius)
                .bg(move || theme().background)
        })
        .child(item(Tab::Oauth, "网页授权"))
        .child(item(Tab::Token, "手动 Token"))
        .child(item(Tab::Import, "本机导入"))
}

fn oauth_panel(kind: impl Fn() -> ClientKind + 'static + Copy) -> impl IntoElement {
    view()
        .gap_3()
        .child(
            text("在浏览器中完成 Google 授权，回调后自动添加，并立即读取额度。")
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view().flex_row().child(action_disabled(
                ButtonKind::Primary,
                move || if store::busy() { "等待授权…" } else { "打开授权页面" },
                store::busy,
                move || store::add_account_by_oauth(kind()),
            )),
        )
}

fn token_panel(tokens: Signal<String>, kind: impl Fn() -> ClientKind + 'static + Copy) -> impl IntoElement {
    view()
        .gap_3()
        .child(
            text("粘贴 refresh_token，或 JSON 数组（取其中的 refresh_token 字段）。可以一次粘贴多个。")
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(
            input()
                .value(move || tokens.get())
                .on_change(move |text: &str| tokens.set(text.to_owned()))
                .multiline(true)
                .h(px(96.0))
                .p_2()
                .text_xs()
                .rounded(theme().radius)
                .border_1()
                .border_color(move || theme().input)
                .bg(move || theme().background)
                .text_color(move || theme().foreground)
                .placeholder(r#"1//0g…   或   [{"refresh_token":"1//0g…"}]"#)
                .focus(|s: StyleRefinement| s.border_color(move || theme().ring)),
        )
        .child(
            view().flex_row().child(action_disabled(
                ButtonKind::Primary,
                move || if store::busy() { "正在添加…" } else { "添加" },
                move || store::busy() || tokens.with(|text| text.trim().is_empty()),
                move || store::add_tokens(tokens.get_untracked(), kind()),
            )),
        )
}

fn import_panel() -> impl IntoElement {
    view()
        .gap_3()
        .child(
            text("读取本机已登录 Antigravity 的 state.vscdb，用其中的 refresh_token 添加账号。")
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view().flex_row().child(action_disabled(
                ButtonKind::Primary,
                move || if store::busy() { "正在读取…" } else { "从本机导入" },
                store::busy,
                store::import_local,
            )),
        )
}

fn enterprise_switch(enterprise: Signal<bool>) -> impl IntoElement {
    toggle_switch(
        move || enterprise.get(),
        "使用企业版（GCP）客户端",
        Some("使用企业版（GCP）客户端"),
        move |next| enterprise.set(next),
    )
}

/// The store's notice, shown inside the dialog while it is open.
fn notice_line() -> impl IntoElement {
    view().min_h(px(18.0)).child(
        text(move || store::notice().map(|notice| notice.text).unwrap_or_default())
            .text_xs()
            .text_color(move || {
                match store::notice().map(|notice| notice.tone) {
                    Some(store::Tone::Ok) => ValueTone::Ok.color(),
                    Some(store::Tone::Bad) => ValueTone::Bad.color(),
                    _ => ValueTone::Ok.color(),
                }
            }),
    )
}
