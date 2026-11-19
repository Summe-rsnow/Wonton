//! The app's styled layer over `nsh_base`.
//!
//! Every control the screens use lives here: features compose these and never
//! style a control themselves. Each takes what varies by meaning (a variant, a
//! state, content) and takes its layout from the caller.

mod button;
pub(crate) mod icons;
mod input;
mod progress;
mod quota;
mod surface;

pub(crate) use button::{
    action, action_disabled, hint_tooltip, icon_action, icon_action_disabled,
    icon_action_with_tip, segmented_icon, ButtonKind, IconKind,
};
pub(crate) use input::{checkbox, radio_option, text_field, toggle_switch};
pub(crate) use progress::progress_bar;
pub(crate) use quota::quota_row;
pub(crate) use surface::{card, divider, empty_state, notice_toast, status_badge, tier_badge};

use nsh_base::theme;
use nsh_core::element::StyleRefinement;
use nsh_core::prelude::*;

/// A theme color by name, with a fallback that is visibly wrong in review
/// rather than a panic at run time.
pub(crate) fn color(name: &'static str) -> AdaptiveColor {
    theme().color(name).unwrap_or_else(|| theme().foreground)
}

/// The one radius the app uses for controls and surfaces.
pub(crate) fn radius() -> Length {
    theme().radius.into()
}

/// A quota tone: which semantic color a value takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueTone {
    Ok,
    Warn,
    Bad,
}

impl ValueTone {
    /// Above half is healthy, a fifth is the warning line, below it is low.
    pub(crate) fn of(remaining: f32) -> Self {
        if remaining > 0.5 {
            Self::Ok
        } else if remaining >= 0.2 {
            Self::Warn
        } else {
            Self::Bad
        }
    }

    pub(crate) fn color(self) -> AdaptiveColor {
        match self {
            Self::Ok => color("success"),
            Self::Warn => color("warning"),
            Self::Bad => theme().destructive,
        }
    }
}

/// The focus ring every interactive control shows from the keyboard, drawn
/// outside the border so a translucent fill stays intact.
pub(crate) fn focus_ring(s: StyleRefinement) -> StyleRefinement {
    s.outline(px(2.0)).outline_color(move || theme().ring)
}
