use std::time::Duration;

use gpui::Animation;
use gpui::AnimationExt;
use gpui::App;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::RenderOnce;
use gpui::Styled;
use gpui::Transformation;
use gpui::Window;
use gpui::div;
use gpui::ease_in_out;
use gpui::percentage;
use gpui::px;
use gpui::relative;

use crate::components::Icon;
use crate::components::IconName;
use crate::components::Sizable;
use crate::components::Size;
use crate::palette;
use crate::theme::ActiveTesseraTheme;

/// A spinning loading indicator.
#[derive(IntoElement)]
pub(crate) struct Spinner {
    size: Size,
}

impl Spinner {
    /// Constructs a medium spinner.
    pub(crate) fn new() -> Self {
        Self { size: Size::Medium }
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Spinner {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().child(
            Icon::new(IconName::Loader)
                .with_size(self.size)
                .with_animation(
                    "spinner",
                    Animation::new(Duration::from_secs_f64(0.8))
                        .repeat()
                        .with_easing(ease_in_out),
                    |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
                ),
        )
    }
}

/// A horizontal progress bar.
#[derive(IntoElement)]
pub(crate) struct Progress {
    /// The progress, from 0 to 100.
    value: f32,
}

impl Progress {
    /// Constructs a progress bar at the given percentage.
    pub(crate) fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 100.0),
        }
    }
}

impl RenderOnce for Progress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.tessera_theme();

        div()
            .relative()
            .w_full()
            .h(px(8.0))
            .rounded(px(4.0))
            .bg(palette::with_alpha(theme.primary, 0.2))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .h_full()
                    .w(relative(self.value / 100.0))
                    .rounded(px(4.0))
                    .bg(theme.primary),
            )
    }
}
