// Adapted from gpui-component 0.6.6 (`src/table/{state,column,delegate,data_table,loading}.rs`),
// Copyright (c) Longbridge, licensed under the Apache License, Version 2.0.
// See THIRD_PARTY.md. Reduced to what tessera uses: one header row, row selection driven by the
// delegate, resizable and sortable columns and a row context menu with submenus and dividers.
// Column moving, pinned columns, column/cell selection, grouped headers, load-more, striping and
// the skeleton shimmer are removed.

use std::ops::Range;
use std::time::Duration;

use gpui::Action;
use gpui::Animation;
use gpui::AnimationExt;
use gpui::AnyElement;
use gpui::App;
use gpui::AppContext;
use gpui::Bounds;
use gpui::Context;
use gpui::Corner;
use gpui::Div;
use gpui::DragMoveEvent;
use gpui::ElementId;
use gpui::Entity;
use gpui::EntityId;
use gpui::FocusHandle;
use gpui::Focusable;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::ListSizingBehavior;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Point;
use gpui::Refineable;
use gpui::Render;
use gpui::RenderOnce;
use gpui::ScrollHandle;
use gpui::ScrollStrategy;
use gpui::ScrollWheelEvent;
use gpui::SharedString;
use gpui::Stateful;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::UniformListScrollHandle;
use gpui::Window;
use gpui::anchored;
use gpui::bounce;
use gpui::canvas;
use gpui::deferred;
use gpui::div;
use gpui::ease_in_out;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;
use gpui::uniform_list;

use crate::components::Icon;
use crate::components::IconName;
use crate::components::Scrollbar;
use crate::components::h_flex;
use crate::components::popover_style;
use crate::components::v_flex;
use crate::palette;
use crate::theme::ActiveTesseraTheme;
use crate::theme::TesseraTheme;

/// The height of the header and of each row.
const ROW_HEIGHT: Pixels = px(32.0);
/// How far behind the pointer a resize drag trails the column edge.
const HANDLE_SIZE: Pixels = px(2.0);
/// Grab room on each side of a resize handle's hairline.
const HANDLE_PADDING: Pixels = px(4.0);
/// The horizontal padding of a cell.
const CELL_PADDING_X: Pixels = px(8.0);
/// The vertical padding of a cell.
const CELL_PADDING_Y: Pixels = px(4.0);
/// The width of the blank column closing off the header.
const LAST_EMPTY_COL: Pixels = px(12.0);

/// The sort state of a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColumnSort {
    /// Not sorted by this column.
    Default,
    /// Sorted ascending.
    Ascending,
    /// Sorted descending.
    Descending,
}

/// A table column definition.
#[derive(Debug, Clone)]
pub(crate) struct Column {
    pub(crate) name: SharedString,
    width: Pixels,
    resizable: bool,
    sort: Option<ColumnSort>,
}

impl Column {
    /// Constructs a column. The key is unused but kept for the delegate's column ids.
    pub(crate) fn new(_key: impl Into<SharedString>, name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            width: px(100.0),
            resizable: true,
            sort: None,
        }
    }

    /// Sets the initial width.
    pub(crate) fn width(mut self, width: impl Into<Pixels>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets whether the column can be resized.
    pub(crate) fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Makes the column sortable, starting in the given state.
    pub(crate) fn sort(mut self, sort: ColumnSort) -> Self {
        self.sort = Some(sort);
        self
    }
}

/// An entry of the row context menu, dispatching the delegate's action type.
pub(crate) enum ContextMenuItem<A: Action + Clone> {
    /// A row dispatching an action when chosen, greyed out while it has none.
    Entry {
        label: SharedString,
        icon: Option<IconName>,
        action: Option<A>,
    },
    /// A separator line.
    Divider,
    /// A row opening a nested menu while hovered.
    Submenu {
        label: SharedString,
        icon: Option<IconName>,
        items: Vec<ContextMenuItem<A>>,
    },
}

impl<A: Action + Clone> ContextMenuItem<A> {
    /// Constructs a menu entry dispatching `action` when chosen, or a greyed out one when
    /// `action` is `None`.
    pub(crate) fn entry(label: impl Into<SharedString>, action: Option<A>) -> Self {
        Self::Entry {
            label: label.into(),
            icon: None,
            action,
        }
    }

    /// Constructs a separator.
    pub(crate) fn divider() -> Self {
        Self::Divider
    }

    /// Constructs a nested menu.
    pub(crate) fn submenu(label: impl Into<SharedString>, items: Vec<ContextMenuItem<A>>) -> Self {
        Self::Submenu {
            label: label.into(),
            icon: None,
            items,
        }
    }

    /// Sets the icon shown before the label.
    pub(crate) fn icon(mut self, icon: IconName) -> Self {
        match &mut self {
            Self::Entry { icon: slot, .. } | Self::Submenu { icon: slot, .. } => *slot = Some(icon),
            Self::Divider => {}
        }

        self
    }
}

/// Supplies the rows and cells of a [`TableState`].
pub(crate) trait TableDelegate: Sized + 'static {
    /// The action type the row context menu dispatches.
    type Action: Action + Clone;

    /// The number of columns.
    fn columns_count(&self, cx: &App) -> usize;

    /// The number of rows.
    fn rows_count(&self, cx: &App) -> usize;

    /// The column definition at the given index.
    fn column(&self, col_ix: usize, cx: &App) -> Column;

    /// Renders a header cell.
    fn render_th(
        &mut self,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement;

    /// Renders the container of a row.
    fn render_tr(
        &mut self,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div>;

    /// Renders a body cell.
    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement;

    /// Renders the body shown while there are no rows.
    fn render_empty(
        &mut self,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement;

    /// The context menu entries for a row.
    fn context_menu(
        &mut self,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Vec<ContextMenuItem<Self::Action>>;

    /// Sorts the rows by a column.
    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    );

    /// Whether the rows are loading.
    fn loading(&self, cx: &App) -> bool;
}

/// A column with its current width.
#[derive(Clone)]
struct ColGroup {
    column: Column,
    width: Pixels,
}

/// The drag payload of a column resize.
#[derive(Clone)]
struct ResizeColumn(EntityId, usize);

impl Render for ResizeColumn {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// The state of a virtualized table.
pub(crate) struct TableState<D: TableDelegate> {
    focus_handle: FocusHandle,
    delegate: D,
    col_groups: Vec<ColGroup>,
    vertical_scroll_handle: UniformListScrollHandle,
    horizontal_scroll_handle: ScrollHandle,
    /// The bounds of the table, recorded while painting.
    bounds: Bounds<Pixels>,
    right_clicked_row: Option<usize>,
    /// Where the open context menu was requested.
    context_menu: Option<Point<Pixels>>,
    /// The top level entry whose nested menu is open.
    open_submenu: Option<usize>,
    /// Where the open nested menu was painted, since it lies outside the menu's own bounds.
    submenu_bounds: Bounds<Pixels>,
    resizing_col: Option<usize>,
}

impl<D: TableDelegate> Focusable for TableState<D> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<D: TableDelegate> TableState<D> {
    /// Constructs a table over the given delegate.
    pub(crate) fn new(delegate: D, _: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            delegate,
            col_groups: Vec::new(),
            vertical_scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: ScrollHandle::new(),
            bounds: Bounds::default(),
            right_clicked_row: None,
            context_menu: None,
            open_submenu: None,
            submenu_bounds: Bounds::default(),
            resizing_col: None,
        };

        this.prepare_col_groups(cx);
        this
    }

    /// The delegate.
    pub(crate) fn delegate(&self) -> &D {
        &self.delegate
    }

    /// The delegate, mutably.
    pub(crate) fn delegate_mut(&mut self) -> &mut D {
        &mut self.delegate
    }

    /// Scrolls the row at the given index to the top.
    pub(crate) fn scroll_to_row(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        self.vertical_scroll_handle
            .scroll_to_item(row_ix, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scrolls just far enough for the row at the given index to be visible.
    pub(crate) fn scroll_row_into_view(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        self.vertical_scroll_handle
            .scroll_to_item(row_ix, ScrollStrategy::Top);
        cx.notify();
    }

    /// Scrolls the rows by one page, leaving the selection alone.
    pub(crate) fn scroll_page(&mut self, down: bool, cx: &mut Context<Self>) {
        let handle = self.vertical_scroll_handle.0.borrow();
        let viewport = handle.base_handle.bounds().size.height;
        let rows = (viewport / ROW_HEIGHT).floor();
        let shift = ROW_HEIGHT * if down { rows } else { -rows };
        let max = handle.base_handle.max_offset().height;
        let mut offset = handle.base_handle.offset();

        offset.y = (offset.y - shift).clamp(-max, px(0.0));
        handle.base_handle.set_offset(offset);
        cx.notify();
    }

    fn prepare_col_groups(&mut self, cx: &mut Context<Self>) {
        self.col_groups = (0..self.delegate.columns_count(cx))
            .map(|col_ix| {
                let column = self.delegate.column(col_ix, cx);

                ColGroup {
                    width: column.width,
                    column,
                }
            })
            .collect();
    }

    fn close_context_menu(&mut self, cx: &mut Context<Self>) {
        self.context_menu = None;
        self.right_clicked_row = None;
        self.open_submenu = None;
        self.submenu_bounds = Bounds::default();
        cx.notify();
    }

    fn on_row_right_click(
        &mut self,
        event: &MouseDownEvent,
        row_ix: usize,
        cx: &mut Context<Self>,
    ) {
        self.right_clicked_row = Some(row_ix);
        self.context_menu = Some(event.position);
        self.open_submenu = None;
        cx.notify();
    }

    /// The left edge of a column, in window coordinates.
    fn col_left(&self, col_ix: usize) -> Pixels {
        let before = self.col_groups[..col_ix]
            .iter()
            .fold(px(0.0), |total, col| total + col.width);

        self.bounds.left() + px(1.0) + before + self.horizontal_scroll_handle.offset().x
    }

    fn resize_col(&mut self, col_ix: usize, width: Pixels, cx: &mut Context<Self>) {
        let Some(col_group) = self.col_groups.get_mut(col_ix) else {
            return;
        };

        let width = width.clamp(px(20.0), px(1200.0));

        if col_group.column.resizable && col_group.width != width {
            col_group.width = width;
            cx.notify();
        }
    }

    fn perform_sort(&mut self, col_ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sort) = self.col_groups.get(col_ix).and_then(|col| col.column.sort) else {
            return;
        };

        let sort = match sort {
            ColumnSort::Ascending => ColumnSort::Default,
            ColumnSort::Descending => ColumnSort::Ascending,
            ColumnSort::Default => ColumnSort::Descending,
        };

        for (ix, col_group) in self.col_groups.iter_mut().enumerate() {
            if ix == col_ix {
                col_group.column.sort = Some(sort);
            } else if col_group.column.sort.is_some() {
                col_group.column.sort = Some(ColumnSort::Default);
            }
        }

        self.delegate.perform_sort(col_ix, sort, window, cx);

        cx.notify();
    }

    fn render_cell(&self, col_ix: usize) -> Div {
        let width = self.col_groups.get(col_ix).map_or(px(0.0), |col| col.width);

        div()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .px(CELL_PADDING_X)
            .py(CELL_PADDING_Y)
    }

    fn render_sort_icon(&self, col_ix: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sort = self.col_groups.get(col_ix)?.column.sort?;
        let theme = *cx.tessera_theme();

        let (icon, is_on) = match sort {
            ColumnSort::Ascending => (IconName::SortAscending, true),
            ColumnSort::Descending => (IconName::SortDescending, true),
            ColumnSort::Default => (IconName::ChevronsUpDown, false),
        };

        Some(
            div()
                .id(("icon-sort", col_ix))
                .p(px(2.0))
                .rounded(theme.radius / 2.0)
                .when(!is_on, |this| this.opacity(0.5))
                .hover(move |this| this.bg(theme.element).opacity(1.0))
                .active(move |this| this.bg(theme.border_strong).opacity(1.0))
                .on_click(cx.listener(move |table, _, window, cx| {
                    table.perform_sort(col_ix, window, cx);
                }))
                .child(Icon::new(icon).size_3().text_color(theme.text))
                .into_any_element(),
        )
    }

    /// The draggable band on the right edge of a column, with the hairline marking it.
    fn render_resize_handle(&self, col_ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let resizable = self
            .col_groups
            .get(col_ix)
            .is_some_and(|col| col.column.resizable);

        if !resizable {
            return div().into_any_element();
        }

        let theme = *cx.tessera_theme();
        let group = SharedString::from(format!("resizable-handle:{col_ix}"));

        h_flex()
            .id(("resizable-handle", col_ix))
            .group(group.clone())
            .occlude()
            .cursor_col_resize()
            .h_full()
            .w(HANDLE_PADDING * 2.0)
            .ml(-HANDLE_PADDING)
            .mr(-HANDLE_PADDING)
            .justify_center()
            .on_drag_move(
                cx.listener(move |table, event: &DragMoveEvent<ResizeColumn>, _, cx| {
                    let ResizeColumn(entity_id, col_ix) = *event.drag(cx);

                    if cx.entity_id() != entity_id {
                        return;
                    }

                    table.resizing_col = Some(col_ix);

                    let width = event.event.position.x - HANDLE_SIZE - table.col_left(col_ix);

                    table.resize_col(col_ix, width, cx);
                }),
            )
            .on_drag(ResizeColumn(cx.entity_id(), col_ix), |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            })
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|table, _, _, cx| {
                    if table.resizing_col.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(
                div()
                    .h_full()
                    .w(px(1.0))
                    .bg(theme.surface)
                    .group_hover(group, move |this| this.bg(theme.border_strong)),
            )
            .into_any_element()
    }

    fn render_th(&mut self, col_ix: usize, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let content = self
            .delegate
            .render_th(col_ix, window, cx)
            .into_any_element();

        h_flex()
            .h_full()
            .child(
                self.render_cell(col_ix).id(("col-header", col_ix)).child(
                    h_flex()
                        .size_full()
                        .justify_between()
                        .items_center()
                        .child(content)
                        .children(self.render_sort_icon(col_ix, cx)),
                ),
            )
            .child(self.render_resize_handle(col_ix, cx))
    }

    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = *cx.tessera_theme();
        let mut columns = h_flex()
            .min_w_full()
            .flex_shrink_0()
            .h(ROW_HEIGHT)
            .border_b_1()
            .border_color(theme.border_strong);

        for col_ix in 0..self.col_groups.len() {
            columns = columns.child(self.render_th(col_ix, window, cx));
        }

        h_flex()
            .id("table-head")
            .w_full()
            .flex_shrink_0()
            .overflow_x_scroll()
            .track_scroll(&self.horizontal_scroll_handle)
            .bg(theme.border)
            .text_color(theme.text)
            .child(columns.child(h_flex().w(LAST_EMPTY_COL).h_full().flex_shrink_0()))
            .into_any_element()
    }

    fn render_row(
        &mut self,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = *cx.tessera_theme();
        let is_right_clicked = self.right_clicked_row == Some(row_ix);
        let offset_x = self.horizontal_scroll_handle.offset().x;

        let mut cells = h_flex().h_full().flex_shrink_0().ml(offset_x);

        for col_ix in 0..self.col_groups.len() {
            let content = self
                .delegate
                .render_td(row_ix, col_ix, window, cx)
                .into_any_element();

            cells = cells.child(self.render_cell(col_ix).child(content));
        }

        let mut tr = self.delegate.render_tr(row_ix, window, cx);
        let style = tr.style().clone();

        tr = tr
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .w_full()
            .h(ROW_HEIGHT)
            .border_b_1()
            .border_color(theme.surface);
        tr.style().refine(&style);

        let is_selected = style.background.is_some();

        tr.hover(move |this| {
            if is_selected || is_right_clicked {
                this
            } else {
                this.bg(theme.border)
            }
        })
        .when(is_right_clicked, |this| {
            this.child(
                div()
                    .absolute()
                    .top(if row_ix == 0 { px(0.0) } else { px(-1.0) })
                    .left_0()
                    .right_0()
                    .bottom(px(-1.0))
                    .border_1()
                    .border_color(theme.primary_active),
            )
        })
        .on_mouse_down(
            MouseButton::Right,
            cx.listener(move |table, event, _, cx| table.on_row_right_click(event, row_ix, cx)),
        )
        .child(div().flex_1().h_full().overflow_hidden().child(cells))
        .into_any_element()
    }

    fn render_loading(&self, cx: &App) -> AnyElement {
        let theme = cx.tessera_theme();

        let row = |header: bool| {
            let bar = |width: Pixels| {
                div()
                    .h(ROW_HEIGHT * 0.5)
                    .w(width)
                    .rounded(theme.radius / 2.0)
                    .bg(if header {
                        theme.element
                    } else {
                        palette::with_alpha(theme.border, 0.6)
                    })
                    // Fades to half and back every two seconds, so the placeholder reads as
                    // loading rather than as content.
                    .with_animation(
                        "skeleton",
                        Animation::new(Duration::from_secs(2))
                            .repeat()
                            .with_easing(bounce(ease_in_out)),
                        |this, delta| this.opacity(1.0 - delta * 0.5),
                    )
            };

            h_flex()
                .gap_3()
                .h(ROW_HEIGHT)
                .overflow_hidden()
                .px(CELL_PADDING_X)
                .py(CELL_PADDING_Y)
                .justify_between()
                .when(header, |this| this.bg(theme.border))
                .when(!header, |this| {
                    this.border_t_1().border_color(theme.surface)
                })
                .child(
                    h_flex()
                        .gap_3()
                        .flex_1()
                        .child(bar(px(84.0)))
                        .child(bar(px(168.0)))
                        .child(bar(px(56.0))),
                )
                .child(bar(px(84.0)))
        };

        v_flex()
            .child(row(true))
            .child(row(false))
            .child(row(false))
            .child(row(false))
            .child(row(false))
            .into_any_element()
    }

    fn render_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let row_ix = self.right_clicked_row?;
        let items = self.delegate.context_menu(row_ix, window, cx);
        let theme = *cx.tessera_theme();
        let open_submenu = self.open_submenu;
        let focus_handle = self.focus_handle.clone();

        if !focus_handle.is_focused(window) {
            window.focus(&focus_handle);
        }

        let mut menu = render_menu();

        for (index, item) in items.into_iter().enumerate() {
            menu = menu.child(match item {
                ContextMenuItem::Divider => render_menu_divider(&theme),
                ContextMenuItem::Entry {
                    label,
                    icon,
                    action,
                } => render_menu_entry(
                    ("context-menu-item", index),
                    label,
                    icon,
                    action,
                    &theme,
                    cx,
                )
                .on_hover(cx.listener(move |table, hovered, _, cx| {
                    if *hovered && table.open_submenu.is_some() {
                        table.open_submenu = None;
                        cx.notify();
                    }
                }))
                .into_any_element(),
                ContextMenuItem::Submenu { label, icon, items } => {
                    let open = open_submenu == Some(index);
                    let mut row =
                        render_menu_row(("context-menu-item", index), label, icon, &theme)
                            .relative()
                            .hover(move |this| this.bg(theme.border))
                            .when(open, |this| this.bg(theme.border))
                            .on_hover(cx.listener(move |table, hovered, _, cx| {
                                if *hovered && table.open_submenu != Some(index) {
                                    table.open_submenu = Some(index);
                                    cx.notify();
                                }
                            }))
                            .child(
                                Icon::new(IconName::ChevronRight)
                                    .size_3()
                                    .ml_auto()
                                    .text_color(theme.text_secondary),
                            );

                    if open {
                        let mut submenu = render_menu();

                        for (child_ix, child) in items.into_iter().enumerate() {
                            submenu = submenu.child(match child {
                                ContextMenuItem::Divider => render_menu_divider(&theme),
                                ContextMenuItem::Entry {
                                    label,
                                    icon,
                                    action,
                                } => render_menu_entry(
                                    ("context-submenu-item", index * 100 + child_ix),
                                    label,
                                    icon,
                                    action,
                                    &theme,
                                    cx,
                                )
                                .into_any_element(),
                                // Nesting stops at one level, so a deeper menu is shown flat.
                                ContextMenuItem::Submenu { label, icon, .. } => render_menu_row(
                                    ("context-submenu-item", index * 100 + child_ix),
                                    label,
                                    icon,
                                    &theme,
                                )
                                .into_any_element(),
                            });
                        }

                        let probe = {
                            let entity = cx.entity();

                            canvas(
                                |_, _, _| {},
                                move |bounds, _, _, cx| {
                                    entity.update(cx, |table, _| table.submenu_bounds = bounds);
                                },
                            )
                            .absolute()
                            .size_full()
                        };

                        row = row.child(
                            div()
                                .absolute()
                                .left(relative(1.0))
                                .top(px(-4.0))
                                .pl_1()
                                .child(popover_style(submenu, &theme).relative().child(probe)),
                        );
                    }

                    row.into_any_element()
                }
            });
        }

        Some(
            deferred(
                anchored()
                    .position(position)
                    .anchor(Corner::TopLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        popover_style(menu, &theme)
                            .id("context-menu")
                            .occlude()
                            .track_focus(&focus_handle)
                            .on_key_down(cx.listener(|table, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "escape" {
                                    cx.stop_propagation();
                                    table.close_context_menu(cx);
                                }
                            }))
                            .on_mouse_down_out(cx.listener(
                                |table, event: &MouseDownEvent, _, cx| {
                                    // The nested menu hangs outside these bounds, so a press on
                                    // it is not a press outside the menu.
                                    if !table.submenu_bounds.contains(&event.position) {
                                        table.close_context_menu(cx);
                                    }
                                },
                            )),
                    ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// Scrolls the columns sideways for horizontal wheel motion over the rows.
    fn on_body_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(window.line_height());
        let delta_x = if event.modifiers.shift && delta.x == px(0.0) {
            delta.y
        } else {
            delta.x
        };

        if delta_x == px(0.0) {
            return;
        }

        let max_x = self.horizontal_scroll_handle.max_offset().width;
        let mut offset = self.horizontal_scroll_handle.offset();

        offset.x = (offset.x + delta_x).clamp(-max_x, px(0.0));

        self.horizontal_scroll_handle.set_offset(offset);
        cx.notify();
    }
}

impl<D: TableDelegate> Render for TableState<D> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows_count = self.delegate.rows_count(cx);
        let loading = self.delegate.loading(cx);
        let entity = cx.entity();

        let probe = canvas(
            |_, _, _| {},
            move |bounds, _, _, cx| entity.update(cx, |table, _| table.bounds = bounds),
        )
        .absolute()
        .size_full();

        let context_menu = self
            .context_menu
            .and_then(|position| self.render_context_menu(position, window, cx));

        if loading {
            return div()
                .size_full()
                .relative()
                .child(probe)
                .child(self.render_loading(cx))
                .into_any_element();
        }

        let body = if rows_count == 0 {
            div()
                .size_full()
                .child(self.delegate.render_empty(window, cx))
                .into_any_element()
        } else {
            h_flex()
                .id("table-body")
                .flex_grow()
                .size_full()
                .on_scroll_wheel(cx.listener(Self::on_body_scroll_wheel))
                .child(
                    uniform_list(
                        "table-uniform-list",
                        rows_count,
                        cx.processor(move |table, visible_range: Range<usize>, window, cx| {
                            visible_range
                                .map(|row_ix| table.render_row(row_ix, window, cx))
                                .collect::<Vec<_>>()
                        }),
                    )
                    .flex_grow()
                    .size_full()
                    .with_sizing_behavior(ListSizingBehavior::Auto)
                    .track_scroll(self.vertical_scroll_handle.clone()),
                )
                .into_any_element()
        };

        let inner = v_flex()
            .id("table-inner")
            .size_full()
            .overflow_hidden()
            .child(self.render_header(window, cx))
            .child(body);

        div()
            .size_full()
            .relative()
            .child(probe)
            .child(inner)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(16.0))
                    .child(
                        Scrollbar::horizontal(
                            "table-horizontal-scrollbar",
                            &self.horizontal_scroll_handle,
                        )
                        .viewport_from_layout(),
                    ),
            )
            .when(rows_count > 0, |this| {
                this.child(
                    div()
                        .absolute()
                        .top(ROW_HEIGHT)
                        .right_0()
                        .bottom_0()
                        .w(px(16.0))
                        .child(
                            Scrollbar::vertical(
                                "table-vertical-scrollbar",
                                &self.vertical_scroll_handle,
                            )
                            .viewport_from_layout(),
                        ),
                )
            })
            .children(context_menu)
            .into_any_element()
    }
}

/// The container of a context menu or one of its nested menus.
fn render_menu() -> Div {
    v_flex()
        .p_1()
        .gap_y_0p5()
        .min_w(gpui::rems(8.0))
        .max_w(px(500.0))
}

/// A separator line between menu entries.
fn render_menu_divider(theme: &TesseraTheme) -> AnyElement {
    div()
        .h(px(1.0))
        .my_1()
        .mx_1()
        .bg(theme.border_strong)
        .into_any_element()
}

/// A menu row with its icon slot and label, without any behaviour.
fn render_menu_row(
    id: impl Into<ElementId>,
    label: SharedString,
    icon: Option<IconName>,
    theme: &TesseraTheme,
) -> Stateful<Div> {
    let text_secondary = theme.text_secondary;

    h_flex()
        .id(id)
        .h(px(26.0))
        .px(px(8.0))
        .gap_x_2()
        .text_sm()
        .rounded(theme.radius)
        .when_some(icon, |this, icon| {
            this.child(Icon::new(icon).size_4().text_color(text_secondary))
        })
        .child(label)
}

/// A menu entry dispatching its action when clicked, or greyed out without one.
fn render_menu_entry<D: TableDelegate>(
    id: impl Into<ElementId>,
    label: SharedString,
    icon: Option<IconName>,
    action: Option<D::Action>,
    theme: &TesseraTheme,
    cx: &mut Context<TableState<D>>,
) -> Stateful<Div> {
    let row = render_menu_row(id, label, icon, theme);
    let hover_bg = theme.border;

    match action {
        Some(action) => row
            .hover(move |this| this.bg(hover_bg))
            .on_click(cx.listener(move |table, _, window, cx| {
                table.close_context_menu(cx);
                window.dispatch_action(Box::new(action.clone()), cx);
            })),
        None => row.text_color(theme.text_muted),
    }
}

/// Renders a [`TableState`] with its border.
#[derive(IntoElement)]
pub(crate) struct DataTable<D: TableDelegate> {
    state: Entity<TableState<D>>,
}

impl<D: TableDelegate> DataTable<D> {
    /// Constructs a table view of the given state.
    pub(crate) fn new(state: &Entity<TableState<D>>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl<D: TableDelegate> RenderOnce for DataTable<D> {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let focus_handle = self.state.read(cx).focus_handle.clone();

        // Clicking the rows takes focus away from the inputs, so the list keys reach the window.
        div()
            .id("table")
            .size_full()
            .track_focus(&focus_handle)
            .on_mouse_down(MouseButton::Left, move |_, window, _| {
                window.focus(&focus_handle)
            })
            .bg(theme.element)
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border_strong)
            .overflow_hidden()
            .child(self.state)
    }
}
