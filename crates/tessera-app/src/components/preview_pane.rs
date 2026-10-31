use std::collections::HashMap;
use std::sync::Arc;

use gpui::App;
use gpui::AppContext as _;
use gpui::Context;
use gpui::Corner;
use gpui::Entity;
use gpui::Focusable;
use gpui::FontWeight;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::MouseMoveEvent;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Point;
use gpui::Render;
use gpui::RenderImage;
use gpui::ScrollStrategy;
use gpui::ScrollWheelEvent;
use gpui::SharedString;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled;
use gpui::TextRun;
use gpui::UniformListScrollHandle;
use gpui::Window;
use gpui::canvas;
use gpui::div;
use gpui::img;
use gpui::px;
use gpui::uniform_list;

use serde::Deserialize;

use porter_animation::Animation;
use porter_animation::AnimationSampler;
use porter_animation::CurveAttribute;
use porter_animation::CurveDataType;
use porter_animation::Joint;
use porter_animation::Keyframes;

use porter_math::Axis;
use porter_math::Quaternion;
use porter_math::Vector3;

use porter_model::Bone;
use porter_model::Model;
use porter_model::Skeleton;

use tessera_viewport::PreviewControlScheme;
use tessera_viewport::PreviewError;
use tessera_viewport::ViewportKeyState;
use tessera_viewport::ViewportRenderer;

use crate::AnimationModel;
use crate::AssetManager;
use crate::AssetPreview;
use crate::Settings;
use crate::audio_player::AudioPlayer;
use crate::audio_player::AudioPlayerError;
use crate::components::Button;
use crate::components::Disableable;
use crate::components::IconName;
use crate::components::Input;
use crate::components::InputEvent;
use crate::components::InputState;
use crate::components::Popover;
use crate::components::PopoverState;
use crate::components::Scrollbar;
use crate::components::Selectable;
use crate::components::Sizable;
use crate::components::Spinner;
use crate::components::TextView;
use crate::components::TextViewState;
use crate::components::h_flex;
use crate::components::popover_style;
use crate::components::scrollable_y;
use crate::components::v_flex;
use crate::palette;
use crate::theme::ActiveTesseraTheme;
use crate::theme::MONO_FONT_FAMILY;

/// Action to bind an animation preview to a model asset (by the id returned
/// from [`AssetManager::animation_preview_models`]), or back to bones only.
/// Dispatched from the preview header model picker.
#[derive(gpui::Action, Clone, PartialEq, Eq, Deserialize)]
#[action(namespace = tessera, no_json)]
pub struct BindAnimationModelAction(pub Option<u64>);

/// Maximum number of results shown in the animation model picker.
const MODEL_SEARCH_LIMIT: usize = 100;

/// How often fly movement is stepped while flying.
const FLY_TICK: std::time::Duration = std::time::Duration::from_millis(8);

/// The mouse buttons held over the viewport. Tracked separately from gpui's
/// move events, which only report one pressed button, since some Unreal
/// controls use two at once.
#[derive(Debug, Default, Clone, Copy)]
struct MouseButtons {
    left: bool,
    right: bool,
    middle: bool,
}

impl MouseButtons {
    /// Sets the held state for a button.
    fn set(&mut self, button: gpui::MouseButton, pressed: bool) {
        match button {
            gpui::MouseButton::Left => self.left = pressed,
            gpui::MouseButton::Right => self.right = pressed,
            gpui::MouseButton::Middle => self.middle = pressed,
            _ => {}
        }
    }
}

/// The movement keys held while flying.
#[derive(Debug, Default, Clone, Copy)]
struct FlyKeys {
    forward: bool,
    back: bool,
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

impl FlyKeys {
    /// Sets the held state for a movement key, returns false if the key
    /// isn't a movement key.
    fn set(&mut self, key: &str, pressed: bool) -> bool {
        let state = match key {
            "w" => &mut self.forward,
            "s" => &mut self.back,
            "a" => &mut self.left,
            "d" => &mut self.right,
            "e" => &mut self.up,
            "q" => &mut self.down,
            _ => return false,
        };

        *state = pressed;

        true
    }

    /// Returns the movement as (forward, right, up), each in -1..=1.
    fn axes(&self) -> (f32, f32, f32) {
        let axis = |positive: bool, negative: bool| positive as i32 as f32 - negative as i32 as f32;

        (
            axis(self.forward, self.back),
            axis(self.right, self.left),
            axis(self.up, self.down),
        )
    }
}

/// Animation preview playback state.
struct AnimationPreview {
    /// The source animation, kept for rebuilding the sampler on seek/rebind.
    animation: Animation,
    sampler: AnimationSampler,
    /// The joints fed to the sampler, as (name, parent) pairs.
    template: Vec<(Option<String>, i32)>,
    frame_count: u32,
    framerate: f32,
    looping: bool,
    playing: bool,
    /// The bound model in rest pose. The renderer skins it on the gpu
    /// against poses sent via [`ViewportRenderer::update_pose`].
    model: Option<Arc<AnimationModel>>,
    /// Retain the FK hierarchy for bones-only playback and seeking after unbinding the mesh.
    source_skeleton: Option<Skeleton>,
    /// Name of the bound model, shown in the dropdown trigger.
    model_name: Option<String>,
    up_axis: Axis,
    /// Focus the camera on the next uploaded frame.
    focus_pending: bool,
}

impl AnimationPreview {
    fn current_frame(&self) -> u32 {
        self.sampler.current_frame()
    }

    fn step(&mut self) -> bool {
        self.sampler.step().is_ok()
    }

    fn reset_sampler(&mut self, sampler: AnimationSampler, template: Vec<(Option<String>, i32)>) {
        self.sampler = sampler;
        self.template = template;
    }
}

/// The tab shown in the preview pane, as in porter-app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewTab {
    Viewport,
    Text,
    Binary,
    Audio,
}

/// Raw file preview content.
struct RawPreview {
    bytes: Arc<Vec<u8>>,
}

/// A docked pane that previews assets using the porter-viewport renderer.
///
/// The renderer is a headless wgpu renderer: frames are rendered offscreen,
/// read back to the cpu and displayed as a gpui image. Frames are only
/// re-rendered when the content or camera changes.
pub(crate) struct PreviewPane {
    renderer: ViewportRenderer,
    frame: Option<Arc<RenderImage>>,
    /// Name of the previewed asset, shown in the header.
    name: String,
    loading: bool,
    error: bool,
    unsupported: bool,
    has_content: bool,
    /// Whether the renderer needs to produce a new frame.
    dirty: bool,
    /// The size the renderer was last resized to, in physical pixels.
    size: (f32, f32),
    /// The window scale factor, for rendering at native resolution.
    scale: f32,
    far_clip: f32,
    control_scheme: PreviewControlScheme,
    /// Whether the viewport toggle buttons are drawn over the preview.
    show_overlay: bool,
    last_mouse: Option<Point<Pixels>>,
    buttons: MouseButtons,
    fly_keys: FlyKeys,
    /// Steps fly movement while the right mouse button is held in the Unreal
    /// control scheme.
    fly_tick: Option<gpui::Task<()>>,
    /// The hidden cursor's pinned position while dragging with the Unreal
    /// control scheme.
    cursor_lock: Option<cursor_lock::CursorLock>,
    /// Focused when the viewport is clicked, so fly keys reach it.
    focus_handle: gpui::FocusHandle,
    audio_player: Option<AudioPlayer>,
    raw: Option<RawPreview>,
    tab: PreviewTab,
    /// The read-only text of a raw file, for the text tab.
    text_view: Entity<TextViewState>,
    /// The scroll position of the hex rows, for the binary tab.
    hex_scroll: UniformListScrollHandle,
    /// Volume applied to loaded audio, from settings.
    volume: u32,
    auto_focus: bool,
    /// Staged seek position (in seconds) while dragging the seek bar.
    pending_seek: Option<f32>,
    /// The seek bar origin and width, captured during layout.
    seek_bounds: (f32, f32),
    /// Keeps the ui updating while audio is loaded.
    audio_tick: Option<gpui::Task<()>>,
    animation: Option<AnimationPreview>,
    animation_pending: bool,
    animation_requires_model_binding: bool,
    /// The asset manager, used to search models for animation previews.
    manager: Arc<dyn AssetManager>,
    /// Search input for the animation model picker.
    model_search: Entity<InputState>,
    /// Model search results, as (model id, name).
    animation_models: Vec<(u64, String)>,
    /// The last model bound to an animation preview. New animation previews
    /// bind to it automatically.
    last_bound_model: Option<Arc<AnimationModel>>,
    /// The width the header title had at the last paint, for clipping long names from the start.
    title_width: Pixels,
    /// Steps animation playback while an animation is loaded.
    animation_tick: Option<gpui::Task<()>>,
    #[allow(dead_code)]
    subscriptions: Vec<gpui::Subscription>,
}

impl PreviewPane {
    /// Constructs a new preview pane.
    pub(crate) fn new(
        settings: &Settings,
        manager: Arc<dyn AssetManager>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut renderer = ViewportRenderer::new();

        renderer.far_clip(settings.far_clip() as f32);

        let model_search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search for models..."));
        let text_view = cx.new(TextViewState::new);

        let subscriptions = vec![cx.subscribe(
            &model_search,
            |pane: &mut Self, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) && pane.is_animation_preview() {
                    pane.refresh_animation_models(cx);
                }
            },
        )];

        Self {
            renderer,
            frame: None,
            name: String::new(),
            loading: false,
            error: false,
            unsupported: false,
            has_content: false,
            dirty: false,
            size: (0.0, 0.0),
            scale: 1.0,
            far_clip: settings.far_clip() as f32,
            control_scheme: settings.preview_controls(),
            show_overlay: settings.preview_overlay(),
            last_mouse: None,
            buttons: MouseButtons::default(),
            fly_keys: FlyKeys::default(),
            fly_tick: None,
            cursor_lock: None,
            focus_handle: cx.focus_handle(),
            audio_player: None,
            raw: None,
            tab: PreviewTab::Viewport,
            text_view,
            hex_scroll: UniformListScrollHandle::new(),
            volume: settings.volume(),
            auto_focus: settings.auto_focus_on_model(),
            pending_seek: None,
            seek_bounds: (0.0, 0.0),
            audio_tick: None,
            animation: None,
            animation_pending: false,
            animation_requires_model_binding: false,
            manager,
            model_search,
            animation_models: Vec::new(),
            last_bound_model: None,
            title_width: px(0.0),
            animation_tick: None,
            subscriptions,
        }
    }

    /// Refreshes the animation model picker results from the current query.
    fn refresh_animation_models(&mut self, cx: &mut Context<Self>) {
        let query = self.model_search.read(cx).value().to_string();

        self.animation_models = self
            .manager
            .animation_preview_models(&query, MODEL_SEARCH_LIMIT);

        cx.notify();
    }

    /// Syncs preview related settings.
    pub(crate) fn sync_settings(&mut self, settings: &Settings, cx: &mut Context<Self>) {
        self.control_scheme = settings.preview_controls();
        self.show_overlay = settings.preview_overlay();
        self.stop_flying();
        self.release_cursor();
        self.far_clip = settings.far_clip() as f32;
        self.renderer.far_clip(self.far_clip);
        self.volume = settings.volume();
        self.auto_focus = settings.auto_focus_on_model();

        if let Some(player) = &mut self.audio_player {
            player.volume(self.volume);
        }

        if self.has_content {
            self.dirty = true;
            cx.notify();
        }
    }

    /// Stops anything currently playing and drops its timer.
    ///
    /// Dropping the [`AudioPlayer`] is what actually silences the output stream, so this must run
    /// whenever the pane stops showing its current asset — a new selection, or the pane being
    /// hidden. `set_preview` already does this for the case where a new preview arrives, but a
    /// request that never produces one (no selection, an error, or the pane closing) would
    /// otherwise leave the previous sound playing indefinitely.
    pub(crate) fn stop_playback(&mut self, cx: &mut Context<Self>) {
        self.audio_player = None;
        self.audio_tick = None;
        self.pending_seek = None;
        self.animation_tick = None;
        cx.notify();
    }

    /// Marks the pane as waiting for a preview response.
    pub(crate) fn set_loading(&mut self, cx: &mut Context<Self>) {
        // The pane is about to show a different asset, so whatever the last one was playing has
        // to stop now rather than when (or if) the replacement arrives.
        self.stop_playback(cx);
        self.loading = true;
        cx.notify();
    }

    /// Sets the preview content from an asset preview response.
    pub(crate) fn set_preview(&mut self, asset: AssetPreview, cx: &mut Context<Self>) {
        self.loading = false;
        self.error = false;
        self.unsupported = false;
        self.has_content = false;
        self.name = String::new();
        self.audio_player = None;
        self.raw = None;
        self.tab = PreviewTab::Viewport;
        self.pending_seek = None;
        self.audio_tick = None;
        self.animation = None;
        self.animation_pending = false;
        self.animation_requires_model_binding = false;
        self.animation_tick = None;

        let result = match asset {
            AssetPreview::NotSupported => {
                self.renderer.clear_preview();
                self.unsupported = true;
                Ok(())
            }
            AssetPreview::PreviewError => {
                self.renderer.clear_preview();
                self.error = true;
                Ok(())
            }
            AssetPreview::RawFile(name, bytes) => {
                self.renderer.clear_preview();
                self.name = name;

                // Show text when the content is valid utf-8 without control
                // characters (other than whitespace and the unit separator).
                let text = std::str::from_utf8(&bytes).ok().filter(|text| {
                    !text
                        .chars()
                        .any(|ch| ch.is_control() && !ch.is_whitespace() && ch != '\u{1f}')
                });
                let is_text = text.is_some();
                let content: SharedString = match text {
                    Some(text) => text.to_string().into(),
                    None => TEXT_UNSUPPORTED.into(),
                };

                self.text_view
                    .update(cx, |view, cx| view.set_text(content, cx));
                self.hex_scroll.scroll_to_item(0, ScrollStrategy::Top);
                self.tab = if is_text {
                    PreviewTab::Text
                } else {
                    PreviewTab::Binary
                };
                self.raw = Some(RawPreview {
                    bytes: Arc::new(bytes),
                });

                Ok(())
            }
            AssetPreview::Audio(name, audio) => {
                self.renderer.clear_preview();
                self.name = name;
                self.tab = PreviewTab::Audio;

                match AudioPlayer::load(audio) {
                    Ok(mut player) => {
                        player.volume(self.volume);
                        player.play();

                        self.audio_player = Some(player);
                        self.start_audio_tick(cx);
                    }
                    Err(AudioPlayerError::Unsupported) => self.unsupported = true,
                    Err(_) => self.error = true,
                }

                Ok(())
            }
            AssetPreview::Image(name, image) => {
                self.name = name.clone();
                self.has_content = true;
                self.renderer.set_preview_image(name, image)
            }
            AssetPreview::Material(name, material) => {
                self.name = name.clone();
                self.has_content = true;
                self.renderer.set_preview_material(name, material)
            }
            AssetPreview::Model(name, model, images) => {
                self.name = name.clone();
                self.has_content = true;
                let materials: Vec<_> =
                    images.into_iter().map(crate::ModelMaterial::from).collect();
                self.renderer.set_preview_model(
                    name,
                    model,
                    &materials,
                    cfg!(feature = "srgb-preview"),
                    self.auto_focus,
                )
            }
            AssetPreview::ModelWithMaterials(name, model, images, focus) => {
                self.name = name.clone();
                self.has_content = true;

                let srgb = cfg!(feature = "srgb-preview");

                self.renderer
                    .set_preview_model(name, model, &images, srgb, focus)
            }
            AssetPreview::AnimationUnbound(name, animation) => {
                self.renderer.clear_preview();
                self.name = name;
                self.has_content = true;
                self.animation_requires_model_binding = true;
                if let Some(animation) = animation {
                    self.start_animation(animation, None, cx)
                } else {
                    self.animation_pending = true;
                    self.refresh_animation_models(cx);
                    Ok(())
                }
            }
            AssetPreview::Animation(name, animation) => {
                self.name = name.clone();
                self.has_content = true;

                let _ = name;

                self.start_animation(animation, None, cx)
            }
            AssetPreview::AnimationWithModel(name, animation, model) => {
                self.name = name;
                self.has_content = true;
                self.start_animation(animation, Some(model), cx)
            }
            #[cfg(feature = "worlds")]
            AssetPreview::Scene(name, scene, focus) => {
                self.name = name.clone();
                self.has_content = true;

                let srgb = cfg!(feature = "srgb-preview");

                self.renderer.set_preview_scene(name, scene, srgb, focus)
            }
            #[cfg(feature = "worlds")]
            AssetPreview::World(name, world, models, focus) => {
                self.name = name.clone();
                self.has_content = true;

                let srgb = cfg!(feature = "srgb-preview");

                self.renderer
                    .set_preview_world(name, world, models, srgb, focus)
            }
        };

        if let Err(error) = result {
            self.has_content = false;

            if matches!(error, PreviewError::Unsupported) {
                self.unsupported = true;
            } else {
                self.error = true;
            }
        }

        self.dirty = true;
        cx.notify();
    }

    /// Keeps the ui updating while audio is loaded, for position display.
    fn start_audio_tick(&mut self, cx: &mut Context<Self>) {
        self.audio_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(200))
                    .await;

                let result = this.update(cx, |pane, cx| {
                    if pane.audio_player.is_none() {
                        return false;
                    }

                    cx.notify();

                    true
                });

                match result {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }));
    }

    /// Starts previewing the given animation, bones only until a model is bound.
    fn start_animation(
        &mut self,
        animation: Animation,
        model: Option<Arc<AnimationModel>>,
        cx: &mut Context<Self>,
    ) -> Result<(), PreviewError> {
        let frame_count = animation.frame_count();
        let framerate = animation.frame_rate;
        let looping = animation.looping;
        let up_axis = animation.up_axis;

        let (sampler, template) = build_animation_sampler(&animation, None);

        // Without any animated joints there is nothing to show.
        if template.is_empty() {
            return Err(PreviewError::Unsupported);
        }

        let mut preview = AnimationPreview {
            animation,
            sampler,
            template,
            frame_count,
            framerate: if framerate.is_finite() && framerate >= 1.0 {
                framerate
            } else {
                30.0
            },
            looping,
            playing: true,
            model: None,
            source_skeleton: None,
            model_name: None,
            up_axis,
            focus_pending: false,
        };

        // Sample the first frame so something shows before playback ticks.
        let _ = preview.step();

        self.animation = Some(preview);

        // Bind the last used model automatically; a tiny fraction of a
        // model's bones are usually animated, so bones alone say little.
        if let Some(model) = model {
            self.last_bound_model = Some(Arc::clone(&model));
            self.bind_animation_model_inner(Some(model), false, cx);
        } else if !self.animation_requires_model_binding
            && let Some(model) = self.last_bound_model.clone()
            && self
                .manager
                .animation_preview_models(&model.name, usize::MAX)
                .iter()
                .any(|(id, _)| *id == model.id)
        {
            self.bind_animation_model_inner(Some(model), false, cx);
        } else {
            self.rebind_animation_renderer()?;
            self.upload_animation_frame()?;
            self.renderer.reset_view();
        }

        self.start_animation_tick(cx);
        self.refresh_animation_models(cx);

        Ok(())
    }

    /// Binds the animation preview to the given model, or back to bones only
    /// when `model` is `None`. The model is remembered and bound automatically
    /// to subsequent animation previews.
    pub(crate) fn bind_animation_model(
        &mut self,
        model: Option<AnimationModel>,
        cx: &mut Context<Self>,
    ) {
        let model = model.map(Arc::new);

        self.last_bound_model = model.clone();
        self.bind_animation_model_inner(model, true, cx);
        // Loading the model cancels playback; resume the timer once binding finishes.
        self.start_animation_tick(cx);
    }

    pub(crate) fn clear_animation_model(&mut self, cx: &mut Context<Self>) {
        self.last_bound_model = None;
        cx.notify();
    }

    /// A unique compatible model can be loaded automatically without choosing an outfit by guesswork.
    pub(crate) fn automatic_animation_model(&self) -> Option<u64> {
        if !self.is_animation_preview()
            || self
                .animation
                .as_ref()
                .is_some_and(|animation| animation.model.is_some())
        {
            return None;
        }
        if self.animation_requires_model_binding
            && let Some(model) = &self.last_bound_model
            && self
                .manager
                .animation_preview_models(&model.name, usize::MAX)
                .iter()
                .any(|(id, _)| *id == model.id)
        {
            return Some(model.id);
        }
        let models = self.manager.animation_preview_models("", 2);
        (models.len() == 1).then(|| models[0].0)
    }

    /// Binds the animation preview to the given model, without touching the
    /// remembered model.
    fn bind_animation_model_inner(
        &mut self,
        model: Option<Arc<AnimationModel>>,
        focus: bool,
        cx: &mut Context<Self>,
    ) {
        self.loading = false;

        let Some(animation) = &mut self.animation else {
            return;
        };

        let frame = animation.current_frame();

        match model {
            Some(model) => {
                animation.up_axis = model.model.up_axis;
                animation.model_name = Some(model.name.clone());
                animation.source_skeleton = Some(model.model.skeleton.clone());
                animation.model = Some(model);
                animation.focus_pending = focus;
            }
            None => {
                animation.up_axis = animation.animation.up_axis;
                animation.model = None;
                animation.model_name = None;
            }
        }

        let (sampler, template) = build_animation_sampler(
            &animation.animation,
            animation
                .model
                .as_ref()
                .map(|model| &model.model.skeleton)
                .or(animation.source_skeleton.as_ref()),
        );

        animation.reset_sampler(sampler, template);

        // Re-sample up to the frame we were on before rebinding.
        for _ in 0..=frame {
            let _ = animation.step();
        }

        let rebind = self
            .rebind_animation_renderer()
            .and_then(|_| self.upload_animation_frame());

        if rebind.is_err() {
            self.error = true;
            self.has_content = false;
        }

        self.start_animation_tick(cx);

        cx.notify();
    }

    /// Whether an animation preview is currently active.
    pub(crate) fn is_animation_preview(&self) -> bool {
        self.animation.is_some() || self.animation_pending
    }

    /// Rebuilds the renderer content for the current animation binding. The
    /// bound model (or the bones-only skeleton) is uploaded once here;
    /// playback then only sends poses via [`Self::upload_animation_frame`].
    fn rebind_animation_renderer(&mut self) -> Result<(), PreviewError> {
        let Some(animation) = &mut self.animation else {
            return Ok(());
        };

        let model = match &animation.model {
            Some(rest) => rest.model.clone(),
            None => {
                // Bones only: a mesh-less model carrying the sampled skeleton.
                let mut model = Model::new();

                model.up_axis = animation.up_axis;
                model.skeleton = sample_skeleton(animation)?;
                model
            }
        };

        let focus = animation.focus_pending;

        animation.focus_pending = false;
        let focus_model = if focus && animation.model.is_some() {
            Some(model.clone())
        } else {
            None
        };
        let focus_pose = focus_model
            .as_ref()
            .map(|_| sample_skeleton(animation))
            .transpose()?;

        // A bones-only preview has no textures; a bound model carries the ones it was decoded
        // with, so the animation renders against the same materials as its model preview.

        let srgb = cfg!(feature = "srgb-preview");

        let materials = animation
            .model
            .as_ref()
            .map(|model| model.materials.as_slice())
            .unwrap_or_default();
        self.renderer
            .set_preview_model(self.name.clone(), model, materials, srgb, focus)?;
        if let (Some(model), Some(pose)) = (&focus_model, &focus_pose) {
            self.renderer.focus_on_pose(model, pose);
        }

        self.dirty = true;

        Ok(())
    }

    /// Uploads the current animation frame pose to the renderer, which skins
    /// the bound model on the gpu.
    fn upload_animation_frame(&mut self) -> Result<(), PreviewError> {
        let Some(animation) = &mut self.animation else {
            return Ok(());
        };

        let skeleton = sample_skeleton(animation)?;

        self.renderer.update_pose(&skeleton);
        self.dirty = true;

        Ok(())
    }

    /// Steps animation playback while an animation is loaded.
    fn start_animation_tick(&mut self, cx: &mut Context<Self>) {
        let interval = std::time::Duration::from_secs_f32(
            1.0 / self
                .animation
                .as_ref()
                .map(|animation| animation.framerate)
                .unwrap_or(30.0)
                .clamp(1.0, 120.0),
        );

        self.animation_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(interval).await;

                let result = this.update(cx, |pane, cx| {
                    let Some(animation) = &mut pane.animation else {
                        return false;
                    };

                    // Bones-only playback can continue once a model has supplied the FK hierarchy.
                    if animation.playing
                        && (animation.model.is_some() || animation.source_skeleton.is_some())
                    {
                        let _ = animation.step();

                        // Stop at the last frame when the animation shouldn't loop.
                        if !animation.looping
                            && animation.current_frame() + 1 >= animation.frame_count
                        {
                            animation.playing = false;
                        }

                        if pane.upload_animation_frame().is_err() {
                            pane.error = true;
                            pane.has_content = false;
                            return false;
                        }
                        cx.notify();
                    }

                    true
                });

                match result {
                    Ok(true) => {}
                    _ => break,
                }
            }
        }));
    }

    /// Toggles animation playback.
    fn toggle_animation_playback(&mut self, cx: &mut Context<Self>) {
        let Some(animation) = &mut self.animation else {
            return;
        };
        // Resuming from the end of a non-looping animation restarts it from
        // the beginning instead of stepping one frame past the last.
        let finished = !animation.looping && animation.current_frame() + 1 >= animation.frame_count;

        if !animation.playing && finished {
            let (sampler, template) = build_animation_sampler(
                &animation.animation,
                animation
                    .model
                    .as_ref()
                    .map(|model| &model.model.skeleton)
                    .or(animation.source_skeleton.as_ref()),
            );

            animation.reset_sampler(sampler, template);

            // Sample the first frame so it shows immediately.
            let _ = animation.step();
            animation.playing = true;

            // Borrow of `animation` ends above; safe to touch `self` again.
            let _ = self.upload_animation_frame();
        } else {
            animation.playing = !animation.playing;
        }

        cx.notify();
    }

    /// Seeks the animation to the given fraction of its length.
    fn seek_animation_to_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(animation) = &mut self.animation else {
            return;
        };
        let last_frame = animation.frame_count.saturating_sub(1);
        let frame = ((fraction.clamp(0.0, 1.0) * last_frame as f32) as u32).min(last_frame);
        let current = animation.current_frame();

        if frame == current {
            return;
        }

        // The sampler only steps forward; rebuild it when seeking backwards.
        if frame < current {
            let (sampler, template) = build_animation_sampler(
                &animation.animation,
                animation
                    .model
                    .as_ref()
                    .map(|model| &model.model.skeleton)
                    .or(animation.source_skeleton.as_ref()),
            );

            animation.reset_sampler(sampler, template);

            for _ in 0..=frame {
                let _ = animation.step();
            }
        } else {
            for _ in current..frame {
                let _ = animation.step();
            }
        }

        let _ = self.upload_animation_frame();

        cx.notify();
    }

    /// Toggles audio playback.
    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(player) = &mut self.audio_player {
            if player.is_playing() {
                player.pause();
            } else {
                player.play();
            }

            cx.notify();
        }
    }

    /// Seeks the audio player to the given fraction of its duration.
    fn seek_to_fraction(&mut self, fraction: f32, commit: bool, cx: &mut Context<Self>) {
        let Some(player) = &mut self.audio_player else {
            return;
        };

        let duration = player.duration().as_secs_f32();
        let position = (fraction.clamp(0.0, 1.0) * duration).min(duration);

        if commit {
            player.seek(std::time::Duration::from_secs_f32(position));
            self.pending_seek = None;
        } else {
            self.pending_seek = Some(position);
        }

        cx.notify();
    }

    /// Whether a text input owned by the preview (the animation model search)
    /// currently has focus, so plain-key shortcuts should not fire.
    pub(crate) fn is_text_input_focused(&self, window: &Window, cx: &App) -> bool {
        self.model_search
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    }

    /// Handles a viewport hotkey, returns whether it was handled.
    pub(crate) fn handle_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        if key == "space" && self.animation.is_some() {
            self.toggle_animation_playback(cx);

            return true;
        }

        match key {
            "r" => self.renderer.reset_view(),
            "f" => self.renderer.focus(),
            "g" => self.renderer.toggle_grid(),
            "b" => self.renderer.toggle_bones(),
            "w" => self.renderer.toggle_wireframe(),
            "m" => self.renderer.toggle_shaded(),
            "n" => self.renderer.cycle_material(),
            _ => return false,
        }

        self.dirty = true;
        cx.notify();

        true
    }

    /// Renders the viewport toggle buttons drawn over the bottom of the preview.
    fn render_overlay_controls(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let mut row = h_flex()
            .gap_1()
            .p_1()
            .rounded(cx.tessera_theme().radius)
            .border_1()
            .border_color(cx.tessera_theme().border)
            .bg(palette::with_alpha(cx.tessera_theme().element, 0.85));

        for (id, icon, key, tooltip) in OVERLAY_CONTROLS {
            row = row.child(
                Button::new(*id)
                    .icon(*icon)
                    .ghost()
                    .small()
                    .tooltip(*tooltip)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.handle_key(key, cx);
                    })),
            );
        }

        div().absolute().bottom_0().left_0().p_2().child(row)
    }

    /// Updates the renderer size to the pane bounds, in physical pixels.
    fn update_bounds(&mut self, width: f32, height: f32, scale: f32, cx: &mut Context<Self>) {
        let width = width * scale;
        let height = height * scale;

        if width < 1.0 || height < 1.0 {
            return;
        }

        if (self.size.0 - width).abs() < 1.0 && (self.size.1 - height).abs() < 1.0 {
            return;
        }

        self.size = (width, height);
        self.scale = scale;
        self.renderer.resize(width, height, self.far_clip);
        self.dirty = true;

        cx.notify();
    }

    /// Renders a new frame if the content or camera changed.
    fn render_frame_if_dirty(&mut self, window: &mut Window) {
        if !self.dirty || self.size.0 < 1.0 || self.size.1 < 1.0 {
            return;
        }

        self.dirty = false;

        // Already the premultiplied bgra gpui wants: the viewport renders to a bgra target and
        // draws nothing translucent, so there is no per-pixel pass to do here. That pass cost a
        // full walk of the frame on the cpu, every frame.
        let (width, height, pixels) = self.renderer.render_bgra();

        let Some(buffer) = image::RgbaImage::from_raw(width, height, pixels) else {
            return;
        };

        let frame = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));

        // Each RenderImage gets its own sprite atlas entry, which gpui only frees on
        // `drop_image`, so the frame being replaced has to be evicted by hand or every
        // rendered frame stays resident in ram and vram.
        if let Some(previous) = self.frame.replace(frame) {
            let _ = window.drop_image(previous);
        }
    }

    /// The viewport key state for the held buttons and the given modifiers.
    fn key_state(&self, modifiers: &gpui::Modifiers) -> ViewportKeyState {
        ViewportKeyState {
            control_scheme: self.control_scheme,
            left: self.buttons.left,
            right: self.buttons.right,
            middle: self.buttons.middle,
            alt: modifiers.alt || modifiers.platform,
            shift: modifiers.shift,
        }
    }

    /// Whether the Unreal controls apply to the current content (they don't
    /// for 2d image previews).
    fn is_unreal_3d(&self) -> bool {
        self.control_scheme == PreviewControlScheme::Unreal
            && self.has_content
            && !self.renderer.is_orthographic()
    }

    /// Whether the viewport is in Unreal style fly mode (right mouse held),
    /// where movement keys fly the camera instead of acting as hotkeys.
    fn is_flying(&self) -> bool {
        self.is_unreal_3d() && self.buttons.right
    }

    /// Handles a movement key while flying, returns whether it was handled.
    fn fly_key(&mut self, key: &str, pressed: bool, cx: &mut Context<Self>) -> bool {
        if pressed && !self.is_flying() {
            return false;
        }

        // Releases are always taken so a key let go after the mouse doesn't
        // stay held.
        if !self.fly_keys.set(key, pressed) {
            return false;
        }

        if pressed && self.fly_tick.is_none() {
            self.start_fly_tick(cx);
        }

        true
    }

    /// Handles a mouse button press over the viewport.
    fn on_mouse_down(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        // Take keyboard focus so fly keys come to the viewport rather than
        // whatever was focused before.
        self.focus_handle.focus(window);

        self.buttons.set(event.button, true);
        self.last_mouse = Some(event.position);

        // Unreal hides the cursor and keeps it in place while dragging.
        if self.is_unreal_3d() && self.cursor_lock.is_none() {
            self.cursor_lock = cursor_lock::lock(event.position);
        }
    }

    /// Handles a mouse button release, over the viewport or outside it.
    fn on_mouse_up(&mut self, button: gpui::MouseButton) {
        self.buttons.set(button, false);

        if button == gpui::MouseButton::Right {
            self.stop_flying();
        }

        if !self.buttons.left && !self.buttons.right && !self.buttons.middle {
            self.release_cursor();
        }
    }

    /// Ends fly mode, releasing any held movement keys.
    fn stop_flying(&mut self) {
        self.fly_keys = FlyKeys::default();
        self.fly_tick = None;
    }

    /// Shows the cursor again where the drag started, if it was locked.
    fn release_cursor(&mut self) {
        if let Some(lock) = self.cursor_lock.take() {
            cursor_lock::unlock(lock);
        }
    }

    /// Steps fly movement from the held keys until fly mode ends.
    fn start_fly_tick(&mut self, cx: &mut Context<Self>) {
        let mut last = std::time::Instant::now();

        self.fly_tick = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(FLY_TICK).await;

                let now = std::time::Instant::now();
                // Capped so a stall doesn't launch the camera across the map.
                let delta = (now - last).as_secs_f32().min(0.1);

                last = now;

                let result = this.update(cx, |pane, cx| {
                    let (forward, right, up) = pane.fly_keys.axes();

                    if forward != 0.0 || right != 0.0 || up != 0.0 {
                        pane.renderer.fly(forward, right, up, delta);
                        pane.dirty = true;
                        cx.notify();
                    }
                });

                if result.is_err() {
                    break;
                }
            }
        }));
    }

    /// Handles mouse movement over the viewport.
    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.last_mouse.replace(event.position);

        let Some(last) = last else {
            return;
        };

        if event.pressed_button.is_none() {
            // A release outside the window never reaches us, catch it here.
            if self.buttons.right {
                self.stop_flying();
            }

            self.buttons = MouseButtons::default();
            self.release_cursor();

            return;
        }

        // While the cursor is locked, movement is measured from the lock point
        // and the cursor is put back there, so it never leaves the viewport.
        let last = match &self.cursor_lock {
            Some(lock) => {
                if event.position == lock.position {
                    return;
                }

                cursor_lock::recenter(lock);

                self.last_mouse = Some(lock.position);

                lock.position
            }
            None => last,
        };

        // The renderer expects the delta as `last - current` (matching the
        // porter-app widget), in physical pixels.
        let delta = (
            f32::from(last.x - event.position.x) * self.scale,
            f32::from(last.y - event.position.y) * self.scale,
        );

        self.renderer
            .mouse_move(delta, self.key_state(&event.modifiers));

        self.dirty = true;
        cx.notify();
    }

    /// Handles scrolling over the viewport.
    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(px(40.0)).y) / 40.0;

        if delta == 0.0 {
            return;
        }

        self.renderer
            .scroll_delta(delta, self.key_state(&event.modifiers));
        self.dirty = true;

        cx.notify();
    }

    /// Commits a staged seek from the seek bar.
    fn commit_pending_seek(&mut self, cx: &mut Context<Self>) {
        let Some(position) = self.pending_seek.take() else {
            return;
        };

        if let Some(player) = &mut self.audio_player {
            player.seek(std::time::Duration::from_secs_f32(position));
        }

        cx.notify();
    }

    /// The seek bar fraction for the given window x position.
    fn seek_fraction_at(&self, x: f32) -> f32 {
        let (origin, width) = self.seek_bounds;

        if width <= 0.0 {
            return 0.0;
        }

        ((x - origin) / width).clamp(0.0, 1.0)
    }

    /// Renders the audio player body.
    fn render_audio(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (playing, duration, position) = match &self.audio_player {
            Some(player) => (
                player.is_playing(),
                player.duration().as_secs_f32(),
                self.pending_seek
                    .unwrap_or_else(|| player.position().as_secs_f32()),
            ),
            None => (false, 0.0, 0.0),
        };

        let fraction = if duration > 0.0 {
            (position / duration).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let entity = cx.entity();

        // Probe the seek bar bounds, for mapping mouse positions to seeks.
        let seek_probe = canvas(
            move |bounds, _, cx: &mut App| {
                let origin = f32::from(bounds.origin.x);
                let width = f32::from(bounds.size.width);

                cx.defer(move |cx| {
                    entity.update(cx, |pane, _| pane.seek_bounds = (origin, width));
                });
            },
            |_, _, _, _| {},
        )
        .size_full();

        let seek_bar = div()
            .id("audio-seek")
            .relative()
            .w_full()
            .h(px(20.0))
            .flex()
            .items_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    let fraction = this.seek_fraction_at(f32::from(event.position.x));
                    this.seek_to_fraction(fraction, false, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(gpui::MouseButton::Left)
                    && this.pending_seek.is_some()
                {
                    let fraction = this.seek_fraction_at(f32::from(event.position.x));
                    this.seek_to_fraction(fraction, false, cx);
                }
            }))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseUpEvent, _, cx| {
                    if this.pending_seek.is_some() {
                        let fraction = this.seek_fraction_at(f32::from(event.position.x));
                        this.seek_to_fraction(fraction, false, cx);
                        this.commit_pending_seek(cx);
                    }
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.commit_pending_seek(cx);
                }),
            )
            .child(div().absolute().inset_0().child(seek_probe))
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .bg(cx.tessera_theme().border)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(cx.tessera_theme().primary)
                            .w(gpui::relative(fraction)),
                    ),
            );

        v_flex()
            .flex_1()
            .w_full()
            .items_center()
            .justify_center()
            .gap_3()
            .px_8()
            .bg(cx.tessera_theme().background)
            .child(
                div()
                    .max_w_full()
                    .overflow_hidden()
                    .text_size(px(13.0))
                    .text_color(cx.tessera_theme().text_secondary)
                    .child(self.name.clone()),
            )
            .child(seek_bar)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .child(
                        Button::new("audio-play-pause")
                            .icon(if playing {
                                IconName::Pause
                            } else {
                                IconName::Play
                            })
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_playback(cx))),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(cx.tessera_theme().text_muted)
                            .child(format!(
                                "{} / {}",
                                format_position(position),
                                format_position(duration)
                            )),
                    ),
            )
    }

    /// Renders the animation playback controls bar.
    fn render_animation_controls(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let (playing, frame, frame_count, framerate) = match &self.animation {
            Some(animation) => (
                animation.playing,
                animation.current_frame(),
                animation.frame_count,
                animation.framerate,
            ),
            None => (false, 0, 0, 0.0),
        };

        let fraction = if frame_count > 1 {
            (frame as f32 / (frame_count - 1) as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let entity = cx.entity();

        // Probe the seek bar bounds, for mapping mouse positions to seeks.
        let seek_probe = canvas(
            move |bounds, _, cx: &mut App| {
                let origin = f32::from(bounds.origin.x);
                let width = f32::from(bounds.size.width);

                cx.defer(move |cx| {
                    entity.update(cx, |pane, _| pane.seek_bounds = (origin, width));
                });
            },
            |_, _, _, _| {},
        )
        .size_full();

        let seek_bar = div()
            .id("anim-seek")
            .relative()
            .flex_1()
            .h(px(20.0))
            .flex()
            .items_center()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, _, cx| {
                    let fraction = this.seek_fraction_at(f32::from(event.position.x));
                    this.seek_animation_to_fraction(fraction, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if event.pressed_button == Some(gpui::MouseButton::Left) {
                    let fraction = this.seek_fraction_at(f32::from(event.position.x));
                    this.seek_animation_to_fraction(fraction, cx);
                }
            }))
            .child(div().absolute().inset_0().child(seek_probe))
            .child(
                div()
                    .w_full()
                    .h(px(6.0))
                    .rounded_full()
                    .bg(cx.tessera_theme().border)
                    .child(
                        div()
                            .h_full()
                            .rounded_full()
                            .bg(cx.tessera_theme().primary)
                            .w(gpui::relative(fraction)),
                    ),
            );

        h_flex()
            .w_full()
            .px_3()
            .py_1()
            .gap_3()
            .items_center()
            .border_t_1()
            .border_color(cx.tessera_theme().border)
            .child(
                Button::new("anim-play-pause")
                    .icon(if playing {
                        IconName::Pause
                    } else {
                        IconName::Play
                    })
                    .xsmall()
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_animation_playback(cx))),
            )
            .child(seek_bar)
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(cx.tessera_theme().text_muted)
                    .whitespace_nowrap()
                    .child(format!(
                        "{} / {}  ·  {framerate:.0} fps",
                        frame + 1,
                        frame_count
                    )),
            )
    }

    /// Switches the shown tab.
    fn set_tab(&mut self, tab: PreviewTab, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            cx.notify();
        }
    }

    /// Renders the tab row at the bottom of the pane, each tab enabled by what the current
    /// preview can show, as in porter-app.
    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let has_raw = self.raw.is_some();
        let has_audio = self.audio_player.is_some();
        let tab = self.tab;
        let raw_tab = matches!(tab, PreviewTab::Text | PreviewTab::Binary);

        let mut tabs: Vec<(&str, IconName, &str, PreviewTab, bool)> = vec![
            (
                "preview-tab-viewport",
                IconName::Box,
                "Viewport",
                PreviewTab::Viewport,
                !has_raw && !has_audio,
            ),
            (
                "preview-tab-text",
                IconName::FileText,
                "Text",
                PreviewTab::Text,
                (has_raw && !has_audio) || raw_tab,
            ),
            (
                "preview-tab-binary",
                IconName::FileCode,
                "Binary",
                PreviewTab::Binary,
                (has_raw && !has_audio) || raw_tab,
            ),
        ];

        if cfg!(feature = "sounds") {
            tabs.push((
                "preview-tab-audio",
                IconName::FileAudio,
                "Audio",
                PreviewTab::Audio,
                has_audio || tab == PreviewTab::Audio,
            ));
        }

        let mut row = h_flex()
            .w_full()
            .px_2()
            .py_1()
            .gap_1()
            .border_t_1()
            .border_color(cx.tessera_theme().border);

        for (id, icon, label, target, enabled) in tabs {
            row = row.child(
                Button::new(id)
                    .xsmall()
                    .ghost()
                    .icon(icon)
                    .label(label)
                    .selected(tab == target)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.set_tab(target, cx))),
            );
        }

        row
    }

    /// Renders the text tab: the raw file as wrapping, selectable text.
    fn render_text(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(cx.tessera_theme().background)
            .font_family(MONO_FONT_FAMILY)
            .text_size(HEX_FONT_SIZE)
            .text_color(cx.tessera_theme().text)
            .child(TextView::new(&self.text_view))
            .into_any_element()
    }

    /// Renders the binary tab: a hex dump laid out like porter-app's binary widget, with the
    /// header pinned above the scrolling rows and the column widths measured from the font.
    fn render_hex(&self, window: &mut Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = *cx.tessera_theme();
        let bytes = self
            .raw
            .as_ref()
            .map(|raw| raw.bytes.clone())
            .unwrap_or_default();

        let mut font = window.text_style().font();

        font.family = MONO_FONT_FAMILY.into();

        let measure = |text: &'static str| {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            window
                .text_system()
                .shape_line(SharedString::from(text), HEX_FONT_SIZE, &[run], None)
        };

        let offset_line = measure(HEX_OFFSET_CONTENT);
        let offset_width = offset_line.width;
        let hex_width = measure(HEX_BYTES_CONTENT).width;
        let text_width = measure(HEX_TEXT_CONTENT).width;
        let row_height = (offset_line.ascent + offset_line.descent).ceil() + px(4.0);
        let rows = bytes.len().div_ceil(HEX_BYTES_PER_ROW);

        let cell = move |width: Pixels| {
            div()
                .flex_none()
                .w(width)
                .h(row_height)
                .line_height(row_height)
                .whitespace_nowrap()
                .overflow_hidden()
        };

        let header = h_flex()
            .flex_none()
            .w_full()
            .h(row_height)
            .line_height(row_height)
            .whitespace_nowrap()
            .overflow_hidden()
            .bg(theme.element)
            .text_color(theme.info)
            .child(HEX_HEADER_CONTENT);

        let list = uniform_list("preview-hex", rows, move |range, _, _| {
            range
                .map(|row| {
                    let offset = row * HEX_BYTES_PER_ROW;
                    let chunk = &bytes[offset..(offset + HEX_BYTES_PER_ROW).min(bytes.len())];

                    let mut hex = String::with_capacity(HEX_BYTES_PER_ROW * 3);
                    let mut decoded = String::with_capacity(HEX_BYTES_PER_ROW * 2);

                    for &byte in chunk {
                        hex.push(HEX_LOOKUP[(byte >> 4) as usize] as char);
                        hex.push(HEX_LOOKUP[(byte & 0x0F) as usize] as char);
                        hex.push(' ');
                        decoded.push(decode_byte(byte));
                    }

                    h_flex()
                        .h(row_height)
                        .child(
                            cell(offset_width)
                                .text_color(theme.info)
                                .child(format!(" {offset:08X} ")),
                        )
                        .child(cell(hex_width).child(hex))
                        .child(cell(text_width).child(decoded))
                })
                .collect()
        })
        .size_full()
        .track_scroll(self.hex_scroll.clone());

        v_flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.background)
            .font_family(MONO_FONT_FAMILY)
            .text_size(HEX_FONT_SIZE)
            .text_color(theme.text)
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(list)
                    .child(Scrollbar::vertical(
                        "preview-hex-scrollbar",
                        &self.hex_scroll,
                    )),
            )
            .into_any_element()
    }

    /// Renders the footer shown under the text, binary and audio tabs, as in porter-app: the
    /// asset name, then the line count, byte size or channel layout.
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = *cx.tessera_theme();

        let name = if self.error {
            "<failed to load>".to_string()
        } else {
            self.name.clone()
        };

        let detail = if self.error {
            String::from("(Error)")
        } else if self.unsupported {
            String::from("(Unsupported)")
        } else {
            match self.tab {
                PreviewTab::Viewport => String::new(),
                PreviewTab::Text => format!("Lines: {}", self.text_view.read(cx).line_count()),
                PreviewTab::Binary => format!(
                    "Size: 0x{:02X}",
                    self.raw.as_ref().map(|raw| raw.bytes.len()).unwrap_or(0)
                ),
                PreviewTab::Audio => self
                    .audio_player
                    .as_ref()
                    .map(|player| format!("{} Ch, {}", player.channels(), player.sample_rate()))
                    .unwrap_or_default(),
            }
        };

        h_flex()
            .flex_none()
            .w_full()
            .px_2()
            .py_1()
            .gap_2()
            .justify_between()
            .border_t_1()
            .border_color(theme.border)
            .text_size(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_color(if self.error {
                        theme.warning
                    } else {
                        theme.text
                    })
                    .child(format!("Name: {name}")),
            )
            .child(div().flex_none().text_color(theme.text).child(detail))
    }

    /// Renders the renderer statistics in a box over the top left of the viewport, one
    /// `Header : value` row per statistic, as in porter-app. Error states show a status row.
    fn render_statistics(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let theme = *cx.tessera_theme();

        let rows: Vec<(String, String)> = if self.error {
            vec![(String::from("Status"), String::from("<failed to load>"))]
        } else if self.unsupported {
            vec![(
                String::from("Status"),
                String::from("<not supported for preview>"),
            )]
        } else if self.has_content {
            let mut statistics = self.renderer.statistics();

            if self.control_scheme == PreviewControlScheme::Unreal
                && !self.renderer.is_orthographic()
            {
                statistics.push((
                    String::from("Speed"),
                    format!("{:.2}x", self.renderer.fly_speed()),
                ));
            }

            statistics
        } else {
            Vec::new()
        };

        if rows.is_empty() {
            return None;
        }

        let mut table = v_flex()
            .p_1()
            .gap_0p5()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .bg(palette::with_alpha(theme.element, 0.85))
            .font_family(MONO_FONT_FAMILY)
            .font_weight(FontWeight::BOLD)
            .text_size(HEX_FONT_SIZE);

        for (header, value) in rows {
            table = table.child(
                h_flex()
                    .px_1()
                    .gap_2()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_none()
                            .w(px(64.0))
                            .text_color(theme.info)
                            .child(header),
                    )
                    .child(div().flex_none().text_color(theme.info).child(":"))
                    .child(div().text_color(theme.text).child(value)),
            );
        }

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .p_2()
                .child(table)
                .into_any_element(),
        )
    }

    /// The header title, clipped from the start behind an ellipsis when it is wider than its slot.
    fn clipped_title(&self, window: &Window) -> String {
        if self.name.is_empty() {
            return "Preview".to_string();
        }

        if self.title_width <= px(0.0) {
            return self.name.clone();
        }

        let mut font = window.text_style().font();

        font.weight = FontWeight::SEMIBOLD;

        let shape = |text: &str| {
            let run = TextRun {
                len: text.len(),
                font: font.clone(),
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            window.text_system().shape_line(
                SharedString::from(text.to_string()),
                TITLE_FONT_SIZE,
                &[run],
                None,
            )
        };

        let line = shape(&self.name);

        if line.width <= self.title_width {
            return self.name.clone();
        }

        let ellipsis = "…";
        let available = self.title_width - shape(ellipsis).width;

        // The first character boundary from which the rest of the name fits beside the ellipsis.
        let cut = self
            .name
            .char_indices()
            .map(|(index, _)| index)
            .find(|&index| line.width - line.x_for_index(index) <= available)
            .unwrap_or(self.name.len());

        format!("{ellipsis}{}", &self.name[cut..])
    }

    /// The status message shown over the viewport, if any. Errors are reported in the
    /// statistics box instead.
    fn status_message(&self) -> Option<&'static str> {
        if self.loading || self.error || self.unsupported {
            None
        } else if self.animation_pending {
            Some("Select a model to preview this animation.")
        } else if !self.has_content {
            Some("Select an asset to preview.")
        } else {
            None
        }
    }
}

/// Renders the animation model picker popover: a search input over all
/// loaded model assets, plus a "Bones only" reset row.
fn render_model_picker(
    model_search: Entity<InputState>,
    models: Vec<(u64, String)>,
    bound: bool,
    popover: &Entity<PopoverState>,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let theme = *cx.tessera_theme();

    let bind_row = |id: SharedString, label: SharedString, action: BindAnimationModelAction| {
        let popover = popover.clone();

        div()
            .id(id)
            .w_full()
            .h(px(28.0))
            .flex_shrink_0()
            .px_2()
            .py_1()
            .rounded(theme.radius)
            .text_size(px(12.5))
            .text_color(theme.text)
            .whitespace_nowrap()
            .overflow_hidden()
            .cursor_pointer()
            .hover(move |this| this.bg(theme.border))
            .child(label)
            .on_click(move |_, window, cx| {
                cx.stop_propagation();

                window.dispatch_action(Box::new(action.clone()), cx);

                popover.update(cx, |popover, cx| popover.dismiss(cx));
            })
    };

    let mut rows = v_flex().gap_0p5();

    if bound {
        rows = rows.child(bind_row(
            "anim-model-none".into(),
            "Bones only".into(),
            BindAnimationModelAction(None),
        ));
    }

    if models.is_empty() {
        rows = rows.child(
            div()
                .px_2()
                .py_1()
                .text_size(px(12.5))
                .text_color(theme.text_muted)
                .child("No models found"),
        );
    }

    for (index, name) in models {
        rows = rows.child(bind_row(
            SharedString::from(format!("anim-model-{index}")),
            name.into(),
            BindAnimationModelAction(Some(index)),
        ));
    }

    popover_style(v_flex(), &theme)
        .w(px(320.0))
        .p_1()
        .gap_1()
        .child(Input::new(&model_search).small())
        .child(
            div()
                .h(px(320.0))
                .child(scrollable_y("anim-model-rows", rows, window, cx)),
        )
        .into_any_element()
}

/// Samples the current frame of an animation preview into a skeleton.
fn sample_skeleton(animation: &mut AnimationPreview) -> Result<Skeleton, PreviewError> {
    let mut skeleton = Skeleton::with_capacity(animation.template.len());

    for (index, (name, parent)) in animation.template.iter().enumerate() {
        let joint = animation
            .sampler
            .evaluate_joint_index(index)
            .unwrap_or_default();

        // Scale comes across with the rest of the transform: the skinning matrix is the posed
        // world matrix against the inverse of the rest one, and both include scale, so leaving it
        // at the default here deforms every bone the rest pose scaled.
        skeleton.bones.push(
            Bone::new(name.clone(), *parent)
                .local_position(joint.local_position)
                .local_rotation(joint.local_rotation)
                .local_scale(joint.local_scale)
                .world_position(joint.world_position)
                .world_rotation(joint.world_rotation)
                .world_scale(joint.world_scale),
        );
    }

    Ok(skeleton)
}

/// Builds an animation sampler with joints from the given skeleton, or
/// free-floating joints for every animated node when no skeleton is bound.
/// Returns the sampler and the joints it was seeded with, as (name, parent).
fn build_animation_sampler(
    animation: &Animation,
    skeleton: Option<&Skeleton>,
) -> (AnimationSampler, Vec<(Option<String>, i32)>) {
    // The sampler only evaluates translate/rotation curves for joints, so keep just those. Each
    // curve's keyframes are one value type by construction, so there is nothing to check.
    let mut sampled = Animation::new(animation.frame_rate, animation.looping);

    sampled.up_axis = animation.up_axis;
    sampled.curves = animation
        .curves
        .iter()
        .filter(|curve| {
            matches!(
                curve.attribute(),
                CurveAttribute::Translate | CurveAttribute::Rotate | CurveAttribute::Scale
            )
        })
        .cloned()
        .collect();

    // The sampler asserts keyframes are sorted by time; some source formats
    // store them out of order, so sort each curve before sampling.
    for curve in &mut sampled.curves {
        curve.sort();
    }

    // The sampler treats every curve as an absolute value, but a viewmodel animation stores its
    // translation relative to the bone's rest position, so its curves are rebased onto that rest
    // pose here. Without this a bone lands at the delta itself and its whole chain is displaced.
    if let Some(skeleton) = skeleton {
        let rest: HashMap<&str, (Vector3, Quaternion, Vector3)> = skeleton
            .bones
            .iter()
            .filter_map(|bone| {
                bone.name.as_deref().map(|name| {
                    (
                        name,
                        (bone.local_position, bone.local_rotation, bone.local_scale),
                    )
                })
            })
            .collect();

        for curve in &mut sampled.curves {
            let attribute = curve.attribute();

            // A per bone override wins over the curve's own data type, for the attributes it
            // names.
            let data_type = animation
                .curve_mode_overrides
                .iter()
                .find(|over| {
                    over.name == curve.name()
                        && match attribute {
                            CurveAttribute::Translate => over.override_translate,
                            CurveAttribute::Rotate => over.override_rotation,
                            CurveAttribute::Scale => over.override_scale,
                            _ => false,
                        }
                })
                .map_or_else(|| curve.data_type(), |over| over.data_type);

            if matches!(data_type, CurveDataType::Absolute) {
                continue;
            }

            let Some((rest_position, rest_rotation, rest_scale)) = rest.get(curve.name()).copied()
            else {
                continue;
            };

            match curve.keyframes_mut() {
                Keyframes::Translate(keyframes) => {
                    for keyframe in keyframes {
                        keyframe.value = rest_position + keyframe.value;
                    }
                }
                Keyframes::Scale(keyframes) => {
                    for keyframe in keyframes {
                        keyframe.value = Vector3::new(
                            rest_scale.x * keyframe.value.x,
                            rest_scale.y * keyframe.value.y,
                            rest_scale.z * keyframe.value.z,
                        );
                    }
                }
                Keyframes::Rotate(keyframes) => {
                    for keyframe in keyframes {
                        keyframe.value = rest_rotation * keyframe.value;
                    }
                }
                _ => {}
            }

            // Rebased onto the rest pose, so the sampler's absolute handling is now correct.
            curve.set_data_type(CurveDataType::Absolute);
        }
    }

    let animation = &sampled;

    let mut sampler = AnimationSampler::new(animation.clone());
    let mut template = Vec::new();

    if let Some(skeleton) = skeleton {
        for bone in &skeleton.bones {
            // The sampler caps how many joints it will take; stop at the cap rather than letting
            // the template run ahead of the joints it is supposed to describe.
            if sampler
                .push_joint(
                    bone.name.clone(),
                    Joint::new(bone.parent)
                        .local_position(bone.local_position)
                        .local_rotation(bone.local_rotation)
                        .local_scale(bone.local_scale),
                )
                .is_err()
            {
                break;
            }

            template.push((bone.name.clone(), bone.parent));
        }
    } else {
        for curve in &animation.curves {
            if !matches!(
                curve.attribute(),
                CurveAttribute::Translate | CurveAttribute::Rotate | CurveAttribute::Scale
            ) {
                continue;
            }

            if template
                .iter()
                .any(|(name, _): &(Option<String>, i32)| name.as_deref() == Some(curve.name()))
            {
                continue;
            }

            if sampler
                .push_joint(Some(curve.name().to_string()), Joint::new(-1))
                .is_err()
            {
                break;
            }

            template.push((Some(curve.name().to_string()), -1));
        }
    }

    (sampler, template)
}

/// Keeps subsecond sound effects distinguishable from empty audio.
fn format_position(seconds: f32) -> String {
    let total = (seconds.max(0.0) * 1000.0) as u64;

    format!(
        "{:02}:{:02}.{:03}",
        total / 60_000,
        total / 1000 % 60,
        total % 1000
    )
}

impl Render for PreviewPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_frame_if_dirty(window);

        let entity = cx.entity();

        // Probe the viewport bounds so the renderer matches the pane size.
        //
        // The canvas has no intrinsic size, so it must be styled to fill the
        // viewport area or its bounds report a zero height.
        let bounds_probe = canvas(
            move |bounds, window, cx: &mut App| {
                let width = f32::from(bounds.size.width);
                let height = f32::from(bounds.size.height);
                let scale = window.scale_factor();

                cx.defer(move |cx| {
                    entity.update(cx, |pane, cx| pane.update_bounds(width, height, scale, cx));
                });
            },
            |_, _, _, _| {},
        )
        .size_full();

        // Probe the title slot so the name can be clipped to it next frame.
        let title_probe = canvas(
            {
                let entity = cx.entity();

                move |bounds, _, cx: &mut App| {
                    let width = bounds.size.width;

                    cx.defer(move |cx| {
                        entity.update(cx, |pane, cx| {
                            if pane.title_width != width {
                                pane.title_width = width;
                                cx.notify();
                            }
                        });
                    });
                }
            },
            |_, _, _, _| {},
        )
        .size_full();

        let mut header = h_flex()
            .w_full()
            .min_w_0()
            .px_3()
            .py_1p5()
            .gap_2()
            .items_center()
            .border_b_1()
            .border_color(cx.tessera_theme().border)
            .child(
                // Long asset paths are cut from the front, behind a leading ellipsis, so the
                // asset's own name stays visible and can't push the pane wider than its slot.
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(TITLE_FONT_SIZE)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.tessera_theme().text_secondary)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().absolute().inset_0().child(title_probe))
                    .child(self.clipped_title(window)),
            );

        // Animations can be bound to a model asset's skeleton.
        if self.is_animation_preview() {
            let models = self.animation_models.clone();
            let model_search = self.model_search.clone();
            let model_name = self
                .animation
                .as_ref()
                .and_then(|animation| animation.model_name.clone());
            let bound = model_name.is_some();
            let current: SharedString = model_name
                .unwrap_or_else(|| {
                    String::from(if self.animation_pending {
                        "Select model"
                    } else {
                        "Bones only"
                    })
                })
                .into();

            header = header.child(
                div()
                    .flex_shrink()
                    .min_w_0()
                    .max_w(px(280.0))
                    .overflow_hidden()
                    .child(
                        Popover::new(
                            "anim-model-picker",
                            move |open| {
                                Button::new("anim-model")
                                    .xsmall()
                                    .outline()
                                    .selected(open)
                                    .label(current)
                                    .into_any_element()
                            },
                            move |popover, window, cx| {
                                render_model_picker(
                                    model_search.clone(),
                                    models.clone(),
                                    bound,
                                    popover,
                                    window,
                                    cx,
                                )
                            },
                        )
                        .anchor(Corner::TopRight),
                    ),
            );
        }

        let mut viewport = div()
            .id("preview-viewport")
            .relative()
            .flex_1()
            .w_full()
            .overflow_hidden()
            .bg(cx.tessera_theme().background)
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel));

        for button in [
            gpui::MouseButton::Left,
            gpui::MouseButton::Right,
            gpui::MouseButton::Middle,
        ] {
            viewport = viewport
                .on_mouse_down(
                    button,
                    cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                        this.on_mouse_down(event, window, cx);
                    }),
                )
                .on_mouse_up(
                    button,
                    cx.listener(move |this, _: &gpui::MouseUpEvent, _, _| {
                        this.on_mouse_up(button);
                    }),
                )
                .on_mouse_up_out(
                    button,
                    cx.listener(move |this, _: &gpui::MouseUpEvent, _, _| {
                        this.on_mouse_up(button);
                    }),
                );
        }

        viewport = viewport
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                // Handled fly keys stop here, so the hotkeys they share
                // (wireframe, export) don't fire while flying.
                if this.fly_key(&event.keystroke.key, true, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, cx| {
                this.fly_key(&event.keystroke.key, false, cx);
            }))
            .child(div().absolute().inset_0().child(bounds_probe));

        if self.has_content
            && let Some(frame) = self.frame.clone()
        {
            // Explicit absolute pixel sizing: the img element sets an aspect
            // ratio style which corrupts the surrounding flex layout when
            // combined with percentage sizes.
            let width = px(self.size.0 / self.scale);
            let height = px(self.size.1 / self.scale);

            viewport = viewport.child(img(frame).absolute().top_0().left_0().w(width).h(height));
        }

        if self.loading {
            viewport = viewport.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Spinner::new()),
            );
        } else if let Some(message) = self.status_message() {
            viewport = viewport.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .text_color(cx.tessera_theme().text_muted)
                    .child(message),
            );
        }

        if !self.loading
            && let Some(statistics) = self.render_statistics(cx)
        {
            viewport = viewport.child(statistics);
        }

        if self.show_overlay && self.has_content && !self.loading {
            viewport = viewport.child(self.render_overlay_controls(cx));
        }

        let body = match self.tab {
            PreviewTab::Viewport => viewport.into_any_element(),
            PreviewTab::Text => self.render_text(cx),
            PreviewTab::Binary => self.render_hex(window, cx),
            PreviewTab::Audio => self.render_audio(cx).into_any_element(),
        };

        let mut pane = v_flex()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .border_t_1()
            .border_l_1()
            .border_r_1()
            .border_b_1()
            .rounded(cx.tessera_theme().radius)
            .border_color(cx.tessera_theme().border)
            .bg(cx.tessera_theme().surface)
            .child(header)
            .child(body);

        if self.animation.is_some() && !self.loading && self.tab == PreviewTab::Viewport {
            pane = pane.child(self.render_animation_controls(cx));
        }

        if self.tab != PreviewTab::Viewport {
            pane = pane.child(self.render_footer(cx));
        }

        // The tab row sits at the bottom of the pane, under the footer, as porter-app lays it out.
        pane.child(self.render_tabs(cx))
    }
}

/// The viewport toggle buttons drawn over the preview, as (id, icon, hotkey, tooltip).
///
/// Each one fires the same path as its hotkey, so the buttons and keys can't drift apart.
const OVERLAY_CONTROLS: &[(&str, IconName, &str, &str)] = &[
    (
        "preview-reset-view",
        IconName::RotateCcw,
        "r",
        "Reset View [R]",
    ),
    (
        "preview-toggle-grid",
        IconName::Grid3x3,
        "g",
        "Toggle Grid [G]",
    ),
    (
        "preview-toggle-bones",
        IconName::Bone,
        "b",
        "Toggle Bones [B]",
    ),
    (
        "preview-toggle-wireframe",
        IconName::Pentagon,
        "w",
        "Toggle Wireframe [W]",
    ),
    (
        "preview-toggle-shaded",
        IconName::Droplet,
        "m",
        "Toggle Shaded [M]",
    ),
    (
        "preview-cycle-material",
        IconName::Images,
        "n",
        "Cycle Image [N]",
    ),
];

/// The font size of the header title.
const TITLE_FONT_SIZE: Pixels = px(12.5);

/// Shown on the text tab when the raw file isn't text.
const TEXT_UNSUPPORTED: &str = "<This asset can't be represented as text>";
/// The font size of the text, binary and statistics views.
const HEX_FONT_SIZE: Pixels = px(12.0);
/// Bytes shown per hex row.
const HEX_BYTES_PER_ROW: usize = 16;
/// Hex digit lookup table.
const HEX_LOOKUP: &[u8; 16] = b"0123456789ABCDEF";
/// The pinned header above the hex rows, as porter-app lays it out.
const HEX_HEADER_CONTENT: &str =
    "  Offset   00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F  Decoded text";
/// Measured for the offset column width.
const HEX_OFFSET_CONTENT: &str = " 00000000  ";
/// Measured for the hex column width.
const HEX_BYTES_CONTENT: &str = "00 11 22 33 44 55 66 77 88 99 AA BB CC DD EE FF  ";
/// Measured for the decoded text column width.
const HEX_TEXT_CONTENT: &str = "DEADBEEFDEADBEEF ";

/// Windows-1252 printable characters for 0x80-0x9F, as porter-app decodes them.
const ANSI_TO_UNICODE: [char; 32] = [
    '\u{20AC}', '.', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '.', '\u{17D}', '.', '.', '\u{2018}',
    '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2DC}', '\u{2122}',
    '\u{161}', '\u{203A}', '\u{153}', '.', '\u{17E}', '\u{178}',
];

/// The character shown for a byte in the decoded column: printable ascii, Latin-1 and the
/// Windows-1252 extras, with everything else as a dot.
fn decode_byte(byte: u8) -> char {
    let character = byte as char;

    if character.is_ascii_alphanumeric()
        || character.is_ascii_punctuation()
        || byte == 0x20
        || (0xA0..=0xFF).contains(&byte)
    {
        character
    } else if byte == 0x80
        || (0x82..=0x8C).contains(&byte)
        || byte == 0x8E
        || (0x91..=0x9C).contains(&byte)
        || (0x9E..=0x9F).contains(&byte)
    {
        ANSI_TO_UNICODE[(byte - 0x80) as usize]
    } else {
        '.'
    }
}

/// Hides the cursor and pins it in place while dragging in the viewport, the
/// way Unreal's viewport does, so drags can't run out of the viewport.
mod cursor_lock {
    use gpui::Pixels;
    use gpui::Point;

    /// A locked cursor: where it sits in the window, and on screen.
    pub(super) struct CursorLock {
        /// The cursor position in window coordinates, as gpui reports it.
        pub position: Point<Pixels>,
        #[cfg(target_os = "windows")]
        screen: (i32, i32),
    }

    /// Hides the cursor, pinned at the given window position.
    #[cfg(target_os = "windows")]
    pub(super) fn lock(position: Point<Pixels>) -> Option<CursorLock> {
        use windows_sys::Win32::Foundation::POINT;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
        use windows_sys::Win32::UI::WindowsAndMessaging::ShowCursor;

        let mut point = POINT { x: 0, y: 0 };

        // SAFETY: `point` is a valid out pointer for the duration of the call.
        if unsafe { GetCursorPos(&mut point) } == 0 {
            return None;
        }

        // SAFETY: only adjusts the calling thread's cursor display count, and
        // `unlock` restores it.
        unsafe { ShowCursor(0) };

        Some(CursorLock {
            position,
            screen: (point.x, point.y),
        })
    }

    /// Moves the cursor back to the lock point.
    #[cfg(target_os = "windows")]
    pub(super) fn recenter(lock: &CursorLock) {
        use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;

        // SAFETY: plain call with screen coordinates.
        unsafe { SetCursorPos(lock.screen.0, lock.screen.1) };
    }

    /// Shows the cursor again, at the lock point.
    #[cfg(target_os = "windows")]
    pub(super) fn unlock(lock: CursorLock) {
        use windows_sys::Win32::UI::WindowsAndMessaging::ShowCursor;

        recenter(&lock);

        // SAFETY: balances the `ShowCursor(0)` made in `lock`.
        unsafe { ShowCursor(1) };
    }

    /// Cursor locking is only implemented on Windows.
    #[cfg(not(target_os = "windows"))]
    pub(super) fn lock(_: Point<Pixels>) -> Option<CursorLock> {
        None
    }

    #[cfg(not(target_os = "windows"))]
    pub(super) fn recenter(_: &CursorLock) {}

    #[cfg(not(target_os = "windows"))]
    pub(super) fn unlock(_: CursorLock) {}
}
