use std::ops::Range;

use gpui::App;
use gpui::ClipboardItem;
use gpui::Context;
use gpui::CursorStyle;
use gpui::Entity;
use gpui::FocusHandle;
use gpui::Focusable;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement;
use gpui::Point;
use gpui::Render;
use gpui::RenderOnce;
use gpui::ScrollHandle;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Styled;
use gpui::StyledText;
use gpui::TextLayout;
use gpui::Window;
use gpui::div;
use gpui::px;

use crate::components::Scrollbar;
use crate::palette;
use crate::theme::ActiveTesseraTheme;

/// A read-only, wrapping text view with mouse selection and copy.
///
/// Selection is by byte offset into the text, mapped to and from the pointer through the layout
/// of the last paint, so it follows line wrapping. Ctrl/Cmd+C copies the selection and Ctrl/Cmd+A
/// selects everything; there is no editing.
pub(crate) struct TextViewState {
    focus_handle: FocusHandle,
    text: SharedString,
    selection: Range<usize>,
    selecting: bool,
    /// The layout of the last paint, for mapping pointer positions to offsets.
    layout: TextLayout,
    scroll: ScrollHandle,
}

impl Focusable for TextViewState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextViewState {
    /// Constructs an empty view.
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            text: SharedString::default(),
            selection: 0..0,
            selecting: false,
            layout: TextLayout::default(),
            scroll: ScrollHandle::new(),
        }
    }

    /// Replaces the text, clearing the selection and scrolling back to the top.
    pub(crate) fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.text = text.into();
        self.selection = 0..0;
        self.selecting = false;
        self.scroll.set_offset(Point::default());
        cx.notify();
    }

    /// The number of lines of the text, counting the last line even without a newline.
    pub(crate) fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }

    /// The text offset under the pointer, snapping past the ends of lines.
    fn offset_at(&self, position: Point<gpui::Pixels>) -> usize {
        match self.layout.index_for_position(position) {
            Ok(offset) | Err(offset) => offset.min(self.text.len()),
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle);

        let offset = self.offset_at(event.position);

        self.selection = offset..offset;
        self.selecting = true;
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selecting {
            return;
        }

        let offset = self.offset_at(event.position);
        let start = self.selection.start;

        self.selection = if offset < start {
            offset..start
        } else {
            start..offset
        };

        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.selecting = false;
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let keystroke = &event.keystroke;

        if !keystroke.modifiers.secondary() {
            return;
        }

        match keystroke.key.as_str() {
            "c" if !self.selection.is_empty() => {
                let selected = &self.text[self.selection.clone()];

                cx.write_to_clipboard(ClipboardItem::new_string(selected.to_string()));
                cx.stop_propagation();
            }
            "a" => {
                self.selection = 0..self.text.len();
                cx.stop_propagation();
                cx.notify();
            }
            _ => {}
        }
    }
}

impl Render for TextViewState {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let style = window.text_style();
        let selection = self.selection.clone();
        let len = self.text.len();

        // The selected range is drawn as a run with a background, so it wraps with the text.
        let runs = [
            (0..selection.start, false),
            (selection.start..selection.end, true),
            (selection.end..len, false),
        ]
        .into_iter()
        .filter(|(range, _)| !range.is_empty())
        .map(|(range, selected)| {
            let mut run = style.to_run(range.len());

            if selected {
                run.background_color = Some(palette::with_alpha(theme.primary, 0.35).into());
            }

            run
        })
        .collect();

        let text = StyledText::new(self.text.clone()).with_runs(runs);

        self.layout = text.layout().clone();

        div()
            .id("text-view")
            .relative()
            .size_full()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                div()
                    .id("text-view-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .child(div().w_full().p_1().pr_4().child(text)),
            )
            .child(Scrollbar::vertical("text-view-scrollbar", &self.scroll))
    }
}

/// Renders a [`TextViewState`] filling its container.
#[derive(IntoElement)]
pub(crate) struct TextView {
    state: Entity<TextViewState>,
}

impl TextView {
    /// Constructs a view of the given state.
    pub(crate) fn new(state: &Entity<TextViewState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for TextView {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        div().size_full().min_h(px(0.0)).child(self.state)
    }
}
