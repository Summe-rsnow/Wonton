//! Wonton-specific surface widgets (`tier_badge`).

use nsh_base::theme;
use nsh_core::prelude::*;

use super::color;

/// A compact pill naming an account's subscription tier (`ULTRA`, `PRO`, `FREE`).
pub(crate) fn tier_badge(
    tier: impl Fn() -> crate::google::AccountTier + Copy + 'static,
) -> impl IntoElement {
    use crate::google::AccountTier;
    let label = move || SharedString::from(tier().label());
    let fg = move || match tier() {
        AccountTier::Ultra => theme().primary,
        AccountTier::Pro => color("warning"),
        AccountTier::Free => theme().muted_foreground,
    };
    let bg = move || match tier() {
        AccountTier::Ultra => theme().primary.opacity(0.16),
        AccountTier::Pro => color("warning").opacity(0.16),
        AccountTier::Free => theme().muted,
    };
    view()
        .px_2()
        .h(px(18.0))
        .items_center()
        .justify_center()
        .rounded(bento::ui::radius)
        .bg(bg)
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(fg),
        )
}
