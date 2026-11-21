//! The screen's flows, with the steps a user takes: version badge, toolbar
//! layout, add-account dialog (without Enterprise/GCP), about & update dialog,
//! proxy settings, theme modes, and close behavior.

use std::time::Duration;

use nsh_core::platform::{Idiom, Os};
use nsh_core::prelude::{px, size};
use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

const GONE: Duration = Duration::from_secs(5);

fn start() -> Headless {
    let mut config = EngineConfig::new(Root::of::<nsh_app_wonton::App>());
    config.viewport = size(px(860.0), px(640.0));
    config.environment.os = Os::Windows;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).unwrap();
    app.wait_for("还没有账号").unwrap();
    app
}

#[test]
fn shows_version_and_updated_toolbar_controls() {
    let app = start();
    assert!(app.screen().shows("v1.1.2"));
    for label in [
        "关于",
        "测试",
        "保存",
        "导入",
        "导出",
        "全部刷新",
        "删除",
        "添加账号…",
        "启动反重力",
        "启动 Antigravity IDE",
        "启动 Antigravity CLI",
    ] {
        assert!(
            app.screen().matches(&format!("role:button[{label}]")).unwrap(),
            "missing toolbar button: {label}"
        );
    }
}

#[test]
fn the_add_dialog_opens_on_oauth_without_enterprise_and_closes() {
    let mut app = start();
    app.press("role:button[添加账号…]").unwrap();
    app.wait_for("网页授权").unwrap();
    assert!(!app.screen().shows("使用企业版（GCP）客户端"));
    assert!(!app.screen().matches("role:button[导入账号 JSON]").unwrap());
    app.press("role:tab[手动 Token]").unwrap();
    app.wait_for_gone("打开授权页面", GONE).unwrap();
    assert!(app.screen().matches("role:button[导入账号 JSON]").unwrap());
    app.press("role:tab[本机导入]").unwrap();
    app.wait_for("从本机导入").unwrap();
    assert!(!app.screen().matches("role:button[导入账号 JSON]").unwrap());
    app.press("role:button[关闭]").unwrap();
    app.wait_for_gone("网页授权", GONE).unwrap();
    assert!(app.screen().shows("还没有账号"));
}

#[test]
fn the_about_dialog_shows_version_links_and_update_check() {
    let mut app = start();
    app.press("role:button[关于]").unwrap();
    app.wait_for("版本更新").unwrap();
    assert!(app.screen().shows("v1.1.2"));
    assert!(app.screen().shows("Apache-2.0"));
    assert!(app.screen().matches("role:button[检测更新]").unwrap());
    assert!(app.screen().matches("role:link[Summe-rsnow/Wonton]").unwrap());
    assert!(app
        .screen()
        .matches("role:link[打开发布页 (GitHub Releases)]")
        .unwrap());
    app.press("role:button[关闭]").unwrap();
    app.wait_for_gone("版本更新", GONE).unwrap();
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
