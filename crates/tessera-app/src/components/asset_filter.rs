use std::collections::BTreeSet;

use gpui::AnyElement;
use gpui::App;
use gpui::Context;
use gpui::Corner;
use gpui::Div;
use gpui::ElementId;
use gpui::Entity;
use gpui::EventEmitter;
use gpui::FontWeight;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::RenderOnce;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder;
use gpui::px;

use crate::components::Icon;
use crate::components::IconName;
use crate::components::Popover;
use crate::components::Sizable;
use crate::components::h_flex;
use crate::components::popover_style;
use crate::components::v_flex;
use crate::theme::ActiveTesseraTheme;

/// Event emitted when a column filter's selection changes.
#[derive(Debug, Clone, Copy)]
pub struct FilterListChanged;

/// The shown options of a multi-select column filter. Every option starts selected.
pub struct FilterListState {
    options: Vec<&'static str>,
    selected: BTreeSet<usize>,
}

impl EventEmitter<FilterListChanged> for FilterListState {}

impl FilterListState {
    /// Constructs a filter over the given options, all selected.
    pub(crate) fn new(options: Vec<&'static str>) -> Self {
        let selected = (0..options.len()).collect();

        Self { options, selected }
    }

    /// The indices of the selected options.
    pub(crate) fn selection(&self) -> &BTreeSet<usize> {
        &self.selected
    }

    /// The labels of the selected options.
    pub(crate) fn selected_values(&self) -> Vec<&'static str> {
        self.selected
            .iter()
            .filter_map(|index| self.options.get(*index).copied())
            .collect()
    }

    fn toggle(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.selected.remove(&index) {
            self.selected.insert(index);
        }

        cx.emit(FilterListChanged);
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        self.selected = (0..self.options.len()).collect();

        cx.emit(FilterListChanged);
        cx.notify();
    }
}

#[derive(Clone, Copy)]
pub(crate) enum AssetFilterKind {
    Type,
    Status,
}

impl AssetFilterKind {
    fn id(self) -> &'static str {
        match self {
            AssetFilterKind::Type => "type",
            AssetFilterKind::Status => "status",
        }
    }
}

#[derive(IntoElement)]
pub(crate) struct AssetColumnFilter {
    id: ElementId,
    label: SharedString,
    kind: AssetFilterKind,
    state: Entity<FilterListState>,
    options: Vec<&'static str>,
    selected_count: usize,
    modified: bool,
}

impl AssetColumnFilter {
    pub(crate) fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        kind: AssetFilterKind,
        state: Entity<FilterListState>,
        options: Vec<&'static str>,
        selected_count: usize,
        modified: bool,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind,
            state,
            options,
            selected_count,
            modified,
        }
    }
}

impl RenderOnce for AssetColumnFilter {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let popover_id = SharedString::from(format!("asset-filter-popover:{:?}", self.id));
        let trigger =
            FilterHeaderTrigger::new(self.id, self.label, self.selected_count, self.modified);
        let kind = self.kind;
        let state = self.state;
        let options = self.options;

        Popover::new(
            popover_id,
            move |open| trigger.selected(open).into_any_element(),
            move |_, _, cx| render_filter_popover_content(kind, &state, &options, cx),
        )
        .anchor(Corner::TopLeft)
        .size_full()
    }
}

fn render_filter_popover_content(
    kind: AssetFilterKind,
    state: &Entity<FilterListState>,
    options: &[&'static str],
    cx: &mut App,
) -> AnyElement {
    let theme = *cx.tessera_theme();
    let selected = state.read(cx).selection().clone();
    let modified = selected.len() != options.len();

    popover_style(v_flex(), &theme)
        .min_w(px(210.0))
        .max_w(px(240.0))
        .p_1()
        .child(render_clear_row(kind, state, modified, cx))
        .child(div().my_1().h(px(1.0)).w_full().bg(theme.border_strong))
        .child(render_option_rows(kind, state, &selected, options, cx))
        .into_any_element()
}

fn render_clear_row(
    kind: AssetFilterKind,
    state: &Entity<FilterListState>,
    modified: bool,
    cx: &mut App,
) -> impl IntoElement {
    let theme = *cx.tessera_theme();

    h_flex()
        .id(SharedString::from(format!(
            "asset-filter-clear-{}",
            kind.id()
        )))
        .h(px(30.0))
        .w_full()
        .justify_between()
        .rounded(theme.radius)
        .px_2()
        .text_size(px(13.0))
        .text_color(theme.text)
        .child(
            h_flex()
                .min_w_0()
                .child(
                    Icon::new(IconName::Funnel)
                        .size_4()
                        .text_color(theme.text_secondary),
                )
                .child(div().ml_2().font_weight(FontWeight::MEDIUM).child("Filter")),
        )
        .when(modified, |this| {
            let state = state.clone();

            this.child(
                h_flex()
                    .id(SharedString::from(format!(
                        "asset-filter-clear-button-{}",
                        kind.id()
                    )))
                    .size(px(20.0))
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .text_color(theme.text_secondary)
                    .hover(move |this| this.bg(theme.border).text_color(theme.text))
                    .child(Icon::new(IconName::CircleX).size_4())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        state.update(cx, |state, cx| state.select_all(cx));
                    }),
            )
        })
}

fn render_option_rows(
    kind: AssetFilterKind,
    state: &Entity<FilterListState>,
    selected: &BTreeSet<usize>,
    options: &[&'static str],
    cx: &mut App,
) -> Div {
    let theme = *cx.tessera_theme();

    options
        .iter()
        .enumerate()
        .fold(v_flex().gap_0p5(), |list, (index, label)| {
            let checked = selected.contains(&index);
            let state = state.clone();
            let row_id = SharedString::from(format!("asset-filter-{}-{index}", kind.id()));

            list.child(
                h_flex()
                    .id(row_id)
                    .h(px(30.0))
                    .w_full()
                    .rounded(theme.radius)
                    .px_2()
                    .text_size(px(13.0))
                    .text_color(theme.text)
                    .cursor_pointer()
                    .hover(move |this| this.bg(theme.border))
                    .child(
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .mr_2()
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(checked, |this| {
                                this.child(
                                    Icon::new(IconName::Check).xsmall().text_color(theme.text),
                                )
                            }),
                    )
                    .child(div().child(*label))
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        state.update(cx, |state, cx| state.toggle(index, cx));
                    }),
            )
        })
}

#[derive(IntoElement)]
struct FilterHeaderTrigger {
    id: ElementId,
    label: SharedString,
    selected_count: usize,
    modified: bool,
    selected: bool,
}

impl FilterHeaderTrigger {
    fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        selected_count: usize,
        modified: bool,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            selected_count,
            modified,
            selected: false,
        }
    }

    /// Highlights the trigger while its popover is open.
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl RenderOnce for FilterHeaderTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let highlighted = self.modified || self.selected;

        h_flex()
            .id(self.id)
            .size_full()
            .items_center()
            .justify_start()
            .gap_1()
            .cursor_pointer()
            .text_color(if highlighted {
                theme.primary_hover
            } else {
                theme.text
            })
            .hover(move |this| this.text_color(theme.primary_hover))
            .child(div().flex_none().child(self.label))
            .child(
                h_flex()
                    .min_w(px(16.0))
                    .h(px(16.0))
                    .px(px(4.0))
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(theme.primary)
                    .text_color(theme.on_primary)
                    .text_size(px(10.0))
                    .line_height(gpui::relative(1.0))
                    .when(!self.modified, |this| this.hidden())
                    .child(self.selected_count.min(99).to_string()),
            )
    }
}
