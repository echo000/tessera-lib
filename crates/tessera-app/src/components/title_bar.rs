use gpui::App;
use gpui::Context;
use gpui::EventEmitter;
use gpui::FontWeight;
use gpui::Hsla;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Render;
use gpui::RenderOnce;
use gpui::Rgba;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::TitlebarOptions;
use gpui::Window;
use gpui::WindowControlArea;
use gpui::div;
use gpui::linear_color_stop;
use gpui::linear_gradient;
use gpui::point;
use gpui::prelude::FluentBuilder;
use gpui::px;

use crate::components::Button;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Selectable;
use crate::components::Sizable;
use crate::components::h_flex;
use crate::theme::ActiveTesseraTheme;

/// The height of the window title bar.
pub(crate) const TITLE_BAR_HEIGHT: Pixels = px(34.0);

/// Window options for a title bar drawn by the app.
pub(crate) fn title_bar_options(title: impl Into<SharedString>) -> TitlebarOptions {
    TitlebarOptions {
        title: Some(title.into()),
        appears_transparent: true,
        traffic_light_position: Some(point(px(9.0), px(9.0))),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum TitleBarEvent {
    ToggleSettings,
}

pub(crate) struct AppTitleBar {
    title: SharedString,
    version: SharedString,
    settings_open: bool,
    /// Set by a press on the bar, so the next drag moves the window.
    should_move: bool,
}

impl EventEmitter<TitleBarEvent> for AppTitleBar {}

impl AppTitleBar {
    pub(crate) fn new(
        title: impl Into<SharedString>,
        version: impl Into<SharedString>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            title: title.into(),
            version: version.into(),
            settings_open: false,
            should_move: false,
        }
    }

    pub(crate) fn sync_settings(&mut self, settings_open: bool) {
        self.settings_open = settings_open;
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let title = self.title.clone();
        let version = self.version.clone();

        let left = div()
            .id("title-bar-left")
            .flex()
            .items_center()
            .gap_2()
            .h_full()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .child(title.to_uppercase()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme.text_secondary)
                    .child(format!("v{version}")),
            );

        let right = div()
            .flex()
            .items_center()
            .justify_end()
            .pr_2()
            .gap_1()
            .h_full()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("settings")
                    .small()
                    .ghost()
                    .selected(self.settings_open)
                    .icon(IconName::Settings)
                    .tooltip("Settings")
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(TitleBarEvent::ToggleSettings);
                    })),
            );

        let is_windows = cfg!(target_os = "windows");
        let is_macos = cfg!(target_os = "macos");

        div().flex_shrink_0().child(
            div()
                .id("title-bar")
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .h(TITLE_BAR_HEIGHT)
                .pl(if is_macos { px(80.0) } else { px(12.0) })
                .border_b_1()
                .border_color(theme.border)
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(mix(theme.element, theme.background, 0.55), 0.0),
                    linear_color_stop(theme.element, 1.0),
                ))
                .when(!is_windows, |this| {
                    this.on_click(move |event, window, _| {
                        if event.click_count() != 2 {
                            return;
                        }

                        if is_macos {
                            window.titlebar_double_click();
                        } else {
                            window.zoom_window();
                        }
                    })
                })
                .on_mouse_down_out(cx.listener(|this, _, _, _| this.should_move = false))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = true),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| this.should_move = false),
                )
                .on_mouse_move(cx.listener(|this, _, window, _| {
                    if this.should_move {
                        this.should_move = false;
                        window.start_window_move();
                    }
                }))
                .child(
                    h_flex()
                        .id("bar")
                        .h_full()
                        .justify_between()
                        .flex_shrink_0()
                        .flex_1()
                        .window_control_area(WindowControlArea::Drag)
                        .child(left)
                        .child(right),
                )
                .when(!is_macos, |this| this.child(WindowControls)),
        )
    }
}

/// Mixes two colors, `amount` of the way from `to` towards `from`.
fn mix(from: Rgba, to: Rgba, amount: f32) -> Hsla {
    Rgba {
        r: from.r * amount + to.r * (1.0 - amount),
        g: from.g * amount + to.g * (1.0 - amount),
        b: from.b * amount + to.b * (1.0 - amount),
        a: from.a * amount + to.a * (1.0 - amount),
    }
    .into()
}

/// The minimize, maximize/restore and close buttons.
#[derive(IntoElement)]
struct WindowControls;

impl RenderOnce for WindowControls {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let controls = window.window_controls();

        h_flex()
            .id("window-controls")
            .flex_shrink_0()
            .h_full()
            .when(controls.minimize, |this| {
                this.child(ControlButton::Minimize)
            })
            .when(controls.maximize, |this| {
                this.child(if window.is_maximized() {
                    ControlButton::Restore
                } else {
                    ControlButton::Maximize
                })
            })
            .child(ControlButton::Close)
    }
}

/// A single window control button.
#[derive(IntoElement, Clone, Copy, PartialEq, Eq)]
enum ControlButton {
    Minimize,
    Restore,
    Maximize,
    Close,
}

impl RenderOnce for ControlButton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let close = self == ControlButton::Close;

        let (id, icon, area) = match self {
            ControlButton::Minimize => {
                ("minimize", IconName::WindowMinimize, WindowControlArea::Min)
            }
            ControlButton::Restore => ("restore", IconName::WindowRestore, WindowControlArea::Max),
            ControlButton::Maximize => {
                ("maximize", IconName::WindowMaximize, WindowControlArea::Max)
            }
            ControlButton::Close => ("close", IconName::WindowClose, WindowControlArea::Close),
        };

        let (hover_bg, hover_fg, active_bg) = if close {
            (theme.danger, theme.on_danger, theme.danger)
        } else {
            (theme.border, theme.text, theme.border_strong)
        };

        div()
            .id(id)
            .flex()
            .w(TITLE_BAR_HEIGHT)
            .h_full()
            .flex_shrink_0()
            .justify_center()
            .items_center()
            .text_color(theme.text)
            .hover(move |style| style.bg(hover_bg).text_color(hover_fg))
            .active(move |style| style.bg(active_bg).text_color(hover_fg))
            .when(cfg!(target_os = "windows"), |this| {
                this.window_control_area(area)
            })
            .when(!cfg!(target_os = "windows"), |this| {
                this.on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();

                    match self {
                        ControlButton::Minimize => window.minimize_window(),
                        ControlButton::Restore | ControlButton::Maximize => window.zoom_window(),
                        ControlButton::Close => window.remove_window(),
                    }
                })
            })
            .child(Icon::new(icon).small())
    }
}
