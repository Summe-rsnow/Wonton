//! Wonton-specific brand mark and tray icon, plus re-exports of shared Bento stroke icons.

use std::sync::OnceLock;

use nsh_core::element::ImageSrc;
use nsh_core::prelude::*;

pub(crate) use bento::ui::icons::{
    svg_icon, ACTIVITY, CLOSE, CODE_IDE, DOWNLOAD, GRIP, PLUS, REFRESH, ROCKET, SAVE, TERMINAL,
    TRASH, UPLOAD,
};

pub(crate) const BRAND_MARK: &str = include_str!("../../assets/logo-mark.svg");
const TRAY_ICON_PNG: &[u8] = include_bytes!("../../assets/tray-icon.png");

/// Renders the full-color Wonton brand mark in its authored gradients.
pub(crate) fn brand_mark(size: f32) -> impl IntoElement {
    bento::ui::icons::brand_mark_svg(BRAND_MARK, size)
}

/// Returns the cached full-color 64x64 data URI for the system tray icon.
pub(crate) fn tray_icon_src() -> ImageSrc {
    static DATA_URI: OnceLock<ImageSrc> = OnceLock::new();
    DATA_URI
        .get_or_init(|| bento::ui::png_data_uri(TRAY_ICON_PNG))
        .clone()
}
