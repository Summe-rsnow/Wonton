//! Surfaces and the small pieces every screen repeats: the card an account
//! sits on, a section heading, a hairline, the empty state and the notice.

use nsh_base::{dialog, theme};
use nsh_core::prelude::*;

use super::{color, icon_action, icons, radius, IconKind, ValueTone};

/// A grouped surface: one hairline boundary, no shadow. The base surface stays
/// flat, so elevation is not spent on cards.
pub(crate) fn card() -> View {
    view()
        .px_3()
        .py(px(10.0))
        .gap_2()
        .rounded(radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
}

/// A hairline between regions; the boundary's owner draws it once.
pub(crate) fn divider() -> impl IntoElement {
    view().h(px(1.0)).bg(move || theme().border)
}

/// The empty state: what this screen is waiting for, and the next action.
pub(crate) fn empty_state(title: &'static str, hint: &'static str) -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .gap_2()
        .p_8()
        .child(text(title).text_sm().text_color(move || theme().foreground))
        .child(text(hint).text_xs().text_color(move || theme().muted_foreground))
}

/// A short status, in a quiet pill: the state is named in words, never by
/// color alone.
pub(crate) fn status_badge(label: impl IntoReactive<SharedString>, tone: ValueTone) -> impl IntoElement {
    view()
        .px_2()
        .h(px(20.0))
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(move || tone.color().opacity(0.16))
        .child(text(label).text_xs().text_color(move || tone.color()))
}

/// A floating feedback toast that occupies zero layout space in the column.
pub(crate) fn notice_toast(
    message: impl Fn() -> String + 'static,
    color: impl Fn() -> AdaptiveColor + Copy + 'static,
    on_close: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .pointer_events(PointerEvents::Auto)
        .flex_row()
        .items_center()
        .gap_2()
        .pl_3()
        .pr_1()
        .py_1()
        .max_w(relative(1.0))
        .rounded(radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .shadow_md()
        .child(view().size_2().flex_shrink_0().rounded_full().bg(color))
        .child(
            text(message)
                .min_w(px(0.0))
                .flex_shrink(1.0)
                .text_xs()
                .text_color(move || theme().foreground),
        )
        .child(icon_action(IconKind::Subtle, icons::CLOSE, "关闭提示", on_close))
}

/// A compact pill naming an account's subscription tier (`ULTRA`, `PRO`, `FREE`).
pub(crate) fn tier_badge(
    tier: impl Fn() -> crate::google::AccountTier + Copy + 'static,
) -> impl IntoElement {
    use crate::google::AccountTier;
    let label = move || SharedString::from(tier().label());
    let fg = move || match tier() {
        AccountTier::Ultra => theme().primary,
        AccountTier::Pro => super::color("warning"),
        AccountTier::Free => theme().muted_foreground,
    };
    let bg = move || match tier() {
        AccountTier::Ultra => theme().primary.opacity(0.16),
        AccountTier::Pro => super::color("warning").opacity(0.16),
        AccountTier::Free => theme().muted,
    };
    view()
        .px_2()
        .h(px(18.0))
        .items_center()
        .justify_center()
        .rounded(radius)
        .bg(bg)
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg),
        )
}

/// Shared modal dialog frame: theme scrim backdrop + centered card with
/// consistent border, radius, elevation, and entrance motion.
pub(crate) fn modal_dialog(
    max_width: f32,
    on_cancel: impl Fn() + 'static,
    content: impl IntoElement,
) -> impl IntoElement {
    let enter = motion().entering(MotionPreset::FadeInUp);
    let card_view = view()
        .bind_motion(&enter)
        .w_full()
        .max_w(px(max_width))
        .mx_4()
        .gap_4()
        .p_6()
        .rounded(theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .shadow_lg()
        .child(content);

    dialog()
        .on_cancel(on_cancel)
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || color("scrim"))
        .child(card_view)
}

/// Non-dismissible modal dialog frame for in-flight progress (e.g. batch import).
pub(crate) fn progress_modal_dialog(max_width: f32, content: impl IntoElement) -> impl IntoElement {
    let card_view = view()
        .w_full()
        .max_w(px(max_width))
        .mx_4()
        .gap_4()
        .p_6()
        .rounded(theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .shadow_lg()
        .child(content);

    dialog()
        .flex_1()
        .items_center()
        .justify_center()
        .bg(move || color("scrim"))
        .child(card_view)
}
