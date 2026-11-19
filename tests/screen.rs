//! The screen's flows, with the steps a user takes: the dialog opens on its
//! first tab, another tab can be chosen, it closes, and saving the proxy
//! reaches the store.

use std::time::Duration;

use nsh_core::platform::{Idiom, Os};
use nsh_core::prelude::{px, size};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

const GONE: Duration = Duration::from_secs(5);

fn start() -> Headless {
    let mut config = EngineConfig::new(Root::of::<nsh_app_wonton::App>());
    config.viewport = size(px(820.0), px(640.0));
    config.environment.os = Os::Windows;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("还没有账号").unwrap();
    app
}

#[test]
fn the_add_dialog_opens_on_oauth_and_closes() {
    let mut app = start();
    app.press("role:button[添加账号…]").unwrap();
    app.wait_for("网页授权").unwrap();
    app.press("role:tab[手动 Token]").unwrap();
    app.wait_for_gone("打开授权页面", GONE).unwrap();
    app.press("role:button[关闭]").unwrap();
    app.wait_for_gone("网页授权", GONE).unwrap();
    assert!(app.screen().shows("还没有账号"));
}

#[test]
fn saving_the_proxy_says_so() {
    let mut app = start();
    app.press("role:button[保存]").unwrap();
    app.wait_for("代理已保存").unwrap();
    app.press("role:button[关闭提示]").unwrap();
    app.wait_for_gone("代理已保存", GONE).unwrap();
}

#[test]
fn exporting_without_accounts_shows_a_notice() {
    let mut app = start();
    assert!(app.screen().matches("role:button[导入]").unwrap());
    app.press("role:button[导出]").unwrap();
    app.wait_for("没有可导出的账号").unwrap();
}

#[test]
fn theme_modes_and_proxy_toggle_work() {
    let mut app = start();
    app.press("role:button[深色模式]").unwrap();
    app.press("role:button[浅色模式]").unwrap();
    app.press("role:button[跟随系统]").unwrap();

    app.press("role:switch[启用 HTTP 代理]").unwrap();
    app.wait_for("已关闭 HTTP 代理（将使用直连）").unwrap();
    app.press("role:switch[启用 HTTP 代理]").unwrap();
    app.wait_for("已启用 HTTP 代理").unwrap();
}

#[test]
fn close_prompt_can_cancel_and_minimize_to_tray_without_asking_again() {
    let mut app = start();

    // First close press shows confirmation prompt; Cancel leaves window open.
    app.press_window_close("main").unwrap();
    app.wait_for("关闭 Wonton").unwrap();
    app.press("role:radio[退出应用]").unwrap();
    app.press("role:button[取消]").unwrap();
    app.wait_for_gone("关闭 Wonton", GONE).unwrap();
    assert!(app.window_state("main").unwrap().is_some_and(|s| s.visible));

    // Second close press: pick Tray + "不再提示", confirm -> hides main window.
    app.press_window_close("main").unwrap();
    app.wait_for("关闭 Wonton").unwrap();
    app.press("role:radio[最小化到系统托盘]").unwrap();
    app.press("role:checkbox[不再提示]").unwrap();
    app.press("role:button[确定]").unwrap();
    app.wait_for_gone("关闭 Wonton", GONE).unwrap();
    assert!(app.window_state("main").unwrap().is_some_and(|s| !s.visible));

    // Subsequent close presses honor the saved preference without re-opening the prompt.
    app.press_window_close("main").unwrap();
    assert!(!app.screen().shows("关闭 Wonton"));
}
