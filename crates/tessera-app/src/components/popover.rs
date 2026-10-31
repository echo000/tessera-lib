use crate::components::box_shadow;
use gpui::AnyElement;
use gpui::App;
use gpui::Bounds;
use gpui::BoxShadow;
use gpui::Component;
use gpui::Context;
use gpui::Corner;
use gpui::ElementId;
use gpui::Entity;
use gpui::FocusHandle;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Refineable;
use gpui::RenderOnce;
use gpui::StatefulInteractiveElement;
use gpui::StyleRefinement;
use gpui::Styled;
use gpui::Window;
use gpui::anchored;
use gpui::canvas;
use gpui::deferred;
use gpui::div;
use gpui::hsla;
use gpui::point;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;

use crate::theme::TesseraTheme;

/// The gap between a trigger and its popover.
const POPOVER_OFFSET: Pixels = px(3.0);

/// Whether a popover is open, kept across frames.
pub(crate) struct PopoverState {
    open: bool,
    focus_handle: FocusHandle,
    /// Where the trigger was last painted, so pressing it again toggles rather than reopens.
    trigger_bounds: Bounds<Pixels>,
}

impl PopoverState {
    /// Closes the popover.
    pub(crate) fn dismiss(&mut self, cx: &mut Context<Self>) {
        self.open = false;
        cx.notify();
    }
}

/// A trigger that opens floating content below it when clicked.
pub(crate) struct Popover<T, F> {
    id: ElementId,
    anchor: Corner,
    style: StyleRefinement,
    trigger: T,
    content: F,
}

impl<T, F> Popover<T, F>
where
    T: FnOnce(bool) -> AnyElement + 'static,
    F: Fn(&Entity<PopoverState>, &mut Window, &mut App) -> AnyElement + 'static,
{
    /// Constructs a popover. `trigger` builds the trigger, told whether the popover is open;
    /// `content` builds what is shown while open, given the popover's state so it can dismiss it.
    pub(crate) fn new(id: impl Into<ElementId>, trigger: T, content: F) -> Self {
        Self {
            id: id.into(),
            anchor: Corner::TopLeft,
            style: StyleRefinement::default(),
            trigger,
            content,
        }
    }

    /// Aligns the given corner of the content with the same side of the trigger.
    pub(crate) fn anchor(mut self, anchor: Corner) -> Self {
        self.anchor = anchor;
        self
    }
}

impl<T, F> Styled for Popover<T, F> {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl<T, F> IntoElement for Popover<T, F>
where
    T: FnOnce(bool) -> AnyElement + 'static,
    F: Fn(&Entity<PopoverState>, &mut Window, &mut App) -> AnyElement + 'static,
{
    type Element = Component<Self>;

    fn into_element(self) -> Self::Element {
        Component::new(self)
    }
}

impl<T, F> RenderOnce for Popover<T, F>
where
    T: FnOnce(bool) -> AnyElement + 'static,
    F: Fn(&Entity<PopoverState>, &mut Window, &mut App) -> AnyElement + 'static,
{
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| PopoverState {
            open: false,
            focus_handle: cx.focus_handle(),
            trigger_bounds: Bounds::default(),
        });
        let (open, focus_handle) = {
            let state = state.read(cx);
            (state.open, state.focus_handle.clone())
        };

        let mut base = div().id(self.id).relative().flex();
        base.style().refine(&self.style);

        let probe = {
            let state = state.clone();

            canvas(
                |_, _, _| {},
                move |bounds, _, _, cx| {
                    state.update(cx, |state, _| state.trigger_bounds = bounds);
                },
            )
            .absolute()
            .size_full()
        };

        let content = open.then(|| (self.content)(&state, window, cx));

        base.child(probe)
            .child((self.trigger)(open))
            .on_click({
                let state = state.clone();
                let focus_handle = focus_handle.clone();

                move |_, window, cx| {
                    let open = state.update(cx, |state, cx| {
                        state.open = !state.open;
                        cx.notify();
                        state.open
                    });

                    if open {
                        window.focus(&focus_handle);
                    }
                }
            })
            .when_some(content, |this, content| {
                let edge = div().absolute().top(relative(1.0));
                let edge = match self.anchor {
                    Corner::TopRight | Corner::BottomRight => edge.right_0(),
                    _ => edge.left_0(),
                };

                this.child(
                    edge.child(
                        deferred(
                            anchored()
                                .anchor(self.anchor)
                                .offset(point(px(0.0), POPOVER_OFFSET))
                                .snap_to_window_with_margin(px(8.0))
                                .child(
                                    div()
                                        .id("popover-content")
                                        .occlude()
                                        .track_focus(&focus_handle)
                                        .key_context("Popover")
                                        .on_key_down({
                                            let state = state.clone();
                                            move |event, _, cx| {
                                                if event.keystroke.key == "escape" {
                                                    cx.stop_propagation();
                                                    state.update(cx, |state, cx| state.dismiss(cx));
                                                }
                                            }
                                        })
                                        .on_mouse_down_out({
                                            let state = state.clone();
                                            move |event, _, cx| {
                                                state.update(cx, |state, cx| {
                                                    if !state
                                                        .trigger_bounds
                                                        .contains(&event.position)
                                                    {
                                                        state.dismiss(cx);
                                                    }
                                                });
                                            }
                                        })
                                        // Keep presses inside from reaching what is under the popover.
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation()
                                        })
                                        .child(content),
                                ),
                        )
                        .with_priority(1),
                    ),
                )
            })
    }
}

/// Gives an element the surface, edge and shadow of a popover.
pub(crate) fn popover_style<E: Styled>(element: E, theme: &TesseraTheme) -> E {
    element
        .bg(theme.element)
        .text_color(theme.text)
        .rounded(theme.radius)
        .shadow(popover_shadow(theme))
}

/// A hairline ring plus Tailwind's `shadow-md`, with gpui's blur radii.
fn popover_shadow(theme: &TesseraTheme) -> Vec<BoxShadow> {
    let ink = hsla(0.0, 0.0, 0.0, 0.1);
    let mut ring: gpui::Hsla = theme.text.into();
    ring.a = 0.1;

    vec![
        box_shadow(px(0.0), px(0.0), px(0.0), px(1.0), ring),
        box_shadow(px(0.0), px(4.0), px(3.0), px(-1.0), ink),
        box_shadow(px(0.0), px(2.0), px(2.0), px(-2.0), ink),
    ]
}
