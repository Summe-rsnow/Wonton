//! Wonton: Antigravity accounts and their quotas on one screen, and the proxy
//! the client starts behind. `nsh.json` `"root"` names [`App`].

mod add_dialog;
mod format;
mod google;
mod launch;
mod localdb;
mod shell;
mod store;
mod ui;

use nsh_base::theme;
use nsh_core::desktop::{TitleBarOptions, TitleBarStyle, WindowAttributes};
use nsh_core::element::desktop::current_window;
use nsh_core::prelude::*;

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("wonton/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The app's root: the whole surface, on the theme's background. The saved
/// accounts are read once, before the screen is built.
#[component]
pub fn App() -> impl IntoElement {
    let mut title_bar = TitleBarOptions::default();
    title_bar.style = TitleBarStyle::Hidden;
    title_bar.height = Some(px(48.0));
    let mut attrs = WindowAttributes::default();
    attrs.title_bar = Some(title_bar);
    attrs.min_size = Some(Some(size(px(720.0), px(560.0))));
    attrs.close_on_press = Some(false);
    current_window().set(attrs);

    store::load();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .p_safe_area()
        .child(shell::screen())
}
