use gpui::App;
use gpui::Global;
use gpui::Pixels;
use gpui::Rgba;
use gpui::px;

use serde::Deserialize;
use serde::Serialize;

use crate::palette;

/// The user-selectable theme variant, persisted in settings.
#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemeVariant {
    #[default]
    Dark,
    Light,
}

impl ThemeVariant {
    /// Returns the opposite variant.
    pub fn toggled(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Dark,
        }
    }
}

/// Semantic colors and metrics for the app and its components, resolved for the active theme
/// variant.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TesseraTheme {
    /// The window background.
    pub background: Rgba,
    /// Subtle panel surfaces (footer, scrollbar tracks, row borders).
    pub surface: Rgba,
    /// Raised surfaces (buttons, popovers, lists, title bar).
    pub element: Rgba,
    /// Standard borders and hover fills.
    pub border: Rgba,
    /// Strong borders (inputs, scrollbar thumbs, switch tracks).
    pub border_strong: Rgba,
    /// Default text.
    pub text: Rgba,
    /// Secondary text.
    pub text_secondary: Rgba,
    /// Muted text.
    pub text_muted: Rgba,
    /// Warning text.
    pub warning: Rgba,
    /// Success text.
    pub success: Rgba,
    /// Informational text.
    pub info: Rgba,
    /// The primary accent color.
    pub primary: Rgba,
    /// The primary accent, emphasized against the background (hover states).
    pub primary_hover: Rgba,
    /// The primary accent, pressed/active states and filled marks.
    pub primary_active: Rgba,
    /// Text rendered on top of primary-filled surfaces.
    pub on_primary: Rgba,
    /// Full-window overlay scrim.
    pub scrim: Rgba,
    /// Destructive actions (the window close button, error notifications).
    pub danger: Rgba,
    /// Text rendered on top of danger-filled surfaces.
    pub on_danger: Rgba,
    /// The settings sidebar background.
    pub sidebar: Rgba,
    /// Active and hovered settings sidebar entries.
    pub sidebar_accent: Rgba,
    /// The settings sidebar's sub-entry rule.
    pub sidebar_border: Rgba,
    /// The inner dot of a slider thumb.
    pub slider_thumb: Rgba,
    /// Scrollbar thumbs at rest.
    pub scrollbar_thumb: Rgba,
    /// The dimming laid over the window behind a dialog.
    pub overlay: Rgba,
    /// The corner radius of controls.
    pub radius: Pixels,
    /// The corner radius of large surfaces (dialogs, notifications).
    pub radius_lg: Pixels,
}

impl Global for TesseraTheme {}

impl TesseraTheme {
    /// The dark theme, matching the porter-app palette.
    pub(crate) const fn dark() -> Self {
        Self {
            background: palette::BACKGROUND_COLOR_DEFAULT,
            surface: palette::BACKGROUND_COLOR_LIGHT_025,
            element: palette::BACKGROUND_COLOR_LIGHT_050,
            border: palette::BACKGROUND_COLOR_LIGHT_100,
            border_strong: palette::BACKGROUND_COLOR_LIGHT_150,
            text: palette::TEXT_COLOR_DEFAULT,
            text_secondary: palette::TEXT_COLOR_SECONDARY,
            text_muted: palette::TEXT_COLOR_MUTED,
            warning: palette::TEXT_COLOR_WARN,
            success: palette::TEXT_COLOR_SUCCESS,
            info: palette::TEXT_COLOR_INFO,
            primary: palette::PRIMARY_COLOR,
            primary_hover: palette::PRIMARY_COLOR_LIGHT_250,
            primary_active: palette::PRIMARY_COLOR_DARK_250,
            on_primary: palette::TEXT_COLOR_DEFAULT,
            scrim: palette::with_alpha(palette::BACKGROUND_COLOR_DEFAULT, 0.6),
            danger: palette::DANGER_COLOR,
            on_danger: palette::TEXT_COLOR_DEFAULT,
            sidebar: palette::from_rgb8(0x0A, 0x0A, 0x0A),
            sidebar_accent: palette::from_rgb8(0x26, 0x26, 0x26),
            sidebar_border: palette::from_rgb8(0x26, 0x26, 0x26),
            slider_thumb: palette::from_rgb8(0x0A, 0x0A, 0x0A),
            scrollbar_thumb: palette::from_rgb8(0x4C, 0x4C, 0x4C),
            overlay: palette::with_alpha(palette::from_rgb8(0x00, 0x00, 0x00), 0.2),
            radius: px(6.0),
            radius_lg: px(10.0),
        }
    }

    /// The light theme.
    pub(crate) const fn light() -> Self {
        Self {
            background: palette::LIGHT_BACKGROUND_COLOR_DEFAULT,
            surface: palette::LIGHT_BACKGROUND_COLOR_DARK_025,
            element: palette::LIGHT_BACKGROUND_COLOR_DARK_050,
            border: palette::LIGHT_BACKGROUND_COLOR_DARK_100,
            border_strong: palette::LIGHT_BACKGROUND_COLOR_DARK_150,
            text: palette::LIGHT_TEXT_COLOR_DEFAULT,
            text_secondary: palette::LIGHT_TEXT_COLOR_SECONDARY,
            text_muted: palette::LIGHT_TEXT_COLOR_MUTED,
            warning: palette::LIGHT_TEXT_COLOR_WARN,
            success: palette::LIGHT_TEXT_COLOR_SUCCESS,
            info: palette::LIGHT_TEXT_COLOR_INFO,
            primary: palette::PRIMARY_COLOR,
            primary_hover: palette::LIGHT_PRIMARY_COLOR_HOVER,
            primary_active: palette::LIGHT_PRIMARY_COLOR_ACTIVE,
            on_primary: palette::TEXT_COLOR_DEFAULT,
            scrim: palette::with_alpha(palette::LIGHT_BACKGROUND_COLOR_DEFAULT, 0.6),
            danger: palette::DANGER_COLOR,
            on_danger: palette::TEXT_COLOR_DEFAULT,
            sidebar: palette::from_rgb8(0xFA, 0xFA, 0xFA),
            sidebar_accent: palette::from_rgb8(0xE5, 0xE5, 0xE5),
            sidebar_border: palette::from_rgb8(0xE5, 0xE5, 0xE5),
            slider_thumb: palette::from_rgb8(0xFF, 0xFF, 0xFF),
            scrollbar_thumb: palette::from_rgb8(0xAB, 0xAB, 0xAB),
            overlay: palette::with_alpha(palette::from_rgb8(0x00, 0x00, 0x00), 0.05),
            radius: px(6.0),
            radius_lg: px(10.0),
        }
    }

    /// The theme for the given variant.
    pub(crate) const fn for_variant(variant: ThemeVariant) -> Self {
        match variant {
            ThemeVariant::Dark => Self::dark(),
            ThemeVariant::Light => Self::light(),
        }
    }
}

/// Access to the active [`TesseraTheme`] from any gpui context.
pub(crate) trait ActiveTesseraTheme {
    fn tessera_theme(&self) -> &TesseraTheme;
}

impl ActiveTesseraTheme for App {
    fn tessera_theme(&self) -> &TesseraTheme {
        self.global::<TesseraTheme>()
    }
}

/// Resolves a cell color supplied by asset managers or column definitions.
///
/// Managers author colors against the dark palette constants; text-tier
/// colors are remapped to the active theme so light mode stays readable,
/// while accent colors (asset types/statuses) pass through unchanged.
pub(crate) fn resolve_cell_color(color: Rgba, theme: &TesseraTheme) -> Rgba {
    if color == palette::TEXT_COLOR_DEFAULT {
        theme.text
    } else if color == palette::TEXT_COLOR_SECONDARY {
        theme.text_secondary
    } else if color == palette::TEXT_COLOR_MUTED {
        theme.text_muted
    } else {
        color
    }
}

/// The monospace font family of the app, for the hex, text and statistics views: the
/// platform's own monospace face.
#[cfg(target_os = "windows")]
pub(crate) const MONO_FONT_FAMILY: &str = "Consolas";
/// The monospace font family of the app, for the hex, text and statistics views: the
/// platform's own monospace face.
#[cfg(target_os = "macos")]
pub(crate) const MONO_FONT_FAMILY: &str = "Menlo";
/// The monospace font family of the app, for the hex, text and statistics views: the
/// platform's own monospace face.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) const MONO_FONT_FAMILY: &str = "DejaVu Sans Mono";

/// Applies the tessera theme for the given variant.
pub(crate) fn apply(variant: ThemeVariant, cx: &mut App) {
    cx.set_global(TesseraTheme::for_variant(variant));
    cx.refresh_windows();
}
