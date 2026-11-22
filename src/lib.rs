//! Wonton: Antigravity accounts and their quotas on one screen, and the proxy
//! the client starts behind. `nsh.json` `"root"` names [`App`].

mod add_dialog;
mod dialogs;
mod format;
mod google;
mod launch;
mod localdb;
mod shell;
mod store;
mod ui;

use bento::AppMeta;
use nsh_base::theme;
use nsh_core::prelude::*;

pub(crate) const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const APP_META: AppMeta = AppMeta {
    name: "Wonton",
    version: APP_VERSION,
    tagline: "轻量、原生的 Antigravity 多账号与额度管理工具",
    github_repo: "Summe-rsnow/Wonton",
    license: "Apache-2.0",
    brand_mark: |size| ui::icons::brand_mark(size).into_element(),
};

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("wonton/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The app's root: the whole surface, on the theme's background. The saved
/// accounts are read once, before the screen is built.
#[component]
pub fn App() -> impl IntoElement {
    bento::shell::init_desktop_window(720.0, 560.0, 48.0);

    store::load();
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .p_safe_area()
        .child(shell::screen())
}
