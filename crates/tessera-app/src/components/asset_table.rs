use std::collections::BTreeSet;
use std::sync::Arc;

use gpui::Action;
use gpui::App;
use gpui::ClickEvent;
use gpui::Context;
use gpui::Div;
use gpui::Entity;
use gpui::FontWeight;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::ParentElement;
use gpui::Rgba;
use gpui::SharedString;
use gpui::Stateful;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder;
use gpui::px;

use serde::Deserialize;

use crate::AssetColumn;
use crate::AssetManager;
use crate::ColumnStatus;
use crate::Sort;
use crate::components::AssetColumnFilter;
use crate::components::AssetFilterKind;
use crate::components::Column;
use crate::components::ColumnSort;
use crate::components::ContextMenuItem;
use crate::components::FilterListState;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::TableDelegate;
use crate::components::TableState;
use crate::components::h_flex;
use crate::components::v_flex;
use crate::palette;
use crate::theme;
use crate::theme::ActiveTesseraTheme;

/// Column data for a single asset row.
type RowInfo = Vec<(String, Option<Rgba>)>;

/// Table delegate that renders assets from an asset manager.
pub struct AssetTableDelegate {
    manager: Arc<dyn AssetManager>,
    asset_columns: Vec<AssetColumn>,
    columns: Vec<Column>,
    type_filter_options: Vec<&'static str>,
    status_filter_options: Vec<&'static str>,
    type_filter: Entity<FilterListState>,
    status_filter: Entity<FilterListState>,
    selection: BTreeSet<usize>,
    loading: bool,
    /// Whether the app is loading, exporting or downloading, which greys out the export entry.
    busy: bool,
    row_cache: Option<(usize, RowInfo)>,
}

/// The actions of the row context menu, as porter-app's menu messages.
#[derive(Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = tessera, no_json)]
pub enum AssetAction {
    /// Exports the given asset.
    Export(usize),
    /// Previews the given asset, raw when the flag is set, opening the preview pane when hidden.
    Preview(usize, bool),
    /// Copies one column's text of the given asset, as (row, column).
    CopyColumn(usize, usize),
    /// Binds the given model row to animation previews. Only offered on rows that are
    /// available models.
    UseAsAnimationPreview(usize),
}

/// Requests made by the table that the app must handle.
#[derive(Debug, Clone)]
pub enum AssetTableRequest {
    /// The selection changed without a plain click, so no preview follows.
    SelectionChanged,
    /// A plain click selected a single row, which should be previewed.
    Selected,
    /// The given asset was double clicked and should be exported.
    ExportAsset(usize),
}

impl gpui::EventEmitter<AssetTableRequest> for TableState<AssetTableDelegate> {}

impl AssetTableDelegate {
    /// Constructs a new asset table delegate.
    pub fn new(
        manager: Arc<dyn AssetManager>,
        asset_columns: Vec<AssetColumn>,
        type_filter_options: Vec<&'static str>,
        status_filter_options: Vec<&'static str>,
        type_filter: Entity<FilterListState>,
        status_filter: Entity<FilterListState>,
    ) -> Self {
        let columns = asset_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let mut result = Column::new(format!("col-{index}"), column.header)
                    .width(px(column.width.clamp(50.0, 1000.0)))
                    .resizable(true);

                if let Some(sort) = column.sort {
                    result = result.sort(match sort {
                        Sort::None => ColumnSort::Default,
                        Sort::Ascending => ColumnSort::Ascending,
                        Sort::Descending => ColumnSort::Descending,
                    });
                }

                result
            })
            .collect();

        Self {
            manager,
            asset_columns,
            columns,
            type_filter_options,
            status_filter_options,
            type_filter,
            status_filter,
            selection: BTreeSet::new(),
            loading: false,
            busy: false,
            row_cache: None,
        }
    }

    /// The current selection.
    pub fn selection(&self) -> &BTreeSet<usize> {
        &self.selection
    }

    /// Replaces the selection with the given row, returning true when it changed.
    pub fn select_single(&mut self, row_ix: usize) -> bool {
        if self.selection.len() == 1 && self.selection.contains(&row_ix) {
            return false;
        }

        self.selection.clear();
        self.selection.insert(row_ix);

        true
    }

    /// Clears the current selection.
    pub fn clear_selection(&mut self) {
        self.selection.clear();
        self.row_cache = None;
    }

    /// Sets whether the app is busy, which greys out the export entry of the context menu.
    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    /// Re-applies the current column sort to freshly loaded rows, as porter-app's `Sort(None)`.
    pub fn reapply_sort(&mut self) {
        let statuses = self
            .asset_columns
            .iter()
            .enumerate()
            .map(|(index, column)| ColumnStatus::new(index, column.sort.unwrap_or(Sort::None)))
            .collect();

        let statuses = self.manager.sort(None, statuses);

        for status in statuses {
            if let Some(column) = self.asset_columns.get_mut(status.index)
                && column.sort.is_some()
            {
                column.sort = Some(status.sort);
            }
        }

        self.row_cache = None;
    }

    /// Sets whether or not the table is loading.
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
        self.row_cache = None;
    }

    /// Invalidates any cached row data.
    pub fn invalidate(&mut self) {
        self.row_cache = None;
    }

    /// Gets the row info for the given index, cached per row while rendering.
    fn row_info(&mut self, row_ix: usize) -> &Vec<(String, Option<Rgba>)> {
        let cached = match self.row_cache.take() {
            Some(cache) if cache.0 == row_ix => cache,
            _ => (row_ix, self.manager.assets_info(row_ix)),
        };
        &self.row_cache.insert(cached).1
    }

    /// Handles selection when a row is clicked, as porter-app does.
    ///
    /// Shift extends the selection from its lowest row without clearing it, the secondary
    /// modifier toggles the row, a plain click selects just the row and previews it, and clicking
    /// the only selected row again does nothing.
    fn handle_click(&mut self, row_ix: usize, event: &ClickEvent) -> AssetTableRequest {
        if event.is_right_click() {
            if !self.selection.contains(&row_ix) {
                self.selection.clear();
                self.selection.insert(row_ix);
            }

            return AssetTableRequest::SelectionChanged;
        }

        if !event.standard_click() {
            return AssetTableRequest::SelectionChanged;
        }

        if event.click_count() == 2 {
            self.selection.clear();
            self.selection.insert(row_ix);

            return AssetTableRequest::ExportAsset(row_ix);
        }

        let modifiers = event.modifiers();

        if modifiers.secondary() {
            if !self.selection.remove(&row_ix) {
                self.selection.insert(row_ix);
            }

            AssetTableRequest::SelectionChanged
        } else if modifiers.shift {
            match self.selection.first().copied() {
                Some(first) if row_ix < first => self.selection.extend(row_ix..first),
                Some(first) if row_ix > first => self.selection.extend(first..=row_ix),
                _ => {
                    if !self.selection.remove(&row_ix) {
                        self.selection.insert(row_ix);
                    }
                }
            }

            AssetTableRequest::SelectionChanged
        } else if self.selection.len() == 1 && self.selection.contains(&row_ix) {
            AssetTableRequest::SelectionChanged
        } else {
            self.selection.clear();
            self.selection.insert(row_ix);

            AssetTableRequest::Selected
        }
    }

    fn filter_column(&self, col_ix: usize) -> Option<AssetFilterKind> {
        match self.asset_columns.get(col_ix).map(|column| column.header) {
            Some("Type") => Some(AssetFilterKind::Type),
            Some("Status") => Some(AssetFilterKind::Status),
            _ => None,
        }
    }

    fn filter_state(&self, column: AssetFilterKind) -> &Entity<FilterListState> {
        match column {
            AssetFilterKind::Type => &self.type_filter,
            AssetFilterKind::Status => &self.status_filter,
        }
    }

    fn filter_options(&self, column: AssetFilterKind) -> &[&'static str] {
        match column {
            AssetFilterKind::Type => &self.type_filter_options,
            AssetFilterKind::Status => &self.status_filter_options,
        }
    }
}

impl TableDelegate for AssetTableDelegate {
    type Action = AssetAction;

    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.manager.assets_visible()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let column_name = self.columns[col_ix].name.clone();

        let Some(filter_column) = self.filter_column(col_ix) else {
            return div().size_full().child(column_name).into_any_element();
        };

        let filter_state = self.filter_state(filter_column).clone();
        let filter_options = self.filter_options(filter_column).to_vec();
        let selected_count = filter_state.read(cx).selection().len();
        let modified = selected_count != filter_options.len();

        h_flex()
            .size_full()
            .items_center()
            .justify_start()
            .child(AssetColumnFilter::new(
                SharedString::from(format!("asset-filter-{col_ix}")),
                column_name,
                filter_column,
                filter_state,
                filter_options,
                selected_count,
                modified,
            ))
            .into_any_element()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let statuses = self
            .asset_columns
            .iter()
            .enumerate()
            .map(|(index, column)| {
                let sort_status = if index == col_ix {
                    match sort {
                        ColumnSort::Default => Sort::None,
                        ColumnSort::Ascending => Sort::Ascending,
                        ColumnSort::Descending => Sort::Descending,
                    }
                } else {
                    column.sort.unwrap_or(Sort::None)
                };

                ColumnStatus::new(index, sort_status)
            })
            .collect();

        let statuses = self.manager.sort(Some(col_ix), statuses);

        for status in statuses {
            if let Some(column) = self.asset_columns.get_mut(status.index)
                && column.sort.is_some()
            {
                column.sort = Some(status.sort);
            }
        }

        self.selection.clear();
        self.row_cache = None;
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let selected = self.selection.contains(&row_ix);

        div()
            .id(("asset-row", row_ix))
            .when(selected, |this| {
                this.bg(palette::with_alpha(cx.tessera_theme().primary, 0.16))
            })
            .on_click(cx.listener(move |table, event: &ClickEvent, _, cx| {
                let request = table.delegate_mut().handle_click(row_ix, event);

                cx.emit(request);
                cx.notify();
            }))
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> Vec<ContextMenuItem<AssetAction>> {
        // Same entries and order as porter-app: export, preview, then one copy entry per column.
        let export = (!self.busy).then_some(AssetAction::Export(row_ix));

        let copy = self
            .asset_columns
            .iter()
            .enumerate()
            .map(|(col_ix, column)| {
                ContextMenuItem::entry(column.header, Some(AssetAction::CopyColumn(row_ix, col_ix)))
            })
            .collect();

        let mut items = vec![
            ContextMenuItem::entry("Export", export).icon(IconName::Download),
            ContextMenuItem::divider(),
            ContextMenuItem::submenu(
                "Preview",
                vec![
                    ContextMenuItem::entry("Asset", Some(AssetAction::Preview(row_ix, false))),
                    ContextMenuItem::entry("Asset (Raw)", Some(AssetAction::Preview(row_ix, true))),
                ],
            )
            .icon(IconName::Box),
            ContextMenuItem::divider(),
            ContextMenuItem::submenu("Copy", copy).icon(IconName::Copy),
        ];

        // Only models can back an animation preview, and the manager is the one that knows which
        // rows those are, so ask it rather than matching on the rendered type column.
        if cfg!(feature = "animations") && self.manager.animation_preview_model_at(row_ix).is_some()
        {
            items.push(ContextMenuItem::divider());
            items.push(
                ContextMenuItem::entry(
                    "Use as Animation Preview",
                    Some(AssetAction::UseAsAnimationPreview(row_ix)),
                )
                .icon(IconName::User),
            );
        }

        items
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let column = self.asset_columns.get(col_ix);
        let column_color = column.and_then(|column| column.color);
        let column_header = column.map(|column| column.header).unwrap_or_default();

        let (value, color) = self
            .row_info(row_ix)
            .get(col_ix)
            .cloned()
            .unwrap_or_default();

        // Colored cells in well-known columns get richer treatments: the type
        // column renders as a tinted pill and the status column as a dot.
        if let Some(color) = color {
            match column_header {
                "Type" => {
                    return h_flex()
                        .size_full()
                        .items_center()
                        .overflow_hidden()
                        .child(
                            // A tinted pill.
                            div()
                                .flex()
                                .items_center()
                                .border_1()
                                .line_height(gpui::relative(1.25))
                                .text_xs()
                                .px_1p5()
                                .py_0p5()
                                .rounded_full()
                                .bg(palette::with_alpha(color, 0.12))
                                .text_color(color)
                                .border_color(palette::with_alpha(color, 0.25))
                                .hover(|this| this.opacity(0.9))
                                .child(value),
                        )
                        .into_any_element();
                }
                "Status" => {
                    return h_flex()
                        .size_full()
                        .items_center()
                        .gap_2()
                        .overflow_hidden()
                        .child(
                            div()
                                .w_1p5()
                                .h_1p5()
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(color),
                        )
                        .child(div().overflow_hidden().text_color(color).child(value))
                        .into_any_element();
                }
                _ => {}
            }
        }

        // Manager-supplied colors are authored against the dark palette, so
        // text-tier colors get remapped to the active theme.
        let color = theme::resolve_cell_color(
            color
                .or(column_color)
                .unwrap_or(palette::TEXT_COLOR_DEFAULT),
            &theme,
        );

        div()
            .size_full()
            .overflow_hidden()
            .text_color(color)
            .child(value)
            .into_any_element()
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let searching = self.manager.assets_total() > 0;

        let (icon, title, hint) = if searching {
            (
                IconName::Search,
                "No matching assets",
                "Try adjusting your search term.",
            )
        } else {
            (
                IconName::Inbox,
                "No assets loaded",
                match (
                    self.manager.supports_files(),
                    self.manager.supports_games(),
                    self.manager.loads_folders(),
                ) {
                    (true, true, true) => {
                        "Load a running instance of a supported game, or a supported game folder, to view and export assets."
                    }
                    (true, true, false) => {
                        "Load a running instance of a supported game, or one of the supported game files, to view and export assets."
                    }
                    (false, true, _) => {
                        "Load a running instance of a supported game to view and export assets."
                    }
                    (true, false, true) => {
                        "Load a supported game folder to view and export assets."
                    }
                    (true, false, false) => {
                        "Load one of the supported game files to view and export assets."
                    }
                    (false, false, _) => "No supported loading mechanisms available.",
                },
            )
        };

        v_flex()
            .size_full()
            .justify_center()
            .items_center()
            .gap_2()
            .child(
                Icon::new(icon)
                    .size_12()
                    .text_color(palette::with_alpha(theme.text_muted, 0.6)),
            )
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text_secondary)
                    .child(title),
            )
            .child(
                div()
                    .max_w(px(420.0))
                    .text_center()
                    .text_size(px(12.5))
                    .text_color(theme.text_muted)
                    .child(hint),
            )
            .into_any_element()
    }

    fn loading(&self, _: &App) -> bool {
        self.loading
    }
}
