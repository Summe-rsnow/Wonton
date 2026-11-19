//! Text fields, switches, radio choices, and selection checkboxes.

use nsh_base::{button, input, switch, theme, Input};
use nsh_core::prelude::*;

use super::{focus_ring, radius};

/// A one-line text field with the app's frame.
pub(crate) fn text_field(
    value: impl IntoReactive<String>,
    placeholder: &'static str,
    on_change: impl Fn(&str) + 'static,
) -> Input {
    input()
        .value(value)
        .on_change(on_change)
        .placeholder(placeholder)
        .h_8()
        .px_3()
        .text_sm()
        .rounded(radius)
        .border_1()
        .border_color(move || theme().input)
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .focus(|s| s.border_color(move || theme().ring))
}

/// A selection checkbox with optional adjacent label and generous hit area.
pub(crate) fn checkbox<M>(
    checked: impl Fn() -> bool + Copy + 'static,
    indeterminate: impl Fn() -> bool + Copy + 'static,
    accessibility_label: impl nsh_core::element::IntoProp<Option<SharedString>, M>,
    label: Option<&'static str>,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    let base = nsh_base::checkbox()
        .checked(checked)
        .indeterminate(indeterminate)
        .accessibility_label(accessibility_label)
        .hit_slop(Points(6.0))
        .flex_row()
        .items_center()
        .gap_2()
        .focus_visible(focus_ring)
        .on_change(move |next: &bool| on_change(*next))
        .indicator_class("w-4 h-4 items-center justify-center border border-input bg-background transition-colors duration-150 checked:bg-primary checked:border-primary indeterminate:bg-primary indeterminate:border-primary")
        .indicator_style(|s| s.rounded(px(4.0)))
        .indicator(
            view()
                .items_center()
                .justify_center()
                .child(when(indeterminate, || {
                    text("−")
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(move || theme().primary_foreground)
                }))
                .child(when(
                    move || checked() && !indeterminate(),
                    || {
                        text("✓")
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(move || theme().primary_foreground)
                    },
                )),
        );
    match label {
        Some(text_label) => base
            .child(
                text(text_label)
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
            )
            .into_element(),
        None => base.into_element(),
    }
}

/// A themed toggle switch with optional adjacent label.
pub(crate) fn toggle_switch(
    checked: impl Fn() -> bool + Copy + 'static,
    accessibility_label: &'static str,
    label: Option<&'static str>,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    let base = switch()
        .checked(checked)
        .accessibility_label(accessibility_label)
        .on_change(move |next: &bool| on_change(*next))
        .focus_visible(focus_ring)
        .h_8()
        .flex_row()
        .items_center()
        .gap_2()
        .track_class("w-9 h-5 rounded-full bg-muted border border-border px-0.5 justify-center transition-colors checked:bg-primary checked:border-primary")
        .thumb_class("size-3.5 rounded-full bg-foreground transition-transform checked:translate-x-4 checked:bg-primary-foreground");
    match label {
        Some(text_label) => base
            .child(
                text(text_label)
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
            )
            .into_element(),
        None => base.into_element(),
    }
}

/// A themed radio option row with indicator ring, title, and muted subtitle.
pub(crate) fn radio_option(
    selected: impl Fn() -> bool + Copy + 'static,
    title: &'static str,
    subtitle: &'static str,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .on_press(on_press)
        .role(Some(AccessibilityRole::Radio))
        .selected(selected)
        .accessibility_label(title)
        .w_full()
        .px_3()
        .py(px(10.0))
        .flex_row()
        .items_center()
        .gap_3()
        .rounded(radius)
        .border_1()
        .border_color(move || if selected() { theme().primary } else { theme().border })
        .bg(move || {
            if selected() {
                theme().accent
            } else {
                theme().background
            }
        })
        .hover(move |s| {
            s.bg(move || {
                if selected() {
                    theme().accent
                } else {
                    theme().muted
                }
            })
        })
        .active(|s| s.scale(0.99))
        .transition_transform()
        .duration(150.0)
        .focus_visible(focus_ring)
        .child(
            view()
                .w(px(16.0))
                .h(px(16.0))
                .flex_shrink_0()
                .rounded_full()
                .border_1()
                .border_color(move || if selected() { theme().primary } else { theme().input })
                .items_center()
                .justify_center()
                .child(when(selected, || {
                    view()
                        .w(px(8.0))
                        .h(px(8.0))
                        .rounded_full()
                        .bg(move || theme().primary)
                })),
        )
        .child(
            view()
                .flex_1()
                .gap(px(2.0))
                .child(
                    text(title)
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || {
                            if selected() {
                                theme().accent_foreground
                            } else {
                                theme().foreground
                            }
                        }),
                )
                .child(
                    text(subtitle)
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                ),
        )
}
