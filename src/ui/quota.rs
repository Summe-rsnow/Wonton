//! One quota pool on a single horizontal row (`5H` and `周` side by side, or `周` only for Argon / Free).

use nsh_base::theme;
use nsh_core::prelude::*;

use super::ValueTone;
use crate::format;
use crate::google::ModelQuota;

/// Renders one quota group (`Gemini`, `Gemini Argon`, `Claude … / GPT`) with
/// its 5H and/or weekly window bars aligned on one compact row.
pub(crate) fn quota_row(
    model: impl Fn() -> ModelQuota + Copy + 'static,
    now_ms: impl Fn() -> i64 + Copy + 'static,
) -> impl IntoElement {
    let show_5h = move || !model().is_weekly_only;
    let show_weekly = move || {
        let m = model();
        m.is_weekly_only || m.weekly_remaining.is_some()
    };
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(22.0))
        .child(
            text(move || SharedString::from(model().name))
                .w(px(126.0))
                .number_of_lines(1)
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || theme().foreground),
        )
        .child(
            view()
                .flex_1()
                .flex_row()
                .items_center()
                .gap_3()
                .child(when(show_5h, move || {
                    quota_window_segment(
                        "5H",
                        move || model().remaining,
                        move || model().reset.unwrap_or_default(),
                        now_ms,
                    )
                }))
                .child(when(show_weekly, move || {
                    quota_window_segment(
                        "周",
                        move || {
                            let m = model();
                            m.weekly_remaining.unwrap_or(m.remaining)
                        },
                        move || {
                            let m = model();
                            if m.is_weekly_only {
                                m.weekly_reset.or(m.reset).unwrap_or_default()
                            } else {
                                m.weekly_reset.unwrap_or_default()
                            }
                        },
                        now_ms,
                    )
                })),
        )
}

fn quota_window_segment(
    window_label: &'static str,
    remaining: impl Fn() -> f32 + Copy + 'static,
    reset: impl Fn() -> String + Copy + 'static,
    now_ms: impl Fn() -> i64 + Copy + 'static,
) -> impl IntoElement {
    let tone = move || ValueTone::of(remaining());
    let percentage = move || format::percent(remaining());
    let reset_text = move || {
        let text = reset();
        format::reset_label(Some(text.as_str()).filter(|text| !text.is_empty()), now_ms())
    };
    view()
        .flex_1()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            text(window_label)
                .w(px(18.0))
                .text_xs()
                .text_color(move || theme().muted_foreground),
        )
        .child(
            view()
                .flex_1()
                .h(px(5.0))
                .flex_row()
                .rounded_full()
                .overflow_hidden()
                .bg(move || theme().muted)
                .child(
                    view()
                        .h(px(5.0))
                        .flex_grow(move || remaining().clamp(0.0, 1.0))
                        .bg(move || tone().color())
                        .transition_all()
                        .duration(240.0),
                )
                .child(
                    view()
                        .h(px(5.0))
                        .flex_grow(move || 1.0 - remaining().clamp(0.0, 1.0)),
                ),
        )
        .child(
            text(move || format!("{}%", percentage()))
                .w(px(36.0))
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .tabular_nums()
                .text_color(move || tone().color()),
        )
        .child(
            text(reset_text)
                .w(px(74.0))
                .text_xs()
                .number_of_lines(1)
                .text_color(move || theme().muted_foreground),
        )
}
