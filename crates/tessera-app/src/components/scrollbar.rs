// Adapted from gpui-component 0.6.6 (`gpui-base/src/scrollbar.rs`),
// Copyright (c) Longbridge, licensed under the Apache License, Version 2.0.
// See THIRD_PARTY.md. Reduced to an always visible bar painted from the tessera theme: the
// fade/expand motion, touch dragging, style overrides and show modes are removed.

use gpui::AnyElement;
use gpui::App;
use gpui::Axis;
use gpui::Bounds;
use gpui::ContentMask;
use gpui::CursorStyle;
use gpui::Element;
use gpui::ElementId;
use gpui::Entity;
use gpui::GlobalElementId;
use gpui::Hitbox;
use gpui::HitboxBehavior;
use gpui::Hsla;
use gpui::InspectorElementId;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::LayoutId;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Point;
use gpui::Position;
use gpui::ScrollHandle;
use gpui::Size;
use gpui::StatefulInteractiveElement;
use gpui::Style;
use gpui::Styled;
use gpui::UniformListScrollHandle;
use gpui::Window;
use gpui::div;
use gpui::fill;
use gpui::point;
use gpui::px;
use gpui::relative;
use gpui::size;

use crate::theme::ActiveTesseraTheme;

/// The width of the scrollbar track.
const WIDTH: Pixels = px(16.0);
const MIN_THUMB_SIZE: Pixels = px(48.0);
const THUMB_WIDTH: Pixels = px(6.0);
const THUMB_ACTIVE_WIDTH: Pixels = px(8.0);
const THUMB_INSET: Pixels = px(4.0);

/// A scroll position the scrollbar can read and drive.
pub(crate) trait ScrollbarHandle: Clone + 'static {
    /// Bounds of the viewport the scrollbar overlays.
    fn viewport_bounds(&self) -> Bounds<Pixels>;
    /// The current scroll offset.
    fn offset(&self) -> Point<Pixels>;
    /// Moves the scroll offset.
    fn set_offset(&self, offset: Point<Pixels>);
    /// The full size of the content.
    fn content_size(&self) -> Size<Pixels>;
}

impl ScrollbarHandle for ScrollHandle {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.bounds()
    }

    fn offset(&self) -> Point<Pixels> {
        ScrollHandle::offset(self)
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        ScrollHandle::set_offset(self, offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        handle_content_size(self)
    }
}

impl ScrollbarHandle for UniformListScrollHandle {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().base_handle.bounds()
    }

    fn offset(&self) -> Point<Pixels> {
        self.0.borrow().base_handle.offset()
    }

    fn set_offset(&self, offset: Point<Pixels>) {
        self.0.borrow_mut().base_handle.set_offset(offset);
    }

    fn content_size(&self) -> Size<Pixels> {
        handle_content_size(&self.0.borrow().base_handle)
    }
}

/// The scrolled content size: the viewport plus how far it can scroll.
fn handle_content_size(handle: &ScrollHandle) -> Size<Pixels> {
    let max_offset = handle.max_offset();
    let viewport = handle.bounds().size;

    size(
        viewport.width + max_offset.width,
        viewport.height + max_offset.height,
    )
}

/// Pointer interaction with the bars, kept across frames.
#[derive(Default)]
struct ScrollbarState {
    hovered_axis: Option<Axis>,
    hovered_on_thumb: Option<Axis>,
    dragged_axis: Option<Axis>,
    drag_pos: Point<Pixels>,
}

/// A scrollbar overlaying its parent, which must be relatively positioned.
pub(crate) struct Scrollbar<H: ScrollbarHandle> {
    id: ElementId,
    axis: Axis,
    handle: H,
    /// Overlay the scrollbar's own layout bounds rather than the handle's viewport.
    use_layout_bounds: bool,
}

impl<H: ScrollbarHandle> Scrollbar<H> {
    /// A vertical scrollbar.
    pub(crate) fn vertical(id: impl Into<ElementId>, handle: &H) -> Self {
        Self {
            id: id.into(),
            axis: Axis::Vertical,
            handle: handle.clone(),
            use_layout_bounds: false,
        }
    }

    /// A horizontal scrollbar.
    pub(crate) fn horizontal(id: impl Into<ElementId>, handle: &H) -> Self {
        Self {
            axis: Axis::Horizontal,
            ..Self::vertical(id, handle)
        }
    }

    /// Overlays the scrollbar's own layout bounds instead of the handle's viewport, for
    /// viewports that exclude a fixed header.
    pub(crate) fn viewport_from_layout(mut self) -> Self {
        self.use_layout_bounds = true;
        self
    }
}

/// The track and thumb of one bar, resolved during prepaint.
struct AxisPrepaintState {
    axis: Axis,
    bar_hitbox: Hitbox,
    bounds: Bounds<Pixels>,
    thumb_bounds: Bounds<Pixels>,
    thumb_fill_bounds: Bounds<Pixels>,
    thumb_bg: Hsla,
    track_bg: Hsla,
    radius: Pixels,
    geometry: ThumbGeometry,
}

#[doc(hidden)]
pub(crate) struct PrepaintState {
    hitbox: Hitbox,
    state: Entity<ScrollbarState>,
    states: Vec<AxisPrepaintState>,
}

/// The same longitudinal geometry drives painting, track clicks and dragging.
/// `length` excludes the inset, while `travel` excludes the full logical thumb.
#[derive(Clone, Copy)]
struct ThumbGeometry {
    origin: Pixels,
    inset: Pixels,
    length: Pixels,
    travel: Pixels,
    extent: Pixels,
}

impl ThumbGeometry {
    fn new(origin: Pixels, container: Pixels, content: Pixels, margin_end: Pixels) -> Self {
        let track = (container - margin_end).max(px(0.0));
        let logical_length = (container / content * container)
            .max(MIN_THUMB_SIZE)
            .min(track);
        let inset = THUMB_INSET.clamp(px(0.0), logical_length / 2.0);

        Self {
            origin,
            inset,
            length: logical_length - inset * 2.0,
            travel: track - logical_length,
            extent: content - container,
        }
    }

    fn start(self, offset: Pixels) -> Pixels {
        self.origin + self.inset + (-offset / self.extent).clamp(0.0, 1.0) * self.travel
    }

    fn offset(self, position: Pixels, grab: Pixels) -> Pixels {
        if self.travel <= px(0.0) {
            return px(0.0);
        }

        -self.extent * ((position - self.origin - self.inset - grab) / self.travel).clamp(0.0, 1.0)
    }

    fn drag_offset(
        self,
        axis: Axis,
        position: Point<Pixels>,
        grab: Point<Pixels>,
        mut offset: Point<Pixels>,
    ) -> Point<Pixels> {
        if self.travel <= px(0.0) {
            return offset;
        }

        match axis {
            Axis::Vertical => offset.y = self.offset(position.y, grab.y),
            Axis::Horizontal => offset.x = self.offset(position.x, grab.x),
        }

        offset
    }
}

impl<H: ScrollbarHandle> IntoElement for Scrollbar<H> {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl<H: ScrollbarHandle> Element for Scrollbar<H> {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style {
            position: Position::Absolute,
            flex_grow: 1.0,
            flex_shrink: 1.0,
            ..Default::default()
        };
        style.size.width = relative(1.0).into();
        style.size.height = relative(1.0).into();

        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        layout_bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let bounds = if self.use_layout_bounds {
            layout_bounds
        } else {
            self.handle.viewport_bounds()
        };
        let hitbox = window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.insert_hitbox(bounds, HitboxBehavior::Normal)
        });

        let state = window.use_state(cx, |_, _| ScrollbarState::default());
        let (hovered_on_thumb, dragged_axis) = {
            let state = state.read(cx);
            (state.hovered_on_thumb, state.dragged_axis)
        };

        let theme = *cx.tessera_theme();
        let scroll_size = self.handle.content_size();
        let offset = self.handle.offset();
        let mut states = Vec::new();

        for axis in [self.axis] {
            let is_vertical = axis == Axis::Vertical;
            let (content, container, position) = if is_vertical {
                (scroll_size.height, hitbox.size.height, offset.y)
            } else {
                (scroll_size.width, hitbox.size.width, offset.x)
            };

            let margin_end = px(0.0);

            // Nothing to scroll along this axis.
            if content <= container {
                continue;
            }

            let bar_bounds = if is_vertical {
                Bounds {
                    origin: point(hitbox.origin.x + hitbox.size.width - WIDTH, hitbox.origin.y),
                    size: size(WIDTH, hitbox.size.height),
                }
            } else {
                Bounds {
                    origin: point(
                        hitbox.origin.x,
                        hitbox.origin.y + hitbox.size.height - WIDTH,
                    ),
                    size: size(hitbox.size.width, WIDTH),
                }
            };

            let active = dragged_axis == Some(axis) || hovered_on_thumb == Some(axis);
            let thumb_width = if active {
                THUMB_ACTIVE_WIDTH
            } else {
                THUMB_WIDTH
            };
            let thumb_bg = if active {
                theme.primary
            } else {
                theme.scrollbar_thumb
            };

            let origin = if is_vertical {
                bar_bounds.origin.y
            } else {
                bar_bounds.origin.x
            };
            let geometry = ThumbGeometry::new(origin, container, content, margin_end);
            let thumb_start = geometry.start(position) - origin;

            // The clickable area of the thumb spans the track; the painted area is narrower.
            let (thumb_bounds, thumb_fill_bounds) = if is_vertical {
                let top_right = bar_bounds.top_right() + point(-THUMB_INSET, thumb_start);

                (
                    Bounds {
                        origin: top_right - point(WIDTH, px(0.0)),
                        size: size(WIDTH, geometry.length),
                    },
                    Bounds {
                        origin: top_right - point(thumb_width, px(0.0)),
                        size: size(thumb_width, geometry.length),
                    },
                )
            } else {
                let bottom_left = bar_bounds.bottom_left() + point(thumb_start, -THUMB_INSET);

                (
                    Bounds {
                        origin: bottom_left - point(px(0.0), WIDTH),
                        size: size(geometry.length, WIDTH),
                    },
                    Bounds {
                        origin: bottom_left - point(px(0.0), thumb_width),
                        size: size(geometry.length, thumb_width),
                    },
                )
            };

            let bar_hitbox = window
                .with_content_mask(Some(ContentMask { bounds: bar_bounds }), |window| {
                    window.insert_hitbox(bar_bounds, HitboxBehavior::Normal)
                });

            let radius = theme
                .radius
                .min(thumb_fill_bounds.size.width / 2.0)
                .min(thumb_fill_bounds.size.height / 2.0);

            states.push(AxisPrepaintState {
                axis,
                bar_hitbox,
                bounds: bar_bounds,
                thumb_bounds,
                thumb_fill_bounds,
                thumb_bg: thumb_bg.into(),
                track_bg: gpui::transparent_black(),
                radius,
                geometry,
            });
        }

        PrepaintState {
            hitbox,
            state,
            states,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        _: &mut App,
    ) {
        let view_id = window.current_view();
        let hitbox_bounds = prepaint.hitbox.bounds;

        window.with_content_mask(
            Some(ContentMask {
                bounds: hitbox_bounds,
            }),
            |window| {
                for axis_state in &prepaint.states {
                    let axis = axis_state.axis;
                    let bounds = axis_state.bounds;
                    let thumb_bounds = axis_state.thumb_bounds;
                    let geometry = axis_state.geometry;

                    window.set_cursor_style(CursorStyle::default(), &axis_state.bar_hitbox);

                    window.paint_layer(hitbox_bounds, |window| {
                        window.paint_quad(fill(bounds, axis_state.track_bg));
                        window.paint_quad(
                            fill(axis_state.thumb_fill_bounds, axis_state.thumb_bg)
                                .corner_radii(axis_state.radius),
                        );
                    });

                    window.on_mouse_event({
                        let state = prepaint.state.clone();
                        let handle = self.handle.clone();

                        move |event: &MouseDownEvent, phase, _, cx| {
                            if !phase.bubble() || !bounds.contains(&event.position) {
                                return;
                            }

                            cx.stop_propagation();

                            if thumb_bounds.contains(&event.position) {
                                // Grab the thumb where it was pressed.
                                let grab = event.position - thumb_bounds.origin;

                                state.update(cx, |state, _| {
                                    state.dragged_axis = Some(axis);
                                    state.drag_pos = grab;
                                });
                            } else {
                                // Jump so the thumb centers on the click.
                                let center = point(geometry.length / 2.0, geometry.length / 2.0);

                                handle.set_offset(geometry.drag_offset(
                                    axis,
                                    event.position,
                                    center,
                                    handle.offset(),
                                ));
                            }

                            cx.notify(view_id);
                        }
                    });

                    window.on_mouse_event({
                        let state = prepaint.state.clone();
                        let handle = self.handle.clone();

                        move |event: &MouseMoveEvent, _, _, cx| {
                            let (hovered_axis, hovered_on_thumb, dragged_axis, drag_pos) = {
                                let state = state.read(cx);
                                (
                                    state.hovered_axis,
                                    state.hovered_on_thumb,
                                    state.dragged_axis,
                                    state.drag_pos,
                                )
                            };

                            let over_bar = bounds.contains(&event.position);
                            let over_thumb = thumb_bounds.contains(&event.position);
                            let next_hovered = if over_bar {
                                Some(axis)
                            } else if hovered_axis == Some(axis) {
                                None
                            } else {
                                hovered_axis
                            };
                            let next_on_thumb = if over_thumb {
                                Some(axis)
                            } else if hovered_on_thumb == Some(axis) {
                                None
                            } else {
                                hovered_on_thumb
                            };

                            let mut notify =
                                next_hovered != hovered_axis || next_on_thumb != hovered_on_thumb;

                            if notify {
                                state.update(cx, |state, _| {
                                    state.hovered_axis = next_hovered;
                                    state.hovered_on_thumb = next_on_thumb;
                                });
                            }

                            // Move the thumb while dragging.
                            if dragged_axis == Some(axis) && event.dragging() {
                                // Keep the drag from selecting text or reaching the content.
                                cx.stop_propagation();

                                let offset = geometry.drag_offset(
                                    axis,
                                    event.position,
                                    drag_pos,
                                    handle.offset(),
                                );

                                if handle.offset() != offset {
                                    handle.set_offset(offset);
                                    notify = true;
                                }
                            }

                            if notify {
                                cx.notify(view_id);
                            }
                        }
                    });

                    window.on_mouse_event({
                        let state = prepaint.state.clone();
                        let handle = self.handle.clone();

                        move |event: &MouseUpEvent, phase, _, cx| {
                            if !phase.bubble() || state.read(cx).dragged_axis != Some(axis) {
                                return;
                            }

                            let drag_pos = state.read(cx).drag_pos;

                            handle.set_offset(geometry.drag_offset(
                                axis,
                                event.position,
                                drag_pos,
                                handle.offset(),
                            ));

                            state.update(cx, |state, _| state.dragged_axis = None);

                            cx.notify(view_id);
                        }
                    });
                }
            },
        );
    }
}

/// Wraps content in a vertically scrolling container with a scrollbar.
///
/// The scroll position is kept under `id` for as long as the container keeps rendering.
pub(crate) fn scrollable_y(
    id: impl Into<ElementId>,
    content: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = id.into();
    let handle = window
        .use_keyed_state(id.clone(), cx, |_, _| ScrollHandle::new())
        .read(cx)
        .clone();

    div()
        .id(id)
        .relative()
        .size_full()
        .min_h_0()
        .child(
            div()
                .id("scroll-content")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&handle)
                .child(content),
        )
        .child(Scrollbar::vertical("scrollbar", &handle))
        .into_any_element()
}
