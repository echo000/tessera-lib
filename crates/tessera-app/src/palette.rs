use gpui::Rgba;

/// Blends a color channel to the target with the given factor.
const fn blend_channel(value: f32, target: f32, factor: f32) -> f32 {
    value * (1.0 - factor) + target * factor
}

/// Constructs a color from 8bit rgb components.
pub const fn from_rgb8(r: u8, g: u8, b: u8) -> Rgba {
    Rgba {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: 1.0,
    }
}

/// Returns the color with the given alpha applied.
pub const fn with_alpha(color: Rgba, alpha: f32) -> Rgba {
    Rgba {
        r: color.r,
        g: color.g,
        b: color.b,
        a: alpha,
    }
}

/// Lightens a color by the given factor.
pub const fn lighten(color: Rgba, factor: f32) -> Rgba {
    let factor = factor.clamp(0.0, 1.0);

    Rgba {
        r: blend_channel(color.r, 1.0, factor),
        g: blend_channel(color.g, 1.0, factor),
        b: blend_channel(color.b, 1.0, factor),
        a: color.a,
    }
}

/// Darkens a color by the given factor.
pub const fn darken(color: Rgba, factor: f32) -> Rgba {
    let factor = factor.clamp(0.0, 1.0);

    Rgba {
        r: blend_channel(color.r, 0.0, factor),
        g: blend_channel(color.g, 0.0, factor),
        b: blend_channel(color.b, 0.0, factor),
        a: color.a,
    }
}

/// Primary color.
pub const PRIMARY_COLOR: Rgba = from_rgb8(0x27, 0x9B, 0xD4);
/// Light 0.25 primary color.
pub const PRIMARY_COLOR_LIGHT_250: Rgba = lighten(PRIMARY_COLOR, 0.25);
/// Dark 0.25 primary color.
pub const PRIMARY_COLOR_DARK_250: Rgba = darken(PRIMARY_COLOR, 0.25);

/// Default background color.
pub const BACKGROUND_COLOR_DEFAULT: Rgba = from_rgb8(0x11, 0x11, 0x11);
/// Light 0.025 background color.
pub const BACKGROUND_COLOR_LIGHT_025: Rgba = lighten(BACKGROUND_COLOR_DEFAULT, 0.025);
/// Light 0.050 background color.
pub const BACKGROUND_COLOR_LIGHT_050: Rgba = lighten(BACKGROUND_COLOR_DEFAULT, 0.050);
/// Light 0.100 background color.
pub const BACKGROUND_COLOR_LIGHT_100: Rgba = lighten(BACKGROUND_COLOR_DEFAULT, 0.100);
/// Light 0.150 background color.
pub const BACKGROUND_COLOR_LIGHT_150: Rgba = lighten(BACKGROUND_COLOR_DEFAULT, 0.150);

/// Default text color.
pub const TEXT_COLOR_DEFAULT: Rgba = from_rgb8(0xFF, 0xFF, 0xFF);
/// Secondary text color.
pub const TEXT_COLOR_SECONDARY: Rgba = darken(TEXT_COLOR_DEFAULT, 0.25);
/// Muted text color.
pub const TEXT_COLOR_MUTED: Rgba = darken(TEXT_COLOR_DEFAULT, 0.5);
/// Porter text color.
pub const TEXT_COLOR_PORTER: Rgba = from_rgb8(0xEC, 0x34, 0xCA);
/// Tessera text color.
pub const TEXT_COLOR_TESSERA: Rgba = from_rgb8(0x00, 0xFF, 0xFF);
/// Info text color.
pub const TEXT_COLOR_INFO: Rgba = PRIMARY_COLOR;
/// Warn text color.
pub const TEXT_COLOR_WARN: Rgba = from_rgb8(0xD4, 0xAF, 0x37);
/// Success text color.
pub const TEXT_COLOR_SUCCESS: Rgba = from_rgb8(0x23, 0xCE, 0x6B);
/// Link text color.
pub const TEXT_COLOR_LINK: Rgba = TEXT_COLOR_INFO;
/// Link hover text color.
pub const TEXT_COLOR_LINK_HOVER: Rgba = lighten(TEXT_COLOR_LINK, 0.3);
/// Disabled text color.
pub const TEXT_COLOR_DISABLED: Rgba = from_rgb8(0x2C, 0x2C, 0x2C);
/// Destructive action color.
pub const DANGER_COLOR: Rgba = from_rgb8(0xEF, 0x44, 0x44);

/// Light 0.25 hover primary color, emphasized against light backgrounds.
pub const LIGHT_PRIMARY_COLOR_HOVER: Rgba = darken(PRIMARY_COLOR, 0.15);
/// Light 0.30 active primary color for light backgrounds.
pub const LIGHT_PRIMARY_COLOR_ACTIVE: Rgba = darken(PRIMARY_COLOR, 0.3);

/// Default background color for the light theme.
pub const LIGHT_BACKGROUND_COLOR_DEFAULT: Rgba = from_rgb8(0xF7, 0xF7, 0xF8);
/// Dark 0.030 light-theme background color.
pub const LIGHT_BACKGROUND_COLOR_DARK_025: Rgba = darken(LIGHT_BACKGROUND_COLOR_DEFAULT, 0.030);
/// Dark 0.055 light-theme background color.
pub const LIGHT_BACKGROUND_COLOR_DARK_050: Rgba = darken(LIGHT_BACKGROUND_COLOR_DEFAULT, 0.055);
/// Dark 0.105 light-theme background color.
pub const LIGHT_BACKGROUND_COLOR_DARK_100: Rgba = darken(LIGHT_BACKGROUND_COLOR_DEFAULT, 0.105);
/// Dark 0.165 light-theme background color.
pub const LIGHT_BACKGROUND_COLOR_DARK_150: Rgba = darken(LIGHT_BACKGROUND_COLOR_DEFAULT, 0.165);

/// Default text color for the light theme.
pub const LIGHT_TEXT_COLOR_DEFAULT: Rgba = from_rgb8(0x1B, 0x1D, 0x21);
/// Secondary text color for the light theme.
pub const LIGHT_TEXT_COLOR_SECONDARY: Rgba = lighten(LIGHT_TEXT_COLOR_DEFAULT, 0.3);
/// Muted text color for the light theme.
pub const LIGHT_TEXT_COLOR_MUTED: Rgba = lighten(LIGHT_TEXT_COLOR_DEFAULT, 0.5);
/// Warn text color for the light theme.
pub const LIGHT_TEXT_COLOR_WARN: Rgba = from_rgb8(0xA1, 0x62, 0x07);
/// Success text color for the light theme.
pub const LIGHT_TEXT_COLOR_SUCCESS: Rgba = from_rgb8(0x1A, 0x7F, 0x42);
/// Info text color for the light theme.
pub const LIGHT_TEXT_COLOR_INFO: Rgba = darken(PRIMARY_COLOR, 0.15);

/// Asset status loaded color.
pub const ASSET_STATUS_LOADED: Rgba = from_rgb8(0x23, 0xCE, 0x6B);
/// Asset status exported color.
pub const ASSET_STATUS_EXPORTED: Rgba = from_rgb8(0x21, 0xB8, 0xEB);
/// Asset status error color.
pub const ASSET_STATUS_ERROR: Rgba = from_rgb8(0xD4, 0xAF, 0x37);
/// Asset status placeholder color.
pub const ASSET_STATUS_PLACEHOLDER: Rgba = from_rgb8(0xEC, 0x34, 0xCA);
/// Asset status exporting color.
pub const ASSET_STATUS_EXPORTING: Rgba = from_rgb8(0x90, 0x7A, 0xD6);
/// Asset status not supported color.
pub const ASSET_STATUS_NOT_SUPPORTED: Rgba = from_rgb8(0xF1, 0xA3, 0x8B);

/// Asset type image color.
pub const ASSET_TYPE_IMAGE: Rgba = from_rgb8(0xCA, 0x61, 0xC3);
/// Asset type model color.
pub const ASSET_TYPE_MODEL: Rgba = from_rgb8(0x00, 0x9D, 0xDC);
/// Asset type material color.
pub const ASSET_TYPE_MATERIAL: Rgba = from_rgb8(0x1B, 0x99, 0x8B);
/// Asset type animation color.
pub const ASSET_TYPE_ANIMATION: Rgba = from_rgb8(0xDB, 0x50, 0x4A);
/// Asset type sound color.
pub const ASSET_TYPE_SOUND: Rgba = from_rgb8(0xD8, 0x1E, 0x5B);
/// Asset type raw file color.
pub const ASSET_TYPE_RAW_FILE: Rgba = from_rgb8(0xFF, 0xFF, 0x00);
/// Asset type map color.
pub const ASSET_TYPE_MAP: Rgba = from_rgb8(0xB0, 0x00, 0xB5);
pub const ASSET_TYPE_WORLD: Rgba = from_rgb8(0x7D, 0x5C, 0xFF);
