use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use gpui::App;
use gpui::AppContext;
use gpui::Bounds;
use gpui::WindowBounds;
use gpui::WindowOptions;
use gpui::px;
use gpui::size;

use crate::AssetColumn;
use crate::AssetManager;
use crate::AssetStatus;
use crate::Controller;
use crate::Settings;
use crate::Sort;
use crate::TesseraApp;
use crate::app::title_case;
use crate::assets::TesseraAssets;
use crate::components::Root;
use crate::components::title_bar;
use crate::palette;
use crate::panic_hook;
use crate::system;
use crate::theme;
use crate::update::UpdateInfo;
use crate::update::UpdateRepository;
use porter_utils::AtomicCancel;

/// State of the active update download.
pub(crate) struct ActiveDownload {
    pub(crate) info: UpdateInfo,
    pub(crate) label: String,
    pub(crate) percent: u32,
    pub(crate) bytes_per_sec: u64,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) failed: Option<String>,
}

/// State of one core asset download session.
pub(crate) struct ActiveAssetDownload {
    pub(crate) name: String,
    pub(crate) percent: u32,
    pub(crate) bytes_per_sec: u64,
    pub(crate) cancel: AtomicCancel,
}

/// A model lookup tied to the preview generation that requested it.
#[derive(Clone, Copy)]
pub(crate) struct AnimationModelRequest {
    pub request_id: u64,
    pub model_id: u64,
}

/// Mutable application state shared by the root view and its workers.
pub struct AppState {
    // Startup configuration, kept inline like porter-app so callers retain the
    // existing fluent builder API without an extra configuration object.
    pub(crate) name: &'static str,
    pub(crate) author: &'static str,
    pub(crate) copyright: &'static str,
    pub(crate) site_url: &'static str,
    pub(crate) donate_url: &'static str,
    pub(crate) version: &'static str,
    pub(crate) description: &'static str,
    /// The GitHub repository whose releases the app updates from, when updates are enabled.
    pub(crate) updates: Option<UpdateRepository>,
    pub(crate) file_filters: Vec<(&'static str, Vec<&'static str>)>,
    pub(crate) files_dropped: Vec<PathBuf>,
    pub(crate) asset_columns: Vec<AssetColumn>,
    pub(crate) asset_type_filter_options: Vec<&'static str>,
    pub(crate) asset_status_filter_options: Vec<&'static str>,

    // Mutable application/runtime state.
    pub(crate) asset_manager: Arc<dyn AssetManager>,
    pub(crate) controller: Controller,
    pub(crate) settings: Settings,
    pub(crate) reload_required: bool,
    pub(crate) loading: bool,
    pub(crate) exporting: bool,
    pub(crate) export_canceled: bool,
    pub(crate) progress: u32,
    pub(crate) last_load: Option<Vec<PathBuf>>,
    pub(crate) show_settings: bool,
    pub(crate) applied_always_on_top: bool,
    pub(crate) preview_request_id: u64,
    pub(crate) animation_bind_request: Option<AnimationModelRequest>,
    /// Set when the pending animation bind came from the row context menu rather than the preview
    /// header's picker. The picker shows its result in the preview itself, but a context menu bind
    /// is invisible until the next animation is previewed, so it needs confirming — and only once
    /// the model has actually decoded.
    pub(crate) animation_bind_announce: bool,
    pub(crate) pending_update: Option<UpdateInfo>,
    pub(crate) update_queue: VecDeque<UpdateInfo>,
    pub(crate) active_download: Option<ActiveDownload>,
    pub(crate) active_asset_download: Option<ActiveAssetDownload>,
    pub(crate) checking_updates: bool,
}

impl AppState {
    /// Constructs a new application state with the given asset manager.
    pub(crate) fn new<T: AssetManager + 'static>(asset_manager: T) -> Self {
        AppState {
            name: "<unset>",
            author: "Tessera Developers",
            copyright: crate::strings::TESSERA_COPYRIGHT,
            site_url: "",
            donate_url: "",
            version: "<unset>",
            description: "<unset>",
            updates: None,
            file_filters: Vec::new(),
            files_dropped: Vec::new(),
            asset_columns: Vec::new(),
            asset_type_filter_options: Vec::new(),
            asset_status_filter_options: default_asset_status_filter_options(),
            asset_manager: Arc::new(asset_manager),
            controller: Controller::new(),
            settings: Settings::default(),
            reload_required: false,
            loading: false,
            exporting: false,
            export_canceled: false,
            progress: 0,
            last_load: None,
            show_settings: false,
            applied_always_on_top: false,
            preview_request_id: 0,
            animation_bind_request: None,
            animation_bind_announce: false,
            pending_update: None,
            update_queue: VecDeque::new(),
            active_download: None,
            active_asset_download: None,
            checking_updates: false,
        }
    }

    /// The name of the application. Used for the main window header and settings/crash files.
    pub const fn name(mut self, name: &'static str) -> Self {
        self.name = name;
        self
    }

    /// The version of the program.
    pub const fn version(mut self, version: &'static str) -> Self {
        self.version = version;
        self
    }

    /// The description of the program.
    pub const fn description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    /// Configures the repository and release archive used for application updates.
    pub const fn updates(
        mut self,
        owner: &'static str,
        repository: &'static str,
        archive: &'static str,
    ) -> Self {
        self.updates = Some(UpdateRepository {
            owner,
            repository,
            archive,
        });
        self
    }

    /// The author of the application.
    pub const fn author(mut self, author: &'static str) -> Self {
        self.author = author;
        self
    }

    /// The copyright of the application.
    pub const fn copyright(mut self, copyright: &'static str) -> Self {
        self.copyright = copyright;
        self
    }

    /// The website of the application.
    pub const fn site_url(mut self, site_url: &'static str) -> Self {
        self.site_url = site_url;
        self
    }

    /// The donation website of the application.
    pub const fn donate_url(mut self, donate_url: &'static str) -> Self {
        self.donate_url = donate_url;
        self
    }

    /// Adds a column to the asset list.
    pub fn column(
        mut self,
        header: &'static str,
        width: usize,
        color: Option<gpui::Rgba>,
        sort: Option<Sort>,
    ) -> Self {
        self.asset_columns
            .push(AssetColumn::new(header, width, color, sort));
        self
    }

    /// Adds the default columns to the asset list.
    pub fn default_columns(self) -> Self {
        self.column("Name", 350, None, None)
            .column("Type", 115, None, None)
            .column("Status", 150, None, None)
            .column("Info", 250, Some(palette::TEXT_COLOR_SECONDARY), None)
    }

    /// Sets the labels shown by the Type column filter.
    pub fn asset_type_filter_options(
        mut self,
        options: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        self.asset_type_filter_options = options.into_iter().collect();
        self
    }

    /// Sets the labels shown by the Status column filter.
    pub fn asset_status_filter_options(
        mut self,
        options: impl IntoIterator<Item = &'static str>,
    ) -> Self {
        self.asset_status_filter_options = options.into_iter().collect();
        self
    }

    /// Adds a file filter to the load files dialog.
    pub fn file_filter(mut self, title: &'static str, extensions: Vec<&'static str>) -> Self {
        self.file_filters.push((title, extensions));
        self
    }

    /// Updates and persists settings, recording whether the loaded assets need reloading.
    pub(crate) fn update_settings(&mut self, update: impl FnOnce(&mut Settings)) {
        let previous = self.settings.clone();
        update(&mut self.settings);

        if !self.reload_required {
            self.reload_required = previous.reload_required(&self.settings);
        }

        self.settings.save(self.name);
    }

    /// Runs the app until the main window is closed.
    pub fn run(mut self) {
        panic_hook::install(self.name, self.version);

        self.settings = Settings::load(self.name);

        porter_threads::initialize_thread_pool();
        system::initialize_workarounds();

        gpui::Application::new()
            .with_assets(TesseraAssets)
            .run(move |cx: &mut App| {
                crate::components::bind_keys(cx);
                theme::apply(self.settings.theme(), cx);

                let window_size = size(px(920.0), px(582.0));
                let bounds = Bounds::centered(None, window_size, cx);

                let title = format!("{} v{}", title_case(self.name), self.version);

                let options = WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(window_size),
                    titlebar: Some(title_bar::title_bar_options(title)),
                    ..Default::default()
                };

                let mut state = Some(self);
                cx.open_window(options, move |window, cx| {
                    let state = state.take().expect("application state already consumed");
                    let view = cx.new(|cx| TesseraApp::new(state, window, cx));

                    cx.new(|_| Root::new(view))
                })
                .expect("failed to open the main window");

                cx.activate(true);
            });
    }
}

fn default_asset_status_filter_options() -> Vec<&'static str> {
    let statuses = AssetStatus::ALL;
    statuses.iter().map(|status| status.as_str()).collect()
}
