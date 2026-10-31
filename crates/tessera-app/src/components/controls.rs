use gpui::App;
use gpui::Bounds;
use gpui::ClickEvent;
use gpui::Component;
use gpui::Corner;
use gpui::ElementId;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::RenderOnce;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::Window;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;
use gpui::rems;

use crate::components::Button;
use crate::components::Disableable;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Popover;
use crate::components::Selectable;
use crate::components::h_flex;
use crate::components::popover_style;
use crate::components::v_flex;
use crate::palette;
use crate::theme::ActiveTesseraTheme;

/// A pill shaped on/off switch.
pub(crate) struct Switch<F> {
    id: ElementId,
    checked: bool,
    disabled: bool,
    on_change: F,
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> Switch<F> {
    /// Constructs a switch; `on_change` receives the toggled value.
    pub(crate) fn new(id: impl Into<ElementId>, checked: bool, on_change: F) -> Self {
        Self {
            id: id.into(),
            checked,
            disabled: false,
            on_change,
        }
    }
}

impl<F> Disableable for Switch<F> {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> IntoElement for Switch<F> {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> RenderOnce for Switch<F> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let checked = self.checked;
        let disabled = self.disabled;
        let on_change = self.on_change;

        // The track is 36x20 with a 2px inset, so the 16px thumb travels 16px.
        div()
            .id(self.id)
            .flex()
            .items_center()
            .flex_shrink_0()
            .w(px(36.0))
            .h(px(20.0))
            .rounded(px(20.0))
            .border(px(2.0))
            .border_color(gpui::transparent_black())
            .bg(if checked {
                theme.primary
            } else {
                theme.border_strong
            })
            .when(disabled, |this| this.opacity(0.5))
            .when(!disabled, |this| {
                this.on_click(move |_, window, cx| on_change(!checked, window, cx))
            })
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(20.0))
                    .ml(if checked { px(16.0) } else { px(0.0) })
                    .bg(theme.background),
            )
    }
}

/// A square check box.
pub(crate) struct Checkbox<F> {
    id: ElementId,
    checked: bool,
    on_change: F,
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> Checkbox<F> {
    /// Constructs a check box; `on_change` receives the toggled value.
    pub(crate) fn new(id: impl Into<ElementId>, checked: bool, on_change: F) -> Self {
        Self {
            id: id.into(),
            checked,
            on_change,
        }
    }
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> IntoElement for Checkbox<F> {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl<F: Fn(bool, &mut Window, &mut App) + 'static> RenderOnce for Checkbox<F> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let checked = self.checked;
        let on_change = self.on_change;

        div()
            .id(self.id)
            .flex()
            .items_center()
            .justify_center()
            .flex_shrink_0()
            .size(rems(1.0))
            .border_1()
            .rounded(theme.radius.min(px(4.0)))
            .map(|this| {
                if checked {
                    this.border_color(theme.primary).bg(theme.primary)
                } else {
                    this.border_color(theme.border_strong)
                        .bg(palette::with_alpha(theme.border_strong, 0.3))
                }
            })
            .on_click(move |_: &ClickEvent, window, cx| on_change(!checked, window, cx))
            .when(checked, |this| {
                this.child(
                    Icon::new(IconName::Check)
                        .size_3()
                        .text_color(theme.on_primary),
                )
            })
    }
}

/// Where a slider was last painted and whether it is being dragged, kept across frames.
#[derive(Default)]
struct SliderDrag {
    bounds: Bounds<Pixels>,
    dragging: bool,
}

/// A horizontal slider over a stepped range.
pub(crate) struct Slider<F> {
    id: ElementId,
    value: f32,
    min: f32,
    max: f32,
    step: f32,
    on_change: F,
}

impl<F: Fn(f32, &mut Window, &mut App) + Clone + 'static> Slider<F> {
    /// Constructs a slider over `min..=max` moving in `step`s; `on_change` receives new values.
    pub(crate) fn new(
        id: impl Into<ElementId>,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        on_change: F,
    ) -> Self {
        Self {
            id: id.into(),
            value,
            min,
            max,
            step,
            on_change,
        }
    }
}

impl<F: Fn(f32, &mut Window, &mut App) + Clone + 'static> IntoElement for Slider<F> {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

/// Snaps a pointer position on the track to a stepped value.
fn slider_value(x: Pixels, bounds: Bounds<Pixels>, min: f32, max: f32, step: f32) -> f32 {
    let width = f32::from(bounds.size.width).max(1.0);
    let fraction = (f32::from(x - bounds.left()) / width).clamp(0.0, 1.0);
    let value = min + fraction * (max - min);

    if step > 0.0 {
        (min + ((value - min) / step).round() * step).clamp(min, max)
    } else {
        value
    }
}

impl<F: Fn(f32, &mut Window, &mut App) + Clone + 'static> RenderOnce for Slider<F> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let drag = window.use_keyed_state(self.id.clone(), cx, |_, _| SliderDrag::default());
        let (min, max, step) = (self.min, self.max, self.step);
        let fraction = if max > min {
            ((self.value - min) / (max - min)).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let value = self.value;
        let on_change = self.on_change;

        let probe = {
            let drag = drag.clone();

            canvas(
                |_, _, _| {},
                move |bounds, _, _, cx| drag.update(cx, |drag, _| drag.bounds = bounds),
            )
            .absolute()
            .size_full()
        };

        div()
            .id(self.id)
            .flex()
            .flex_1()
            .w_full()
            .items_center()
            .h_6()
            .on_mouse_down(MouseButton::Left, {
                let drag = drag.clone();
                let on_change = on_change.clone();
                move |event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();

                    let bounds = drag.update(cx, |drag, _| {
                        drag.dragging = true;
                        drag.bounds
                    });
                    let next = slider_value(event.position.x, bounds, min, max, step);

                    if next != value {
                        on_change(next, window, cx);
                    }
                }
            })
            .on_mouse_move({
                let drag = drag.clone();
                move |event: &MouseMoveEvent, window, cx| {
                    let (dragging, bounds) = {
                        let drag = drag.read(cx);
                        (drag.dragging, drag.bounds)
                    };

                    if !dragging || !event.dragging() {
                        return;
                    }

                    let next = slider_value(event.position.x, bounds, min, max, step);

                    if next != value {
                        on_change(next, window, cx);
                    }
                }
            })
            .on_mouse_up_out(MouseButton::Left, {
                let drag = drag.clone();
                move |_, _, cx| drag.update(cx, |drag, _| drag.dragging = false)
            })
            .on_mouse_up(MouseButton::Left, {
                let drag = drag.clone();
                move |_, _, cx| drag.update(cx, |drag, _| drag.dragging = false)
            })
            .child(
                div()
                    .relative()
                    .w_full()
                    .h_1p5()
                    .rounded(theme.radius)
                    .bg(palette::with_alpha(theme.primary, 0.2))
                    .child(probe)
                    .child(
                        div()
                            .absolute()
                            .h_full()
                            .left_0()
                            .w(relative(fraction))
                            .rounded(theme.radius)
                            .bg(theme.primary),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(-5.0))
                            .left(relative(fraction))
                            .ml(px(-8.0))
                            .size_4()
                            .p(px(1.0))
                            .rounded_full()
                            .bg(palette::with_alpha(theme.primary, 0.5))
                            .child(div().size_full().rounded_full().bg(theme.slider_thumb)),
                    ),
            )
    }
}

/// A dropdown choosing one of several options, shown as a button with the current label.
pub(crate) struct Select<F> {
    id: ElementId,
    options: Vec<(SharedString, SharedString)>,
    value: SharedString,
    on_change: F,
}

impl<F: Fn(SharedString, &mut Window, &mut App) + Clone + 'static> Select<F> {
    /// Constructs a select over `(key, label)` options with the given key selected;
    /// `on_change` receives the chosen key.
    pub(crate) fn new(
        id: impl Into<ElementId>,
        options: Vec<(SharedString, SharedString)>,
        value: impl Into<SharedString>,
        on_change: F,
    ) -> Self {
        Self {
            id: id.into(),
            options,
            value: value.into(),
            on_change,
        }
    }
}

impl<F: Fn(SharedString, &mut Window, &mut App) + Clone + 'static> IntoElement for Select<F> {
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl<F: Fn(SharedString, &mut Window, &mut App) + Clone + 'static> RenderOnce for Select<F> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let label = self
            .options
            .iter()
            .find(|(key, _)| *key == self.value)
            .map_or_else(|| self.value.clone(), |(_, label)| label.clone());
        let options = self.options;
        let value = self.value;
        let on_change = self.on_change;

        Popover::new(
            self.id,
            move |open| {
                Button::new("select-trigger")
                    .outline()
                    .selected(open)
                    .child(
                        h_flex().gap_2().justify_between().child(label).child(
                            Icon::new(IconName::ChevronDown)
                                .size_4()
                                .text_color(palette::with_alpha(theme.text, 0.75)),
                        ),
                    )
                    .into_any_element()
            },
            move |popover, _, cx| {
                let theme = *cx.tessera_theme();
                let mut menu = v_flex().p_1().gap_y_0p5().min_w(rems(8.0)).max_w(px(500.0));

                for (index, (key, label)) in options.iter().enumerate() {
                    let checked = *key == value;
                    let key = key.clone();
                    let on_change = on_change.clone();
                    let popover = popover.clone();

                    menu = menu.child(
                        h_flex()
                            .id(("select-option", index))
                            .h(px(26.0))
                            .px(px(8.0))
                            .gap_x_2()
                            .text_sm()
                            .rounded(theme.radius)
                            .hover(move |this| this.bg(theme.border))
                            .child(div().size_4().flex().items_center().when(checked, |this| {
                                this.child(Icon::new(IconName::Check).size_4())
                            }))
                            .child(label.clone())
                            .on_click(move |_, window, cx| {
                                popover.update(cx, |popover, cx| popover.dismiss(cx));
                                on_change(key.clone(), window, cx);
                            }),
                    );
                }

                popover_style(menu, &theme).into_any_element()
            },
        )
        .anchor(Corner::TopRight)
    }
}
