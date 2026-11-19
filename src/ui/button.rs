//! The app's buttons and icon actions, with uniform hover, active,
//! focus, disabled, and destructive feedback.

use nsh_base::{button, link, theme, tooltip};
use nsh_core::element::IntoTooltip;
use nsh_core::prelude::*;

use super::icons::svg_icon;
use super::{color, focus_ring, radius};

/// What a button means, not how it looks at one call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonKind {
    /// The one committal action of a region.
    Primary,
    /// An ordinary visible action.
    Secondary,
    /// A quiet toolbar or inline action.
    Ghost,
    /// An irreversible destructive action (e.g. confirming deletion).
    Destructive,
}

/// Visual emphasis for non-text icon buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconKind {
    /// Solid primary Gemini accent icon CTA (30pt toolbar scale).
    Primary,
    /// Bordered toolbar icon button (30pt scale).
    Secondary,
    /// Bordered toolbar client launcher (30pt scale).
    ToolbarLaunch,
    /// Pill surface used for per-account client launchers (26pt compact row scale).
    Launch,
    /// Borderless utility action (export, refresh, dismiss).
    Subtle,
    /// Borderless destructive trigger (delete -> confirmation dialog).
    Danger,
}

/// A labeled action, enabled.
pub(crate) fn action(
    kind: ButtonKind,
    label: impl IntoReactive<SharedString>,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    build(kind, label, || false, on_press)
}

/// A labeled action, disabled while `disabled` reads true.
pub(crate) fn action_disabled(
    kind: ButtonKind,
    label: impl IntoReactive<SharedString>,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    build(kind, label, disabled, on_press)
}

fn build(
    kind: ButtonKind,
    label: impl IntoReactive<SharedString>,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let element = button()
        .on_press(on_press)
        .disabled(disabled)
        .h_8()
        .px_3()
        .items_center()
        .justify_center()
        .rounded(radius)
        .transition_transform()
        .duration(150.0)
        .active(|s| s.scale(0.97))
        .focus_visible(focus_ring)
        .disabled_style(|s| s.opacity(0.4));
    match kind {
        ButtonKind::Primary => element
            .bg(move || theme().primary)
            .hover(|s| s.opacity(0.9))
            .active(|s| s.opacity(0.85))
            .child(
                text(label)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().primary_foreground),
            ),
        ButtonKind::Secondary => element
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().background)
            .hover(|s| s.bg(move || theme().accent))
            .active(|s| s.bg(move || theme().muted))
            .child(text(label).text_sm().text_color(move || theme().foreground)),
        ButtonKind::Ghost => element
            .hover(|s| s.bg(move || theme().accent))
            .active(|s| s.bg(move || theme().muted))
            .child(text(label).text_sm().text_color(move || theme().muted_foreground)),
        ButtonKind::Destructive => element
            .bg(move || theme().destructive)
            .hover(|s| s.opacity(0.9))
            .active(|s| s.opacity(0.85))
            .child(
                text(label)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || color("destructive-foreground")),
            ),
    }
}

/// A desktop hover/focus tooltip following the app's popover surface.
pub(crate) fn hint_tooltip(label: impl IntoReactive<SharedString>) -> impl IntoTooltip {
    tooltip().delay(250.0).child(
        view()
            .px_2()
            .py_1()
            .rounded(radius)
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().popover)
            .shadow_sm()
            .child(
                text(label)
                    .text_xs()
                    .text_color(move || theme().popover_foreground),
            ),
    )
}

/// A non-text vector icon action with screen-reader label and hover tooltip.
pub(crate) fn icon_action(
    kind: IconKind,
    markup: &'static str,
    label: &'static str,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    icon_action_with_tip(kind, markup, label, label, || false, on_press)
}

/// A non-text vector icon action that disables cleanly while `disabled` reads true.
pub(crate) fn icon_action_disabled(
    kind: IconKind,
    markup: &'static str,
    label: &'static str,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    icon_action_with_tip(kind, markup, label, label, disabled, on_press)
}

/// A non-text vector icon action with separate reactive tooltip text.
pub(crate) fn icon_action_with_tip(
    kind: IconKind,
    markup: &'static str,
    a11y_label: &'static str,
    tooltip_label: impl IntoReactive<SharedString>,
    disabled: impl Fn() -> bool + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let size = match kind {
        IconKind::Primary | IconKind::Secondary | IconKind::ToolbarLaunch => 30.0,
        IconKind::Launch | IconKind::Subtle | IconKind::Danger => 26.0,
    };
    let icon_size = match kind {
        IconKind::Primary | IconKind::Secondary | IconKind::ToolbarLaunch => 15.0,
        IconKind::Launch | IconKind::Subtle | IconKind::Danger => 14.0,
    };
    let base = button()
        .on_press(on_press)
        .disabled(disabled)
        .accessibility_label(a11y_label)
        .w(px(size))
        .h(px(size))
        .items_center()
        .justify_center()
        .rounded(radius)
        .transition_transform()
        .duration(150.0)
        .active(|s| s.scale(0.94))
        .focus_visible(focus_ring)
        .disabled_style(|s| s.opacity(0.4));

    let styled = match kind {
        IconKind::Primary => base
            .bg(move || theme().primary)
            .hover(|s| s.opacity(0.9))
            .active(|s| s.opacity(0.85))
            .child(svg_icon(markup, icon_size, move || theme().primary_foreground)),
        IconKind::Secondary => base
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().background)
            .hover(|s| s.bg(move || theme().accent).border_color(move || theme().ring))
            .active(|s| s.bg(move || theme().muted))
            .child(svg_icon(markup, icon_size, move || theme().foreground)),
        IconKind::ToolbarLaunch | IconKind::Launch => base
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().secondary)
            .hover(|s| s.bg(move || theme().accent).border_color(move || theme().ring))
            .active(|s| s.bg(move || theme().muted))
            .child(svg_icon(markup, icon_size, move || theme().foreground)),
        IconKind::Subtle => base
            .hover(|s| s.bg(move || theme().accent))
            .active(|s| s.bg(move || theme().muted))
            .child(svg_icon(markup, icon_size, move || theme().muted_foreground)),
        IconKind::Danger => base
            .hover(|s| s.bg(move || theme().muted))
            .active(|s| s.bg(move || theme().accent))
            .child(svg_icon(markup, icon_size, move || theme().destructive)),
    };

    view()
        .flex_shrink_0()
        .tooltip(hint_tooltip(tooltip_label))
        .child(styled)
}

/// One segment of a compact non-text segmented control (e.g. appearance modes).
pub(crate) fn segmented_icon(
    active: impl Fn() -> bool + Copy + 'static,
    markup: &'static str,
    label: &'static str,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    view()
        .flex_shrink_0()
        .tooltip(hint_tooltip(label))
        .child(
            button()
                .on_press(on_press)
                .accessibility_label(label)
                .w(px(26.0))
                .h(px(24.0))
                .items_center()
                .justify_center()
                .rounded(px(6.0))
                .style_when(active, |s| s.bg(move || theme().accent))
                .hover(|s| s.bg(move || theme().secondary))
                .active(|s| s.scale(0.95))
                .transition_transform()
                .duration(150.0)
                .focus_visible(focus_ring)
                .child(svg_icon(markup, 14.0, move || {
                    if active() {
                        theme().accent_foreground
                    } else {
                        theme().muted_foreground
                    }
                })),
        )
}

/// An external hyperlink styled with the app's primary accent and focus ring.
pub(crate) fn external_link<M>(
    label: impl IntoReactive<SharedString>,
    href: impl nsh_core::element::IntoProp<Option<SharedString>, M>,
) -> impl IntoElement {
    link()
        .href(href)
        .flex_row()
        .items_center()
        .gap_1()
        .rounded(px(4.0))
        .focus_visible(focus_ring)
        .hover(|s| s.opacity(0.8))
        .active(|s| s.opacity(0.65))
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().primary),
        )
        .child(svg_icon(super::icons::EXTERNAL_LINK, 12.0, move || {
            theme().primary
        }))
}

/// A button-styled external hyperlink (e.g. primary release download CTA).
pub(crate) fn external_link_button<M>(
    kind: ButtonKind,
    label: impl IntoReactive<SharedString>,
    href: impl nsh_core::element::IntoProp<Option<SharedString>, M>,
) -> impl IntoElement {
    let base = link()
        .href(href)
        .h_8()
        .px_3()
        .flex_row()
        .items_center()
        .justify_center()
        .gap_1p5()
        .rounded(radius)
        .transition_transform()
        .duration(150.0)
        .active(|s| s.scale(0.97))
        .focus_visible(focus_ring);
    match kind {
        ButtonKind::Primary => base
            .bg(move || theme().primary)
            .hover(|s| s.opacity(0.9))
            .active(|s| s.opacity(0.85))
            .child(
                text(label)
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().primary_foreground),
            )
            .child(svg_icon(super::icons::EXTERNAL_LINK, 13.0, move || {
                theme().primary_foreground
            })),
        _ => base
            .border_1()
            .border_color(move || theme().border)
            .bg(move || theme().background)
            .hover(|s| s.bg(move || theme().accent))
            .active(|s| s.bg(move || theme().muted))
            .child(text(label).text_sm().text_color(move || theme().foreground))
            .child(svg_icon(super::icons::EXTERNAL_LINK, 13.0, move || {
                theme().foreground
            })),
    }
}
