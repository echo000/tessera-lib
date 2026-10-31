use gpui::BoxShadow;
use gpui::Div;
use gpui::Hsla;
use gpui::Pixels;
use gpui::Styled;
use gpui::div;
use gpui::point;

/// A horizontal flex container with its children centered vertically.
pub(crate) fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// A vertical flex container.
pub(crate) fn v_flex() -> Div {
    div().flex().flex_col()
}

/// A drop shadow, in CSS `box-shadow` order.
pub(crate) fn box_shadow(
    x: Pixels,
    y: Pixels,
    blur: Pixels,
    spread: Pixels,
    color: impl Into<Hsla>,
) -> BoxShadow {
    BoxShadow {
        color: color.into(),
        offset: point(x, y),
        blur_radius: blur,
        spread_radius: spread,
    }
}
