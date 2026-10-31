use gpui::App;
use gpui::Hsla;
use gpui::IntoElement;
use gpui::RenderOnce;
use gpui::StyleRefinement;
use gpui::Styled;
use gpui::Transformation;
use gpui::Window;
use gpui::prelude::FluentBuilder;
use gpui::svg;

use crate::components::Sizable;
use crate::components::Size;

/// The svg icons shipped with tessera-app, under `assets/icons`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IconName {
    Check,
    ChevronDown,
    ChevronRight,
    ChevronUp,
    ChevronsUpDown,
    CircleCheck,
    CircleX,
    Copy,
    Discord,
    Download,
    Folder,
    FolderOpen,
    Funnel,
    Gamepad,
    Inbox,
    Info,
    Loader,
    Pause,
    Play,
    Undo2,
    Search,
    Settings,
    SortAscending,
    SortDescending,
    TriangleAlert,
    User,
    WindowClose,
    WindowMaximize,
    WindowMinimize,
    WindowRestore,
    X,
    Bone,
    Droplet,
    Grid3x3,
    Images,
    Pentagon,
    RotateCcw,
    Box,
    FileText,
    FileCode,
    FileAudio,
}

impl IconName {
    /// Every icon, for serving them as assets.
    pub(crate) const ALL: [IconName; 41] = [
        IconName::Check,
        IconName::ChevronDown,
        IconName::ChevronRight,
        IconName::ChevronUp,
        IconName::ChevronsUpDown,
        IconName::CircleCheck,
        IconName::CircleX,
        IconName::Copy,
        IconName::Discord,
        IconName::Download,
        IconName::Folder,
        IconName::FolderOpen,
        IconName::Funnel,
        IconName::Gamepad,
        IconName::Inbox,
        IconName::Info,
        IconName::Loader,
        IconName::Pause,
        IconName::Play,
        IconName::Undo2,
        IconName::Search,
        IconName::Settings,
        IconName::SortAscending,
        IconName::SortDescending,
        IconName::TriangleAlert,
        IconName::User,
        IconName::WindowClose,
        IconName::WindowMaximize,
        IconName::WindowMinimize,
        IconName::WindowRestore,
        IconName::X,
        IconName::Bone,
        IconName::Droplet,
        IconName::Grid3x3,
        IconName::Images,
        IconName::Pentagon,
        IconName::RotateCcw,
        IconName::Box,
        IconName::FileText,
        IconName::FileCode,
        IconName::FileAudio,
    ];

    /// The asset path of the icon.
    pub(crate) const fn path(self) -> &'static str {
        match self {
            IconName::Check => "icons/check.svg",
            IconName::ChevronDown => "icons/chevron-down.svg",
            IconName::ChevronRight => "icons/chevron-right.svg",
            IconName::ChevronUp => "icons/chevron-up.svg",
            IconName::ChevronsUpDown => "icons/chevrons-up-down.svg",
            IconName::CircleCheck => "icons/circle-check.svg",
            IconName::CircleX => "icons/circle-x.svg",
            IconName::Copy => "icons/copy.svg",
            IconName::Discord => "icons/discord.svg",
            IconName::Download => "icons/download.svg",
            IconName::Folder => "icons/folder.svg",
            IconName::FolderOpen => "icons/folder-open.svg",
            IconName::Funnel => "icons/funnel.svg",
            IconName::Gamepad => "icons/gamepad-2.svg",
            IconName::Inbox => "icons/inbox.svg",
            IconName::Info => "icons/info.svg",
            IconName::Loader => "icons/loader.svg",
            IconName::Pause => "icons/pause.svg",
            IconName::Play => "icons/play.svg",
            IconName::Undo2 => "icons/undo-2.svg",
            IconName::Search => "icons/search.svg",
            IconName::Settings => "icons/settings.svg",
            IconName::SortAscending => "icons/sort-ascending.svg",
            IconName::SortDescending => "icons/sort-descending.svg",
            IconName::TriangleAlert => "icons/triangle-alert.svg",
            IconName::User => "icons/user.svg",
            IconName::WindowClose => "icons/window-close.svg",
            IconName::WindowMaximize => "icons/window-maximize.svg",
            IconName::WindowMinimize => "icons/window-minimize.svg",
            IconName::WindowRestore => "icons/window-restore.svg",
            IconName::X => "icons/x.svg",
            IconName::Bone => "icons/bone.svg",
            IconName::Droplet => "icons/droplet.svg",
            IconName::Grid3x3 => "icons/grid-3x3.svg",
            IconName::Images => "icons/images.svg",
            IconName::Pentagon => "icons/pentagon.svg",
            IconName::RotateCcw => "icons/rotate-ccw.svg",
            IconName::Box => "icons/box.svg",
            IconName::FileText => "icons/file-text.svg",
            IconName::FileCode => "icons/file-code.svg",
            IconName::FileAudio => "icons/file-audio.svg",
        }
    }

    /// The svg source of the icon, embedded in the binary.
    pub(crate) const fn bytes(self) -> &'static [u8] {
        match self {
            IconName::Check => include_bytes!("../../assets/icons/check.svg"),
            IconName::ChevronDown => include_bytes!("../../assets/icons/chevron-down.svg"),
            IconName::ChevronRight => include_bytes!("../../assets/icons/chevron-right.svg"),
            IconName::ChevronUp => include_bytes!("../../assets/icons/chevron-up.svg"),
            IconName::ChevronsUpDown => include_bytes!("../../assets/icons/chevrons-up-down.svg"),
            IconName::CircleCheck => include_bytes!("../../assets/icons/circle-check.svg"),
            IconName::CircleX => include_bytes!("../../assets/icons/circle-x.svg"),
            IconName::Copy => include_bytes!("../../assets/icons/copy.svg"),
            IconName::Discord => include_bytes!("../../assets/icons/discord.svg"),
            IconName::Download => include_bytes!("../../assets/icons/download.svg"),
            IconName::Folder => include_bytes!("../../assets/icons/folder.svg"),
            IconName::FolderOpen => include_bytes!("../../assets/icons/folder-open.svg"),
            IconName::Funnel => include_bytes!("../../assets/icons/funnel.svg"),
            IconName::Gamepad => include_bytes!("../../assets/icons/gamepad-2.svg"),
            IconName::Inbox => include_bytes!("../../assets/icons/inbox.svg"),
            IconName::Info => include_bytes!("../../assets/icons/info.svg"),
            IconName::Loader => include_bytes!("../../assets/icons/loader.svg"),
            IconName::Pause => include_bytes!("../../assets/icons/pause.svg"),
            IconName::Play => include_bytes!("../../assets/icons/play.svg"),
            IconName::Undo2 => include_bytes!("../../assets/icons/undo-2.svg"),
            IconName::Search => include_bytes!("../../assets/icons/search.svg"),
            IconName::Settings => include_bytes!("../../assets/icons/settings.svg"),
            IconName::SortAscending => include_bytes!("../../assets/icons/sort-ascending.svg"),
            IconName::SortDescending => include_bytes!("../../assets/icons/sort-descending.svg"),
            IconName::TriangleAlert => include_bytes!("../../assets/icons/triangle-alert.svg"),
            IconName::User => include_bytes!("../../assets/icons/user.svg"),
            IconName::WindowClose => include_bytes!("../../assets/icons/window-close.svg"),
            IconName::WindowMaximize => include_bytes!("../../assets/icons/window-maximize.svg"),
            IconName::WindowMinimize => include_bytes!("../../assets/icons/window-minimize.svg"),
            IconName::WindowRestore => include_bytes!("../../assets/icons/window-restore.svg"),
            IconName::X => include_bytes!("../../assets/icons/x.svg"),
            IconName::Bone => include_bytes!("../../assets/icons/bone.svg"),
            IconName::Droplet => include_bytes!("../../assets/icons/droplet.svg"),
            IconName::Grid3x3 => include_bytes!("../../assets/icons/grid-3x3.svg"),
            IconName::Images => include_bytes!("../../assets/icons/images.svg"),
            IconName::Pentagon => include_bytes!("../../assets/icons/pentagon.svg"),
            IconName::RotateCcw => include_bytes!("../../assets/icons/rotate-ccw.svg"),
            IconName::Box => include_bytes!("../../assets/icons/box.svg"),
            IconName::FileText => include_bytes!("../../assets/icons/file-text.svg"),
            IconName::FileCode => include_bytes!("../../assets/icons/file-code.svg"),
            IconName::FileAudio => include_bytes!("../../assets/icons/file-audio.svg"),
        }
    }
}

/// An svg icon, colored and sized from the surrounding text unless told otherwise.
#[derive(IntoElement)]
pub(crate) struct Icon {
    name: IconName,
    style: StyleRefinement,
    color: Option<Hsla>,
    size: Option<Size>,
    transformation: Option<Transformation>,
}

impl Icon {
    /// Constructs an icon.
    pub(crate) fn new(name: IconName) -> Self {
        Self {
            name,
            style: StyleRefinement::default(),
            color: None,
            size: None,
            transformation: None,
        }
    }

    /// Transforms the rendered icon, e.g. to rotate it.
    pub(crate) fn transform(mut self, transformation: Transformation) -> Self {
        self.transformation = Some(transformation);
        self
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }

    fn text_color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl Sizable for Icon {
    fn with_size(mut self, size: Size) -> Self {
        self.size = Some(size);
        self
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let text_style = window.text_style();
        let text_size = text_style.font_size.to_pixels(window.rem_size());
        let color = self.color.unwrap_or(text_style.color);
        let sized = self.style.size.width.is_some() || self.style.size.height.is_some();

        let mut icon = svg();
        *icon.style() = self.style;

        icon.flex_shrink_0()
            .text_color(color)
            .when(!sized, |this| this.size(text_size))
            .when_some(self.size, |this, size| match size {
                Size::XSmall => this.size_3(),
                Size::Small => this.size_3p5(),
                Size::Medium => this.size_4(),
            })
            .path(self.name.path())
            .when_some(self.transformation, |this, transformation| {
                this.with_transformation(transformation)
            })
    }
}
