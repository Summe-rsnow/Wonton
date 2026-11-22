//! Wonton-specific brand mark, tray icon, and Antigravity launcher SVG icons,
//! plus re-exports of shared Bento stroke icons.

use std::sync::OnceLock;

use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

pub(crate) use bento::ui::icons::{
    svg_icon, ACTIVITY, CLOSE, DOWNLOAD, GRIP, INFO, PLUS, REFRESH, SAVE, TERMINAL, TRASH, UPLOAD,
};

pub(crate) const BRAND_MARK: &str = include_str!("../../assets/logo-mark.svg");
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/tray-icon.png");

pub(crate) const ROCKET: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="M4.5 16.5c-1.5 1.26-2 5-2 5s3.74-.5 5-2c.71-.84.7-2.13-.09-2.91a2.18 2.18 0 0 0-2.91-.09z"/><path d="M12 15l-3-3a22 22 0 0 1 2-3.95A12.88 12.88 0 0 1 22 2c0 2.72-.78 7.5-6 11a22.35 22.35 0 0 1-4 2z"/><path d="M9 12H4s.55-3.03 2-4c1.62-1.08 5 0 5 0"/><path d="M12 15v5s3.03-.55 4-2c1.08-1.62 0-5 0-5"/></svg>"##;

pub(crate) const CODE_IDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"><path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/></svg>"##;

/// Renders the full-color Wonton brand mark in its authored Gemini gradients.
pub(crate) fn brand_mark(size: f32) -> impl IntoElement {
    svg(ImageSrc::svg_markup(BRAND_MARK)).size(px(size))
}

/// Returns the cached full-color 64x64 data URI for the system tray icon.
pub(crate) fn tray_icon_src() -> ImageSrc {
    static DATA_URI: OnceLock<ImageSrc> = OnceLock::new();
    DATA_URI
        .get_or_init(|| bento::ui::png_data_uri(TRAY_ICON_PNG))
        .clone()
}
