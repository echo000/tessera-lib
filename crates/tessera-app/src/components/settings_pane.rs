use std::collections::BTreeSet;

use gpui::AnyElement;
use gpui::App;
use gpui::AppContext as _;
use gpui::Context;
use gpui::Div;
use gpui::ElementId;
use gpui::Entity;
use gpui::FontWeight;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::ParentElement;
use gpui::Point;
use gpui::ScrollHandle;
use gpui::SharedString;
use gpui::Stateful;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::Subscription;
use gpui::Transformation;
use gpui::Window;
use gpui::div;
use gpui::percentage;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;
use gpui::rems;

#[cfg(feature = "animations")]
use porter_animation::AnimationFileType;
#[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
use porter_audio::AudioFileType;
use porter_model::ModelFileType;
use porter_texture::ImageFileType;
use tessera_viewport::PreviewControlScheme;

use crate::TesseraApp;
use crate::components::Button;
use crate::components::Checkbox;
use crate::components::Disableable;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Input;
use crate::components::InputState;
use crate::components::Notification;
use crate::components::Scrollbar;
use crate::components::Select;
use crate::components::Sizable;
use crate::components::Slider;
use crate::components::Switch;
use crate::components::WindowExt;
use crate::components::h_flex;
use crate::components::v_flex;
use crate::palette;
#[cfg(feature = "normal-maps-convertible")]
use crate::settings::ImageNormalMapProcessing;
use crate::settings::ModelMaterialProcessing;
use crate::settings::Settings as AppSettings;
#[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
use crate::settings::SoundPathProcessing;
use crate::system;
use crate::theme::ActiveTesseraTheme;
use crate::theme::TesseraTheme;

/// Bounds of the preview's far clipping distance, matching what the setting itself clamps to.
const FAR_CLIP_MIN: u32 = 10_000;
const FAR_CLIP_MAX: u32 = 1_000_000;
/// The range spans two orders of magnitude, so the slider moves in steps rather than units.
const FAR_CLIP_STEP: u32 = 10_000;
/// The width of the settings sidebar.
const SIDEBAR_WIDTH: f32 = 251.0;
/// The most characters the custom scale field holds, as in porter-app.
const CUSTOM_SCALE_MAX_LENGTH: usize = 8;

/// Image export formats, as (key, label).
const IMAGE_FORMATS: &[(&str, &str)] = &[
    ("dds", "DDS"),
    ("png", "PNG"),
    ("tiff", "TIFF"),
    ("tga", "TGA"),
];

/// Model material image placements, as (key, label).
const MODEL_MATERIAL_PLACEMENTS: &[(&str, &str)] = &[
    ("material_folder", "In material folders"),
    ("model_folder", "In the model folder"),
    ("skip", "Skip"),
];

/// Preview camera control schemes, as (key, label).
const PREVIEW_CONTROLS: &[(&str, &str)] = &[
    ("simple", "Simple"),
    ("maya", "Maya"),
    ("blender", "Blender"),
    ("unreal", "Unreal"),
];

/// Sound export folder layouts, as (key, label).
#[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
const SOUND_PATHS: &[(&str, &str)] = &[
    ("file_name", "In one folder"),
    ("folder_tree", "In the asset's own folders"),
];

/// Normal map conventions, as (key, label).
#[cfg(feature = "normal-maps-convertible")]
const NORMAL_MAPS: &[(&str, &str)] = &[
    ("none", "None"),
    ("opengl", "OpenGL"),
    ("directx", "DirectX"),
];

/// The control of a setting row, bound to the app settings.
#[derive(Clone, Copy)]
enum Field {
    /// A switch bound to a boolean setting.
    Switch(fn(&AppSettings) -> bool, fn(&mut AppSettings, bool)),
    /// A switch for one of the load settings, shown off and disabled while every asset is
    /// treated as a raw file.
    LoadSwitch(fn(&AppSettings) -> bool, fn(&mut AppSettings, bool)),
    /// The load raw files switch, disabled while every asset is treated as a raw file.
    #[cfg(feature = "raw-files")]
    LoadRawSwitch,
    /// The custom model scale switch, enabled only while assets are scaled on export.
    CustomScaleSwitch,
    /// A check box bound to a boolean setting.
    #[cfg(feature = "worlds")]
    Checkbox(fn(&AppSettings) -> bool, fn(&mut AppSettings, bool)),
    /// A dropdown over (key, label) options bound to a keyed setting.
    Select(
        &'static [(&'static str, &'static str)],
        fn(&AppSettings) -> &'static str,
        fn(&mut AppSettings, &str),
    ),
    /// A check box enabling a model export format.
    ModelFormat(ModelFileType),
    /// A check box enabling an animation export format.
    #[cfg(feature = "animations")]
    AnimFormat(AnimationFileType),
    /// A check box enabling an audio export format.
    #[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
    AudioFormat(AudioFileType),
    /// The preview far clip slider.
    FarClip,
    /// The preview audio volume slider.
    #[cfg(feature = "sounds")]
    Volume,
    /// The custom model scale text field.
    CustomScale,
    /// The manual update check button.
    CheckUpdates,
    /// A button opening a url, as (id, label, url).
    Link(&'static str, &'static str, &'static str),
}

impl Field {
    /// The value of a switch or check box field.
    fn checked(self, settings: &AppSettings) -> Option<bool> {
        match self {
            Field::Switch(get, _) => Some(get(settings)),
            Field::LoadSwitch(get, _) => Some(get(settings) && !force_raw_files(settings)),
            #[cfg(feature = "raw-files")]
            Field::LoadRawSwitch => Some(settings.load_raw_files()),
            Field::CustomScaleSwitch => Some(settings.custom_scale().is_some()),
            #[cfg(feature = "worlds")]
            Field::Checkbox(get, _) => Some(get(settings)),
            Field::ModelFormat(file_type) => Some(settings.model_file_types().contains(&file_type)),
            #[cfg(feature = "animations")]
            Field::AnimFormat(file_type) => Some(settings.anim_file_types().contains(&file_type)),
            #[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
            Field::AudioFormat(file_type) => Some(settings.audio_file_types().contains(&file_type)),
            _ => None,
        }
    }

    /// Sets the value of a switch or check box field.
    fn set_checked(self, settings: &mut AppSettings, value: bool) {
        match self {
            Field::Switch(_, set) | Field::LoadSwitch(_, set) => set(settings, value),
            #[cfg(feature = "raw-files")]
            Field::LoadRawSwitch => settings.set_load_raw_files(value),
            Field::CustomScaleSwitch => settings.set_custom_scale(value.then_some(1.0)),
            #[cfg(feature = "worlds")]
            Field::Checkbox(_, set) => set(settings, value),
            Field::ModelFormat(file_type) => settings.set_model_file_type(file_type, value),
            #[cfg(feature = "animations")]
            Field::AnimFormat(file_type) => settings.set_anim_file_type(file_type, value),
            #[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
            Field::AudioFormat(file_type) => settings.set_audio_file_type(file_type, value),
            _ => {}
        }
    }

    /// Whether the field renders as a switch rather than a check box.
    fn is_switch(self) -> bool {
        match self {
            Field::Switch(..) | Field::LoadSwitch(..) | Field::CustomScaleSwitch => true,
            #[cfg(feature = "raw-files")]
            Field::LoadRawSwitch => true,
            _ => false,
        }
    }

    /// Whether the field's control can't be used in the current settings.
    fn disabled(self, settings: &AppSettings) -> bool {
        match self {
            Field::LoadSwitch(..) => force_raw_files(settings),
            #[cfg(feature = "raw-files")]
            Field::LoadRawSwitch => force_raw_files(settings),
            Field::CustomScaleSwitch => !settings.auto_scale(),
            _ => false,
        }
    }

    /// Whether the field differs from its default, for the fields a page reset covers.
    fn is_modified(self, settings: &AppSettings, defaults: &AppSettings) -> bool {
        if let Field::Select(_, get, _) = self {
            return get(settings) != get(defaults);
        }

        self.checked(settings)
            .zip(self.checked(defaults))
            .is_some_and(|(value, default)| value != default)
    }

    /// Restores the field's default, for the fields a page reset covers.
    fn reset(self, settings: &mut AppSettings, defaults: &AppSettings) {
        if let Field::Select(_, get, set) = self {
            set(settings, get(defaults));
        } else if let Some(default) = self.checked(defaults) {
            self.set_checked(settings, default);
        }
    }
}

/// An entry of a settings group.
enum Item {
    /// A labelled row with a control.
    Row {
        title: &'static str,
        description: Option<&'static str>,
        field: Field,
    },
    /// The export directory picker.
    ExportDirectory,
    /// The line explaining why the load switches are disabled.
    ForceRawNotice,
    /// The config folder controls.
    Troubleshooting,
    /// The app identity and credits.
    About,
}

impl Item {
    fn row(title: &'static str, description: &'static str, field: Field) -> Self {
        Item::Row {
            title,
            description: Some(description),
            field,
        }
    }

    fn plain(title: &'static str, field: Field) -> Self {
        Item::Row {
            title,
            description: None,
            field,
        }
    }

    /// Whether the item matches a lowercase search; custom items only show while not searching.
    fn is_match(&self, query: &str) -> bool {
        match self {
            Item::Row {
                title, description, ..
            } => {
                title.to_lowercase().contains(query)
                    || description
                        .is_some_and(|description| description.to_lowercase().contains(query))
            }
            _ => query.is_empty(),
        }
    }
}

/// A group of settings, titled unless it is the only one of its page.
struct Group {
    title: Option<&'static str>,
    items: Vec<Item>,
}

impl Group {
    fn new(title: &'static str) -> Self {
        Self {
            title: Some(title),
            items: Vec::new(),
        }
    }

    fn untitled() -> Self {
        Self {
            title: None,
            items: Vec::new(),
        }
    }

    fn item(mut self, item: Item) -> Self {
        self.items.push(item);
        self
    }
}

/// A page of the settings sidebar.
struct Page {
    title: &'static str,
    icon: Option<IconName>,
    resettable: bool,
    groups: Vec<Group>,
}

impl Page {
    fn new(title: &'static str) -> Self {
        Self {
            title,
            icon: None,
            resettable: true,
            groups: Vec::new(),
        }
    }

    fn group(mut self, group: Group) -> Self {
        self.groups.push(group);
        self
    }

    /// The groups with an item matching the search.
    fn visible_groups(&self, query: &str) -> Vec<usize> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(_, group)| group.items.iter().any(|item| item.is_match(query)))
            .map(|(index, _)| index)
            .collect()
    }

    /// The fields matching the search.
    fn fields(&self, query: &str) -> Vec<Field> {
        self.groups
            .iter()
            .flat_map(|group| group.items.iter())
            .filter(|item| item.is_match(query))
            .filter_map(|item| match item {
                Item::Row { field, .. } => Some(*field),
                _ => None,
            })
            .collect()
    }
}

/// Which page and group the settings view shows, kept while it stays open.
struct SettingsNav {
    search: Entity<InputState>,
    page: usize,
    group: Option<usize>,
    /// Pages whose group list is expanded in the sidebar.
    expanded: BTreeSet<usize>,
    scroll: ScrollHandle,
    _search_changed: Subscription,
}

/// Holds the input state for the custom scale text field.
struct ScaleFieldState {
    input: Entity<InputState>,
    last: Option<f32>,
}

impl TesseraApp {
    /// Renders the settings view: a page list beside the selected page.
    pub(crate) fn render_settings(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let pages = self.settings_pages();
        let nav = window.use_keyed_state("tessera-settings", cx, |window, cx| {
            let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search..."));
            let search_changed = cx.subscribe(&search, |_: &mut SettingsNav, _, _, cx| {
                cx.notify();
            });

            SettingsNav {
                search,
                page: 0,
                group: None,
                expanded: BTreeSet::from([0]),
                scroll: ScrollHandle::new(),
                _search_changed: search_changed,
            }
        });

        let query = nav.read(cx).search.read(cx).value().to_lowercase();
        let visible = pages
            .iter()
            .map(|page| page.visible_groups(&query))
            .collect::<Vec<_>>();

        // Keep the selection on a page the search still shows.
        let selected = nav.read(cx).page;

        if visible.get(selected).is_none_or(|groups| groups.is_empty())
            && let Some(page) = visible.iter().position(|groups| !groups.is_empty())
        {
            nav.update(cx, |nav, _| {
                nav.page = page;
                nav.group = None;
            });
        }

        let sidebar = render_sidebar(&nav, &pages, &visible, cx);
        let page = self.render_settings_page(&nav, &pages, &visible, &query, window, cx);

        h_flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .map(|mut this| {
                this.style().align_items = Some(gpui::AlignItems::Stretch);
                this
            })
            .child(sidebar)
            .child(page)
    }

    /// Renders the selected settings page: its title over the scrolling groups.
    fn render_settings_page(
        &self,
        nav: &Entity<SettingsNav>,
        pages: &[Page],
        visible: &[Vec<usize>],
        query: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let app = self;
        let view = &cx.entity();
        let theme = *cx.tessera_theme();
        let (page_ix, scroll) = {
            let nav = nav.read(cx);
            (nav.page, nav.scroll.clone())
        };

        let Some(page) = pages.get(page_ix) else {
            return div().flex_1().into_any_element();
        };

        let fields = page.fields(query);
        let defaults = AppSettings::default();
        let resettable = page.resettable && {
            let settings = &app.state.settings;

            fields
                .iter()
                .any(|field| field.is_modified(settings, &defaults))
        };

        let mut groups = v_flex()
            .id(("settings-groups", page_ix))
            .size_full()
            .px_4()
            .overflow_y_scroll()
            .track_scroll(&scroll);

        for group_ix in visible.get(page_ix).into_iter().flatten().copied() {
            let group = &page.groups[group_ix];
            let mut rows = v_flex()
                .border_1()
                .border_color(theme.border_strong)
                .p_4()
                .gap_4()
                .rounded(theme.radius);

            for (item_ix, item) in group.items.iter().enumerate() {
                if item.is_match(query) {
                    let id = (page_ix * 100 + group_ix) * 100 + item_ix;

                    rows = rows.child(render_item(app, view, item, id, window, cx));
                }
            }

            groups = groups.child(
                v_flex()
                    .w_full()
                    .py_4()
                    .gap_4()
                    .when_some(group.title, |this, title| {
                        this.child(
                            div()
                                .text_color(theme.text_secondary)
                                .line_height(relative(1.25))
                                .child(title),
                        )
                    })
                    .child(rows),
            );
        }

        let reset = resettable.then(|| {
            let view = view.clone();

            Button::new("reset")
                .icon(IconName::Undo2)
                .ghost()
                .small()
                .tooltip("Reset All")
                .on_click(move |_, _, cx| {
                    let fields = fields.clone();

                    view.update(cx, |app, cx| {
                        let defaults = AppSettings::default();

                        app.state.update_settings(|settings| {
                            for field in fields {
                                field.reset(settings, &defaults);
                            }
                        });
                        cx.notify();
                    });
                })
        });

        v_flex()
            .id(("settings-page", page_ix))
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                v_flex()
                    .p_4()
                    .gap_3()
                    .border_b_1()
                    .border_color(theme.border_strong)
                    .child(h_flex().justify_between().child(page.title).children(reset)),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .child(groups)
                    .child(Scrollbar::vertical("settings-scrollbar", &scroll)),
            )
            .into_any_element()
    }

    /// The settings pages, as enabled by the build features.
    fn settings_pages(&self) -> Vec<Page> {
        let load_group = Group::new("Load Settings").item(Item::row(
            "Load models",
            "Load and display model assets.",
            Field::LoadSwitch(AppSettings::load_models, AppSettings::set_load_models),
        ));

        #[cfg(feature = "animations")]
        let load_group = load_group.item(Item::row(
            "Load animations",
            "Load and display animation assets.",
            Field::LoadSwitch(
                AppSettings::load_animations,
                AppSettings::set_load_animations,
            ),
        ));

        let load_group = load_group.item(Item::row(
            "Load images",
            "Load and display image assets.",
            Field::LoadSwitch(AppSettings::load_images, AppSettings::set_load_images),
        ));

        #[cfg(feature = "materials")]
        let load_group = load_group.item(Item::row(
            "Load materials",
            "Load and display material assets.",
            Field::LoadSwitch(AppSettings::load_materials, AppSettings::set_load_materials),
        ));

        #[cfg(feature = "sounds")]
        let load_group = load_group.item(Item::row(
            "Load sounds",
            "Load and display sound assets.",
            Field::LoadSwitch(AppSettings::load_sounds, AppSettings::set_load_sounds),
        ));

        #[cfg(feature = "worlds")]
        let load_group = load_group.item(Item::row(
            "Load maps",
            "Load and display map assets.",
            Field::LoadSwitch(AppSettings::load_maps, AppSettings::set_load_maps),
        ));

        #[cfg(feature = "raw-files")]
        let load_group = load_group.item(Item::row(
            "Load raw files",
            "Load and display raw file assets.",
            Field::LoadRawSwitch,
        ));

        // The load switches are disabled while everything loads raw, and say so.
        let load_group = if force_raw_files(&self.state.settings) {
            load_group.item(Item::ForceRawNotice)
        } else {
            load_group
        };

        // Built ahead of the page so the audio item can be left out of a build without sound
        // support without the far-clip one going with it.
        let preview_group = Group::new("Preview")
            .item(Item::row(
                "Far clip distance",
                "Set the preview far clip distance (May impact performance).",
                Field::FarClip,
            ))
            .item(Item::row(
                "Camera controls",
                "Unreal: hold right mouse to look, fly with WASD/QE and scroll to change speed.",
                Field::Select(
                    PREVIEW_CONTROLS,
                    |settings| preview_controls_key(settings.preview_controls()),
                    |settings, key| {
                        settings.set_preview_controls(match key {
                            "maya" => PreviewControlScheme::Maya,
                            "blender" => PreviewControlScheme::Blender,
                            "unreal" => PreviewControlScheme::Unreal,
                            _ => PreviewControlScheme::Simple,
                        });
                    },
                ),
            ))
            .item(Item::row(
                "Show controls overlay",
                "Show the viewport toggle buttons over the preview.",
                Field::Switch(
                    AppSettings::preview_overlay,
                    AppSettings::set_preview_overlay,
                ),
            ))
            .item(Item::row(
                "Auto focus on model",
                "Focus the camera on a model when it is previewed.",
                Field::Switch(
                    AppSettings::auto_focus_on_model,
                    AppSettings::set_auto_focus_on_model,
                ),
            ));

        #[cfg(feature = "sounds")]
        let preview_group = preview_group.item(Item::row(
            "Audio volume",
            "Volume percentage for audio preview (limited for safety).",
            Field::Volume,
        ));

        let general = Page::new("General")
            .group(load_group)
            .group(
                Group::new("Export")
                    .item(Item::row(
                        "Skip previously exported assets",
                        "Assets that already exist in the export directory are not exported again.",
                        Field::Switch(
                            AppSettings::skip_previously_exported,
                            AppSettings::set_skip_previously_exported,
                        ),
                    ))
                    .item(Item::ExportDirectory),
            )
            .group(preview_group);

        let models = Page::new("Models")
            .group(
                Group::new("Export options")
                    .item(Item::row(
                        "Export all available LODs",
                        "Export every level of detail instead of only the highest.",
                        Field::Switch(AppSettings::export_lods, AppSettings::set_export_lods),
                    ))
                    .item(Item::row(
                        "Build blend shapes",
                        "Build blend shapes for supported games.",
                        Field::Switch(
                            AppSettings::build_blend_shapes,
                            AppSettings::set_build_blend_shapes,
                        ),
                    ))
                    .item(Item::row(
                        "Model material images",
                        "Where each material's images are written, or skip them entirely.",
                        Field::Select(
                            MODEL_MATERIAL_PLACEMENTS,
                            |settings| model_material_key(settings.model_material_processing()),
                            |settings, key| {
                                settings.set_model_material_processing(match key {
                                    "model_folder" => ModelMaterialProcessing::InModelFolder,
                                    "skip" => ModelMaterialProcessing::Skip,
                                    _ => ModelMaterialProcessing::InMaterialFolder,
                                });
                            },
                        ),
                    ))
                    .item(Item::row(
                        "Custom model scale",
                        "Apply a custom scale factor to exported model assets.",
                        Field::CustomScaleSwitch,
                    ))
                    .item(Item::row(
                        "Custom scale factor",
                        "Only applied while custom model scale is enabled.",
                        Field::CustomScale,
                    ))
                    .item(Item::row(
                        "Scale exported assets",
                        "Scale models, animations and worlds on export, using the custom \
                         scale factor when one is set.",
                        Field::Switch(AppSettings::auto_scale, AppSettings::set_auto_scale),
                    )),
            )
            .group(
                Group::new("Formats")
                    .item(Item::plain("Cast", Field::ModelFormat(ModelFileType::Cast)))
                    .item(Item::plain("OBJ", Field::ModelFormat(ModelFileType::Obj)))
                    .item(Item::plain(
                        "Valve SMD",
                        Field::ModelFormat(ModelFileType::Smd),
                    ))
                    .item(Item::plain(
                        "XNALara",
                        Field::ModelFormat(ModelFileType::XnaLara),
                    ))
                    .item(Item::plain(
                        "CoD XModel Export",
                        Field::ModelFormat(ModelFileType::XModelExport),
                    ))
                    .item(Item::plain(
                        "Autodesk Maya ASCII",
                        Field::ModelFormat(ModelFileType::Maya),
                    ))
                    .item(Item::plain("FBX", Field::ModelFormat(ModelFileType::Fbx))),
            );

        let images_group = Group::new("Export").item(Item::row(
            "Image format",
            "DDS and PNG are lossless and recommended. TIFF may have compatibility issues. TGA may be lossy or take more space.",
            Field::Select(
                IMAGE_FORMATS,
                |settings| image_format_key(settings.image_file_type()),
                |settings, key| {
                    settings.set_image_file_type(match key {
                        "dds" => ImageFileType::Dds,
                        "tiff" => ImageFileType::Tiff,
                        "tga" => ImageFileType::Tga,
                        _ => ImageFileType::Png,
                    });
                },
            ),
        ));

        #[cfg(feature = "normal-maps-convertible")]
        let images_group = images_group.item(Item::row(
            "Normal map conversion",
            "Convert normal maps to the selected convention.",
            Field::Select(
                NORMAL_MAPS,
                |settings| normal_map_key(settings.image_normal_map_processing()),
                |settings, key| {
                    settings.set_image_normal_map_processing(match key {
                        "opengl" => ImageNormalMapProcessing::OpenGl,
                        "directx" => ImageNormalMapProcessing::DirectX,
                        _ => ImageNormalMapProcessing::None,
                    });
                },
            ),
        ));

        let mut pages = vec![general, models, Page::new("Images").group(images_group)];

        #[cfg(feature = "materials")]
        pages.push(
            Page::new("Materials").group(
                Group::new("Export")
                    .item(Item::row(
                        "Export material info",
                        "Write a description of each material next to the exported model.",
                        Field::Switch(
                            AppSettings::export_material_info,
                            AppSettings::set_export_material_info,
                        ),
                    ))
                    .item(Item::row(
                        "Strip directory from material path",
                        "Only keep the material file name in exported material paths.",
                        Field::Switch(
                            AppSettings::strip_material_dir,
                            AppSettings::set_strip_material_dir,
                        ),
                    )),
            ),
        );

        #[cfg(feature = "animations")]
        {
            let animations = Page::new("Animations").group(Group::new("Formats").item(
                Item::plain("Cast", Field::AnimFormat(AnimationFileType::Cast)),
            ));

            #[cfg(feature = "animations-bake")]
            let animations = animations.group(Group::new("Export").item(Item::row(
                "Bake animations",
                "Request baked animation exports from backends that support baking.",
                Field::Switch(AppSettings::anim_bake, AppSettings::set_anim_bake),
            )));

            pages.push(animations);
        }

        #[cfg(feature = "worlds")]
        pages.push(
            Page::new("Maps").group(Group::new("Formats").item(Item::plain(
                "Cast",
                Field::Checkbox(
                    AppSettings::map_export_cast,
                    AppSettings::set_map_export_cast,
                ),
            ))),
        );

        #[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
        pages.push(
            Page::new("Audio")
                .group(
                    Group::new("Formats")
                        .item(Item::plain("WAV", Field::AudioFormat(AudioFileType::Wav)))
                        .item(Item::plain("FLAC", Field::AudioFormat(AudioFileType::Flac))),
                )
                .group(Group::new("Export").item(Item::row(
                    "Folders",
                    "Sounds are named by the path they had in the game. Keep that structure, \
                     or put every sound in one folder.",
                    Field::Select(
                        SOUND_PATHS,
                        |settings| sound_path_key(settings.sound_path_processing()),
                        |settings, key| {
                            settings.set_sound_path_processing(match key {
                                "folder_tree" => SoundPathProcessing::FolderTree,
                                _ => SoundPathProcessing::FileName,
                            });
                        },
                    ),
                ))),
        );

        let advanced = Page::new("Advanced")
            .group(
                Group::new("Behavior")
                    .item(Item::row(
                        "Log assets",
                        "Write a log of loaded assets to disk.",
                        Field::Switch(AppSettings::log_assets, AppSettings::set_log_assets),
                    ))
                    .item(Item::row(
                        "Always on top",
                        "Keep the window above all other windows.",
                        Field::Switch(AppSettings::always_on_top, AppSettings::set_always_on_top),
                    )),
            )
            .group(Group::new("Troubleshooting").item(Item::Troubleshooting));

        #[cfg(feature = "raw-files-forcible")]
        let advanced = advanced.group(Group::new("Raw assets").item(Item::row(
            "Force raw files",
            "Request raw loading for every asset. Requires a reload and backend support.",
            Field::Switch(
                AppSettings::force_raw_files,
                AppSettings::set_force_raw_files,
            ),
        )));

        pages.push(advanced);

        let mut about = Page::new("About").group(Group::untitled().item(Item::About));

        about.icon = Some(IconName::Info);
        about.resettable = false;

        if self.state.updates.is_some() {
            about = about.group(Group::new("Updates").item(Item::row(
                "Check for updates",
                "Check the configured repository for a newer app version.",
                Field::CheckUpdates,
            )));
        }

        if !self.state.site_url.is_empty() {
            about = about.group(Group::new("Website").item(Item::plain(
                "Website",
                Field::Link("about-site", "Website...", self.state.site_url),
            )));
        }

        if !self.state.donate_url.is_empty() {
            about = about.group(Group::new("Support").item(Item::plain(
                "Donate",
                Field::Link("about-donate", "Donate...", self.state.donate_url),
            )));
        }

        pages.push(about);
        pages
    }
}

/// Renders the settings sidebar: the search box over the page list.
fn render_sidebar(
    nav: &Entity<SettingsNav>,
    pages: &[Page],
    visible: &[Vec<usize>],
    cx: &mut App,
) -> AnyElement {
    let theme = *cx.tessera_theme();
    let (search, selected_page, selected_group, expanded) = {
        let nav = nav.read(cx);
        (
            nav.search.clone(),
            nav.page,
            nav.group,
            nav.expanded.clone(),
        )
    };

    let mut menu = v_flex().gap_2().pt_3().pb_3();

    for (page_ix, page) in pages.iter().enumerate() {
        let groups = &visible[page_ix];

        if groups.is_empty() {
            continue;
        }

        let has_submenu = groups.len() > 1;
        let open = has_submenu && expanded.contains(&page_ix);
        let active = selected_page == page_ix && (selected_group.is_none() || groups.len() == 1);

        let item = sidebar_item(
            ("settings-page", page_ix),
            page.icon,
            page.title,
            active,
            &theme,
        )
        .when(has_submenu, |this| {
            let nav = nav.clone();

            // The caret expands or collapses the group list without selecting the page.
            this.child(
                div()
                    .id(("settings-page-caret", page_ix))
                    .flex()
                    .items_center()
                    .justify_center()
                    .flex_shrink_0()
                    .size_5()
                    .rounded(theme.radius)
                    .hover(move |this| this.bg(palette::with_alpha(theme.border, 0.5)))
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        nav.update(cx, |nav, cx| {
                            if !nav.expanded.remove(&page_ix) {
                                nav.expanded.insert(page_ix);
                            }
                            cx.notify();
                        });
                    })
                    .child(
                        Icon::new(IconName::ChevronRight)
                            .xsmall()
                            .when(open, |this| {
                                this.transform(Transformation::rotate(percentage(0.25)))
                            }),
                    ),
            )
        })
        .on_click({
            let nav = nav.clone();
            move |_, _, cx| {
                nav.update(cx, |nav, cx| {
                    nav.page = page_ix;
                    nav.group = None;
                    nav.expanded.insert(page_ix);
                    nav.scroll.set_offset(Point::default());
                    cx.notify();
                });
            }
        });

        let mut entry = div().w_full().child(item);

        if open {
            let mut submenu = v_flex()
                .border_l_1()
                .border_color(theme.sidebar_border)
                .gap_1()
                .ml_3p5()
                .pl_2p5()
                .py_0p5();

            for (position, group_ix) in groups.iter().copied().enumerate() {
                let Some(title) = page.groups[group_ix].title else {
                    continue;
                };

                let active = selected_page == page_ix && selected_group == Some(group_ix);
                let nav = nav.clone();

                submenu = submenu.child(
                    sidebar_item(
                        ("settings-group", page_ix * 100 + group_ix),
                        None,
                        title,
                        active,
                        &theme,
                    )
                    .on_click(move |_, _, cx| {
                        nav.update(cx, |nav, cx| {
                            nav.page = page_ix;
                            nav.group = Some(group_ix);
                            nav.scroll.scroll_to_item(position);
                            cx.notify();
                        });
                    }),
                );
            }

            entry = entry.child(submenu);
        }

        menu = menu.child(entry);
    }

    v_flex()
        .id("settings-sidebar")
        .flex_shrink_0()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .overflow_hidden()
        .bg(theme.sidebar)
        .border_r_1()
        .border_color(theme.sidebar_border)
        .child(
            h_flex()
                .pt_3()
                .px_3()
                .gap_2()
                .child(Input::new(&search).prefix(Icon::new(IconName::Search))),
        )
        .child(
            v_flex()
                .id("settings-sidebar-content")
                .flex_1()
                .min_h_0()
                .px_3()
                .overflow_y_scroll()
                .child(menu),
        )
        .into_any_element()
}

/// A sidebar entry, highlighted while active.
fn sidebar_item(
    id: impl Into<ElementId>,
    icon: Option<IconName>,
    label: &'static str,
    active: bool,
    theme: &TesseraTheme,
) -> Stateful<Div> {
    let accent = theme.sidebar_accent;

    h_flex()
        .id(id)
        .w_full()
        .h_7()
        .overflow_x_hidden()
        .flex_shrink_0()
        .p_2()
        .gap_x_2()
        .rounded(theme.radius)
        .text_sm()
        .map(|this| {
            if active {
                this.font_weight(FontWeight::MEDIUM).bg(accent)
            } else {
                this.hover(move |this| this.bg(palette::with_alpha(accent, 0.8)))
            }
        })
        .when_some(icon, |this, icon| this.child(Icon::new(icon).size_4()))
        .child(h_flex().flex_1().overflow_x_hidden().child(label))
}

/// Renders a settings group entry.
fn render_item(
    app: &TesseraApp,
    view: &Entity<TesseraApp>,
    item: &Item,
    id: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = *cx.tessera_theme();

    match item {
        Item::Row {
            title,
            description,
            field,
        } => h_flex()
            .id(("settings-item", id))
            .w_full()
            .justify_between()
            .items_center()
            .gap_3()
            .child(
                v_flex()
                    .flex_1()
                    .max_w(relative(0.6))
                    .child(div().text_sm().line_height(rems(1.25)).child(*title))
                    .when_some(*description, |this, description| {
                        this.child(
                            div()
                                .size_full()
                                .text_sm()
                                .text_color(theme.text_secondary)
                                .child(description),
                        )
                    }),
            )
            .child(render_field(app, view, *field, id, window, cx))
            .into_any_element(),
        Item::ExportDirectory => render_export_directory(app, view, cx),
        Item::ForceRawNotice => div()
            .text_sm()
            .text_color(theme.warning)
            .child("(Treat all assets as raw files is enabled)")
            .into_any_element(),
        Item::Troubleshooting => h_flex()
            .gap_2()
            .items_center()
            .child(
                Button::new("reset-settings")
                    .label("Reset Settings")
                    .outline()
                    .small()
                    .on_click({
                        let view = view.clone();

                        move |_, _, cx| {
                            view.update(cx, |app, cx| {
                                app.state
                                    .update_settings(|settings| *settings = AppSettings::default());
                                crate::theme::apply(app.state.settings.theme(), cx);
                                cx.notify();
                            });
                        }
                    }),
            )
            .child(
                Button::new("open-config-folder")
                    .label("Open Config Folder")
                    .outline()
                    .small()
                    .on_click(|_, _, _| {
                        if let Some(project_directory) =
                            directories::ProjectDirs::from("com", "", "Tessera")
                        {
                            system::open_folder(project_directory.config_dir());
                        }
                    }),
            )
            .into_any_element(),
        Item::About => render_about(app, cx),
    }
}

/// Renders the control of a setting row.
fn render_field(
    app: &TesseraApp,
    view: &Entity<TesseraApp>,
    field: Field,
    id: usize,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let theme = *cx.tessera_theme();
    let settings = &app.state.settings;

    if let Some(checked) = field.checked(settings) {
        let view = view.clone();
        let on_change = move |value, _: &mut Window, cx: &mut App| {
            view.update(cx, |app, cx| {
                app.state
                    .update_settings(|settings| field.set_checked(settings, value));
                cx.notify();
            });
        };

        return if field.is_switch() {
            Switch::new(("settings-switch", id), checked, on_change)
                .disabled(field.disabled(settings))
                .into_any_element()
        } else {
            Checkbox::new(("settings-check", id), checked, on_change).into_any_element()
        };
    }

    match field {
        Field::Select(options, get, set) => {
            let value = get(settings);
            let view = view.clone();

            Select::new(
                ("settings-select", id),
                options
                    .iter()
                    .map(|(key, label)| (SharedString::from(*key), SharedString::from(*label)))
                    .collect(),
                value,
                move |key, _, cx| {
                    view.update(cx, |app, cx| {
                        app.state.update_settings(|settings| set(settings, &key));
                        cx.notify();
                    });
                },
            )
            .into_any_element()
        }
        Field::FarClip => {
            let far_clip = settings.far_clip();
            let view = view.clone();

            h_flex()
                .gap_3()
                .items_center()
                .child(div().w_64().child(Slider::new(
                    "tessera-preview-far-clip-slider",
                    far_clip as f32,
                    FAR_CLIP_MIN as f32,
                    FAR_CLIP_MAX as f32,
                    FAR_CLIP_STEP as f32,
                    move |value, _, cx| {
                        let far_clip = value
                            .round()
                            .clamp(FAR_CLIP_MIN as f32, FAR_CLIP_MAX as f32)
                            as u32;

                        view.update(cx, |app, cx| {
                            app.state
                                .update_settings(|settings| settings.set_far_clip(far_clip));
                            cx.notify();
                        });
                    },
                )))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.text_secondary)
                        .child(format!("{far_clip}")),
                )
                .into_any_element()
        }
        #[cfg(feature = "sounds")]
        Field::Volume => {
            let volume = settings.volume();
            let view = view.clone();

            h_flex()
                .gap_3()
                .items_center()
                .child(div().w_64().child(Slider::new(
                    "tessera-preview-volume-slider",
                    volume as f32,
                    0.0,
                    50.0,
                    1.0,
                    move |value, _, cx| {
                        let volume = value.round().clamp(0.0, 50.0) as u32;

                        view.update(cx, |app, cx| {
                            app.state
                                .update_settings(|settings| settings.set_volume(volume));
                            cx.notify();
                        });
                    },
                )))
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.text_secondary)
                        .child(format!("{volume}%")),
                )
                .into_any_element()
        }
        Field::CustomScale => render_custom_scale(app, view, window, cx),
        Field::CheckUpdates => {
            let checking = app.is_checking_updates();
            let view = view.clone();

            Button::new("check-for-updates")
                .outline()
                .label(if checking {
                    "Checking..."
                } else {
                    "Check for Updates..."
                })
                .disabled(checking)
                .on_click(move |_, _, cx| {
                    view.update(cx, |app, cx| app.on_check_for_updates(cx));
                })
                .into_any_element()
        }
        Field::Link(id, label, url) => Button::new(id)
            .outline()
            .label(label)
            .on_click(move |_, _, cx| cx.open_url(url))
            .into_any_element(),
        _ => div().into_any_element(),
    }
}

/// Renders the custom model scale text field with its Apply button, as porter-app has it.
///
/// The field holds at most eight characters and is only usable while assets are scaled on
/// export with a custom scale. Applying a value that isn't a number warns and resets it to 1.0.
fn render_custom_scale(
    app: &TesseraApp,
    view: &Entity<TesseraApp>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let scale = app.state.settings.custom_scale();
    let enabled = scale.is_some() && app.state.settings.auto_scale();

    // Only the Apply button commits the field, as in porter-app.
    let state_entity = window.use_keyed_state("tessera-custom-scale-input", cx, |window, cx| {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .max_length(CUSTOM_SCALE_MAX_LENGTH)
                .default_value(format_custom_scale(scale.unwrap_or(1.0)))
        });

        ScaleFieldState { input, last: scale }
    });

    // Sync the displayed value when the setting changed externally (e.g. reset).
    state_entity.update(cx, |state, cx| {
        if state.last != scale {
            state.last = scale;
            state.input.update(cx, |input, cx| {
                input.set_value(format_custom_scale(scale.unwrap_or(1.0)), window, cx);
            });
        }
    });

    let input = state_entity.read(cx).input.clone();
    let apply_view = view.clone();
    let apply_state = state_entity.clone();

    h_flex()
        .gap_2()
        .items_center()
        .child(Input::new(&input).disabled(!enabled).w_32())
        .child(
            Button::new("apply-custom-scale")
                .label("Apply")
                .outline()
                .small()
                .disabled(!enabled)
                .on_click(move |_, window, cx| {
                    apply_view.update(cx, |app, cx| {
                        app.apply_custom_scale(&apply_state, window, cx)
                    });
                }),
        )
        .into_any_element()
}

impl TesseraApp {
    /// Applies the custom scale field: a number is saved, anything else warns and resets the
    /// scale to 1.0, as porter-app does.
    fn apply_custom_scale(
        &mut self,
        state: &Entity<ScaleFieldState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = &self.state.settings;

        if settings.custom_scale().is_none() || !settings.auto_scale() {
            return;
        }

        let value = state.read(cx).input.read(cx).value().trim().parse::<f32>();

        let scale = match value {
            Ok(value) => value,
            Err(_) => {
                window.push_notification(
                    Notification::warning(crate::strings::CUSTOM_SCALE_FACTOR_ERROR),
                    cx,
                );

                1.0
            }
        };

        self.state
            .update_settings(|settings| settings.set_custom_scale(Some(scale)));

        // Show what was saved, so a rejected value reads as 1.0 again.
        state.update(cx, |state, cx| {
            state.last = Some(scale);
            state.input.update(cx, |input, cx| {
                input.set_value(format_custom_scale(scale), window, cx);
            });
        });

        cx.notify();
    }
}

/// Whether every asset is treated as a raw file, which disables the load switches.
fn force_raw_files(settings: &AppSettings) -> bool {
    cfg!(feature = "raw-files-forcible") && settings.force_raw_files()
}

/// Formats a custom scale factor.
fn format_custom_scale(scale: f32) -> String {
    format!("{scale:?}")
}

/// Renders the about item showing the app identity, credits and disclaimer.
fn render_about(app: &TesseraApp, cx: &mut App) -> AnyElement {
    use crate::strings;

    let theme = *cx.tessera_theme();
    let name = app.state.name;
    let version = app.state.version;
    let description = app.state.description;

    v_flex()
        .gap_3()
        .w_full()
        .items_center()
        .justify_center()
        .child(app.render_logo(64.0, cx))
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_size(px(24.0))
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .child(name.to_uppercase()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme.text_muted)
                        .child(format!("v{version}")),
                ),
        )
        .child(
            div()
                .text_size(px(13.5))
                .text_color(theme.text_secondary)
                .child(description),
        )
        .child(
            div()
                .text_center()
                .text_size(px(13.5))
                .text_color(theme.warning)
                .child(strings::PORTER_THANKS),
        )
        .child(
            div()
                .text_center()
                .text_size(px(13.0))
                .text_color(theme.text_secondary)
                .child(app.state.author),
        )
        .child(
            div()
                .pt_2()
                .text_center()
                .text_size(px(11.5))
                .text_color(theme.text_muted)
                .child(format!(
                    "{} — \"{}\" {}.",
                    strings::PORTER_DISCLAIMER,
                    name.to_uppercase(),
                    app.state.copyright
                )),
        )
        .into_any_element()
}

/// Renders the export directory item with browse/open controls.
fn render_export_directory(
    app: &TesseraApp,
    view: &Entity<TesseraApp>,
    cx: &mut App,
) -> AnyElement {
    let theme = *cx.tessera_theme();
    let path = app.state.settings.output_directory();

    let browse_view = view.clone();
    let open_path = path.clone();

    v_flex()
        .gap_2()
        .child(div().text_sm().child("Export directory"))
        .child(
            div()
                .text_xs()
                .text_color(theme.text_muted)
                .child("Location where exported assets are saved."),
        )
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(
                    div()
                        .flex_1()
                        .px_2p5()
                        .py_1()
                        .rounded_md()
                        .overflow_hidden()
                        .bg(theme.background)
                        .border_1()
                        .border_color(theme.border)
                        .text_sm()
                        .text_color(theme.text_secondary)
                        .child(path.display().to_string()),
                )
                .child(
                    Button::new("browse-export-folder")
                        .label("Browse...")
                        .outline()
                        .small()
                        .on_click(move |_, _, cx| {
                            browse_view.update(cx, |app, _| app.on_pick_export_folder());
                        }),
                )
                .child(
                    Button::new("open-export-folder-settings")
                        .label("Open")
                        .outline()
                        .small()
                        .on_click(move |_, _, _| {
                            system::open_folder(&open_path);
                        }),
                ),
        )
        .into_any_element()
}

/// The dropdown key for an image file type.
fn image_format_key(file_type: ImageFileType) -> &'static str {
    match file_type {
        ImageFileType::Dds => "dds",
        ImageFileType::Png => "png",
        ImageFileType::Tiff => "tiff",
        ImageFileType::Tga => "tga",
        ImageFileType::Pvr => "pvr",
    }
}

/// The dropdown key for a model material processing method.
fn model_material_key(processing: ModelMaterialProcessing) -> &'static str {
    match processing {
        ModelMaterialProcessing::Skip => "skip",
        ModelMaterialProcessing::InModelFolder => "model_folder",
        ModelMaterialProcessing::InMaterialFolder => "material_folder",
    }
}

#[cfg(all(feature = "sounds", feature = "sounds-convertible"))]
/// The dropdown key for an export folder layout.
fn sound_path_key(processing: SoundPathProcessing) -> &'static str {
    match processing {
        SoundPathProcessing::FileName => "file_name",
        SoundPathProcessing::FolderTree => "folder_tree",
    }
}

/// The dropdown key for a normal map processing method.
#[cfg(feature = "normal-maps-convertible")]
fn normal_map_key(processing: ImageNormalMapProcessing) -> &'static str {
    match processing {
        ImageNormalMapProcessing::None => "none",
        ImageNormalMapProcessing::OpenGl => "opengl",
        ImageNormalMapProcessing::DirectX => "directx",
    }
}

/// The dropdown key for a preview control scheme.
fn preview_controls_key(controls: PreviewControlScheme) -> &'static str {
    match controls {
        PreviewControlScheme::Simple => "simple",
        PreviewControlScheme::Maya => "maya",
        PreviewControlScheme::Blender => "blender",
        PreviewControlScheme::Unreal => "unreal",
    }
}
