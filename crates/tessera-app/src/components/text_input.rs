use std::ops::Range;
use std::time::Duration;

use gpui::App;
use gpui::Bounds;
use gpui::ClipboardItem;
use gpui::Context;
use gpui::CursorStyle;
use gpui::Element;
use gpui::ElementId;
use gpui::ElementInputHandler;
use gpui::Entity;
use gpui::EntityInputHandler;
use gpui::EventEmitter;
use gpui::FocusHandle;
use gpui::Focusable;
use gpui::GlobalElementId;
use gpui::InspectorElementId;
use gpui::InteractiveElement;
use gpui::IntoElement;
use gpui::KeyBinding;
use gpui::LayoutId;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseMoveEvent;
use gpui::MouseUpEvent;
use gpui::PaintQuad;
use gpui::ParentElement;
use gpui::Pixels;
use gpui::Point;
use gpui::Render;
use gpui::RenderOnce;
use gpui::ShapedLine;
use gpui::SharedString;
use gpui::StatefulInteractiveElement;
use gpui::Style;
use gpui::StyleRefinement;
use gpui::Styled;
use gpui::Task;
use gpui::TextRun;
use gpui::UTF16Selection;
use gpui::UnderlineStyle;
use gpui::Window;
use gpui::actions;
use gpui::div;
use gpui::fill;
use gpui::point;
use gpui::prelude::FluentBuilder;
use gpui::px;
use gpui::relative;
use gpui::size;

use crate::components::Disableable;
use crate::components::Icon;
use crate::components::IconName;
use crate::components::Sizable;
use crate::components::Size;
use crate::palette;
use crate::theme::ActiveTesseraTheme;

/// The key context text inputs bind their keys in.
const CONTEXT: &str = "TesseraInput";
/// How long the caret stays on, then off, while blinking.
const BLINK_INTERVAL: Duration = Duration::from_millis(530);
/// How long the caret holds steady after an edit or caret move before blinking resumes.
const BLINK_PAUSE: Duration = Duration::from_millis(300);
/// The caret is an integer width so it stays crisp.
const CURSOR_WIDTH: Pixels = px(2.0);

actions!(
    tessera_input,
    [
        Backspace,
        Delete,
        Left,
        Right,
        SelectLeft,
        SelectRight,
        SelectAll,
        Home,
        End,
        SelectToHome,
        SelectToEnd,
        WordLeft,
        WordRight,
        SelectWordLeft,
        SelectWordRight,
        DeleteWordLeft,
        DeleteWordRight,
        Paste,
        Cut,
        Copy,
        Enter,
    ]
);

/// Registers the text input key bindings.
pub(crate) fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(CONTEXT)),
        KeyBinding::new("delete", Delete, Some(CONTEXT)),
        KeyBinding::new("left", Left, Some(CONTEXT)),
        KeyBinding::new("right", Right, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("home", Home, Some(CONTEXT)),
        KeyBinding::new("end", End, Some(CONTEXT)),
        KeyBinding::new("shift-home", SelectToHome, Some(CONTEXT)),
        KeyBinding::new("shift-end", SelectToEnd, Some(CONTEXT)),
        KeyBinding::new("enter", Enter, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-left", WordLeft, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-right", WordRight, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-shift-left", SelectWordLeft, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-shift-right", SelectWordRight, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-backspace", DeleteWordLeft, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-delete", DeleteWordRight, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-left", WordLeft, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-right", WordRight, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-backspace", DeleteWordLeft, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-delete", DeleteWordRight, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, Some(CONTEXT)),
    ]);
}

/// Events emitted by a text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputEvent {
    /// The text changed.
    Change,
    /// Enter was pressed.
    PressEnter,
    /// The input lost focus.
    Blur,
}

/// The editable state of a single line text input.
pub(crate) struct InputState {
    focus_handle: FocusHandle,
    content: SharedString,
    placeholder: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    last_layout: Option<ShapedLine>,
    last_bounds: Option<Bounds<Pixels>>,
    /// How far the text is scrolled left to keep the cursor in view.
    scroll_x: Pixels,
    is_selecting: bool,
    /// The most characters the content may hold, when limited.
    max_length: Option<usize>,
    /// Whether the caret is in the "on" half of its blink.
    cursor_visible: bool,
    /// Set while the caret holds steady after input; cleared by the pause timer.
    blink_paused: bool,
    /// Bumped whenever the blink restarts, so stale timers do nothing.
    blink_epoch: usize,
    blink_task: Task<()>,
    _focus: gpui::Subscription,
    _blur: gpui::Subscription,
}

impl EventEmitter<InputEvent> for InputState {}

impl Focusable for InputState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl InputState {
    /// Constructs an empty input.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let focus = cx.on_focus(&focus_handle, window, |this, _, cx| {
            this.start_blink(cx);
        });
        let blur = cx.on_blur(&focus_handle, window, |this, _, cx| {
            this.stop_blink(cx);
            cx.emit(InputEvent::Blur);
        });

        Self {
            focus_handle,
            content: SharedString::default(),
            placeholder: SharedString::default(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            last_layout: None,
            last_bounds: None,
            scroll_x: px(0.0),
            is_selecting: false,
            max_length: None,
            cursor_visible: false,
            blink_paused: false,
            blink_epoch: 0,
            blink_task: Task::ready(()),
            _focus: focus,
            _blur: blur,
        }
    }

    /// Starts blinking the caret, from the visible state.
    fn start_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = false;
        self.blink_paused = false;
        self.blink(self.blink_epoch, cx);
    }

    /// Stops blinking; the caret is not painted while unfocused.
    fn stop_blink(&mut self, cx: &mut Context<Self>) {
        self.blink_epoch += 1;
        self.blink_task = Task::ready(());
        cx.notify();
    }

    /// Toggles the caret and schedules the next toggle, unless this blink was superseded.
    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if self.blink_paused || epoch != self.blink_epoch {
            return;
        }

        self.cursor_visible = !self.cursor_visible;
        cx.notify();

        self.blink_epoch += 1;

        let epoch = self.blink_epoch;

        self.blink_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BLINK_INTERVAL).await;

            let _ = this.update(cx, |this, cx| this.blink(epoch, cx));
        });
    }

    /// Shows the caret at once and holds it until input pauses, then blinks again.
    fn touch_cursor(&mut self, cx: &mut Context<Self>) {
        self.cursor_visible = true;
        self.blink_paused = true;
        self.blink_epoch += 1;

        let epoch = self.blink_epoch;

        self.blink_task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BLINK_PAUSE).await;

            let _ = this.update(cx, |this, cx| {
                if epoch == this.blink_epoch {
                    this.blink_paused = false;
                    this.blink(epoch, cx);
                }
            });
        });
    }

    /// Whether the caret is painted this frame.
    fn cursor_shown(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
            && (self.cursor_visible || self.blink_paused)
            && window.is_window_active()
    }

    /// Sets the text shown while the input is empty.
    pub(crate) fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Limits the content to the given number of characters; longer input is cut.
    pub(crate) fn max_length(mut self, max_length: usize) -> Self {
        self.max_length = Some(max_length);
        self
    }

    /// Sets the initial text.
    pub(crate) fn default_value(mut self, value: impl Into<SharedString>) -> Self {
        self.content = value.into();
        self.selected_range = self.content.len()..self.content.len();
        self
    }

    /// The current text.
    pub(crate) fn value(&self) -> SharedString {
        self.content.clone()
    }

    /// Replaces the text, without emitting a change.
    pub(crate) fn set_value(
        &mut self,
        value: impl Into<SharedString>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.content = value.into();
        self.selected_range = self.content.len()..self.content.len();
        self.selection_reversed = false;
        self.marked_range = None;
        cx.notify();
    }

    /// Focuses the input, selecting its text.
    pub(crate) fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus_handle);
        self.selected_range = 0..self.content.len();
        self.selection_reversed = false;
        cx.notify();
    }

    /// Clears the text, emitting a change.
    fn clean(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selected_range = 0..self.content.len();
        self.replace_text_in_range(None, "", window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), cx);
        } else {
            self.move_to(self.selected_range.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            self.move_to(self.next_boundary(self.selected_range.end), cx);
        } else {
            self.move_to(self.selected_range.end, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_boundary(self.cursor_offset()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_boundary(self.cursor_offset()), cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
        self.select_to(self.content.len(), cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(0, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.content.len(), cx);
    }

    fn select_to_home(&mut self, _: &SelectToHome, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_to_end(&mut self, _: &SelectToEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.content.len(), cx);
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.previous_word_start(self.cursor_offset()), cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.next_word_end(self.cursor_offset()), cx);
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.previous_word_start(self.cursor_offset()), cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.next_word_end(self.cursor_offset()), cx);
    }

    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.previous_word_start(self.cursor_offset()), cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.selected_range.is_empty() {
            self.select_to(self.next_word_end(self.cursor_offset()), cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let previous = self.previous_boundary(self.cursor_offset());

            if previous == self.cursor_offset() {
                return;
            }

            self.select_to(previous, cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected_range.is_empty() {
            let next = self.next_boundary(self.cursor_offset());

            if next == self.cursor_offset() {
                return;
            }

            self.select_to(next, cx);
        }

        self.replace_text_in_range(None, "", window, cx);
    }

    fn enter(&mut self, _: &Enter, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(InputEvent::PressEnter);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text.replace(['\r', '\n'], " "), window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.content[self.selected_range.clone()].to_string(),
            ));

            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;

        let index = self.index_for_mouse_position(event.position);

        if event.click_count >= 3 {
            self.move_to(0, cx);
            self.select_to(self.content.len(), cx);
        } else if event.click_count == 2 {
            // A double click takes the word under the pointer.
            let range = word_range_at(&self.content, index).unwrap_or(index..index);

            self.move_to(range.start, cx);
            self.select_to(range.end, cx);
        } else if event.modifiers.shift {
            self.select_to(index, cx);
        } else {
            self.move_to(index, cx);
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            self.select_to(self.index_for_mouse_position(event.position), cx);
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected_range = offset..offset;
        self.selection_reversed = false;
        self.touch_cursor(cx);
        cx.notify();
    }

    fn cursor_offset(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if self.content.is_empty() {
            return 0;
        }

        let (Some(bounds), Some(line)) = (self.last_bounds.as_ref(), self.last_layout.as_ref())
        else {
            return 0;
        };

        if position.y < bounds.top() {
            return 0;
        }

        if position.y > bounds.bottom() {
            return self.content.len();
        }

        line.closest_index_for_x(position.x - bounds.left() + self.scroll_x)
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }

        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }

        self.touch_cursor(cx);
        cx.notify();
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.content.chars() {
            if utf16_count >= offset {
                break;
            }

            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.content.chars() {
            if utf8_count >= offset {
                break;
            }

            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn range_to_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(range.start)..self.offset_to_utf16(range.end)
    }

    fn range_from_utf16(&self, range_utf16: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range_utf16.start)..self.offset_from_utf16(range_utf16.end)
    }

    /// The grapheme boundary before the offset.
    fn previous_boundary(&self, offset: usize) -> usize {
        previous_grapheme(&self.content, offset)
    }

    /// The grapheme boundary after the offset.
    fn next_boundary(&self, offset: usize) -> usize {
        next_grapheme(&self.content, offset)
    }

    /// The start of the word before the offset, skipping whitespace in between.
    fn previous_word_start(&self, offset: usize) -> usize {
        let mut chars = self.content[..offset].char_indices().rev().peekable();

        while chars
            .peek()
            .is_some_and(|(_, ch)| CharacterKind::from(*ch) == CharacterKind::Whitespace)
        {
            chars.next();
        }

        let Some((index, ch)) = chars.next() else {
            return 0;
        };

        let mut start = index;

        if CharacterKind::from(ch) == CharacterKind::Word {
            while let Some((index, _)) =
                chars.next_if(|(_, ch)| CharacterKind::from(*ch) == CharacterKind::Word)
            {
                start = index;
            }
        }

        start
    }

    /// The end of the word after the offset, skipping whitespace in between.
    fn next_word_end(&self, offset: usize) -> usize {
        let mut chars = self.content[offset..].char_indices().peekable();

        while chars
            .peek()
            .is_some_and(|(_, ch)| CharacterKind::from(*ch) == CharacterKind::Whitespace)
        {
            chars.next();
        }

        let Some((index, ch)) = chars.next() else {
            return self.content.len();
        };

        let mut end = index + ch.len_utf8();

        if CharacterKind::from(ch) == CharacterKind::Word {
            while let Some((index, ch)) =
                chars.next_if(|(_, ch)| CharacterKind::from(*ch) == CharacterKind::Word)
            {
                end = index + ch.len_utf8();
            }
        }

        offset + end
    }

    /// Replaces a range of the content, returning the byte length of what was inserted after
    /// the character limit cut it.
    fn replace_content(&mut self, range: Range<usize>, new_text: &str) -> usize {
        let new_text = match self.max_length {
            Some(max_length) => {
                let kept = self.content[..range.start].chars().count()
                    + self.content[range.end..].chars().count();
                let room = max_length.saturating_sub(kept);
                let end = new_text
                    .char_indices()
                    .nth(room)
                    .map_or(new_text.len(), |(index, _)| index);

                &new_text[..end]
            }
            None => new_text,
        };

        self.content =
            (self.content[..range.start].to_owned() + new_text + &self.content[range.end..]).into();

        new_text.len()
    }
}

impl Render for InputState {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Input::new(&cx.entity())
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));

        Some(self.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected_range),
            reversed: self.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let inserted = self.replace_content(range.clone(), new_text);

        self.selected_range = range.start + inserted..range.start + inserted;
        self.selection_reversed = false;
        self.marked_range = None;
        self.touch_cursor(cx);

        cx.emit(InputEvent::Change);
        cx.notify();
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .or(self.marked_range.clone())
            .unwrap_or(self.selected_range.clone());

        let inserted = self.replace_content(range.clone(), new_text);

        self.marked_range = (inserted > 0).then(|| range.start..range.start + inserted);
        self.selected_range = new_selected_range_utf16
            .as_ref()
            .map(|range_utf16| self.range_from_utf16(range_utf16))
            .map(|new_range| new_range.start + range.start..new_range.end + range.start)
            .unwrap_or_else(|| range.start + inserted..range.start + inserted);
        self.touch_cursor(cx);

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let last_layout = self.last_layout.as_ref()?;
        let range = self.range_from_utf16(&range_utf16);

        Some(Bounds::from_corners(
            point(
                bounds.left() + last_layout.x_for_index(range.start) - self.scroll_x,
                bounds.top(),
            ),
            point(
                bounds.left() + last_layout.x_for_index(range.end) - self.scroll_x,
                bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let line_point = self.last_bounds?.localize(&point)?;
        let last_layout = self.last_layout.as_ref()?;
        let utf8_index = last_layout.index_for_x(line_point.x + self.scroll_x)?;

        Some(self.offset_to_utf16(utf8_index))
    }
}

/// Paints the text, selection and cursor of an input, and registers it for text input.
struct TextElement {
    input: Entity<InputState>,
    placeholder_color: gpui::Hsla,
    selection_color: gpui::Hsla,
    cursor_color: gpui::Hsla,
}

#[doc(hidden)]
pub(crate) struct TextPrepaintState {
    line: Option<ShapedLine>,
    cursor: Option<PaintQuad>,
    selection: Option<PaintQuad>,
    scroll_x: Pixels,
}

impl IntoElement for TextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextElement {
    type RequestLayoutState = ();
    type PrepaintState = TextPrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
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
        let mut style = Style::default();
        style.size.width = relative(1.0).into();
        style.size.height = window.line_height().into();
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.min_size.width = px(0.0).into();

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let content = input.content.clone();
        let selected_range = input.selected_range.clone();
        let cursor = input.cursor_offset();
        let style = window.text_style();

        let (display_text, text_color) = if content.is_empty() {
            (input.placeholder.clone(), self.placeholder_color)
        } else {
            (content, style.color)
        };

        let run = TextRun {
            len: display_text.len(),
            font: style.font(),
            color: text_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let runs = match input.marked_range.as_ref() {
            Some(marked_range) => [
                TextRun {
                    len: marked_range.start,
                    ..run.clone()
                },
                TextRun {
                    len: marked_range.end - marked_range.start,
                    underline: Some(UnderlineStyle {
                        color: Some(run.color),
                        thickness: px(1.0),
                        wavy: false,
                    }),
                    ..run.clone()
                },
                TextRun {
                    len: display_text.len() - marked_range.end,
                    ..run
                },
            ]
            .into_iter()
            .filter(|run| run.len > 0)
            .collect(),
            None => vec![run],
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let line = window
            .text_system()
            .shape_line(display_text, font_size, &runs, None);

        // Scroll just far enough to keep the cursor visible.
        let cursor_x = line.x_for_index(cursor);
        let width = bounds.size.width;
        let mut scroll_x = input.scroll_x;

        if cursor_x - scroll_x > width - px(2.0) {
            scroll_x = cursor_x - width + px(2.0);
        } else if cursor_x < scroll_x {
            scroll_x = cursor_x;
        }

        scroll_x = scroll_x.min((line.width - width + px(2.0)).max(px(0.0)));

        let left = bounds.left() - scroll_x;
        let (selection, cursor) = if selected_range.is_empty() {
            (
                None,
                Some(fill(
                    Bounds::new(
                        point(left + cursor_x, bounds.top()),
                        size(CURSOR_WIDTH, bounds.size.height),
                    ),
                    self.cursor_color,
                )),
            )
        } else {
            (
                Some(fill(
                    Bounds::from_corners(
                        point(left + line.x_for_index(selected_range.start), bounds.top()),
                        point(left + line.x_for_index(selected_range.end), bounds.bottom()),
                    ),
                    self.selection_color,
                )),
                None,
            )
        };

        TextPrepaintState {
            line: Some(line),
            cursor,
            selection,
            scroll_x,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.input.read(cx).focus_handle.clone();

        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.input.clone()),
            cx,
        );

        let Some(line) = prepaint.line.take() else {
            return;
        };

        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }

            let _ = line.paint(
                point(bounds.left() - prepaint.scroll_x, bounds.top()),
                window.line_height(),
                window,
                cx,
            );

            if self.input.read(cx).cursor_shown(window)
                && let Some(cursor) = prepaint.cursor.take()
            {
                window.paint_quad(cursor);
            }
        });

        let scroll_x = prepaint.scroll_x;

        self.input.update(cx, |input, _| {
            input.last_layout = Some(line);
            input.last_bounds = Some(bounds);
            input.scroll_x = scroll_x;
        });
    }
}

/// A single line text input bound to an [`InputState`].
#[derive(IntoElement)]
pub(crate) struct Input {
    state: Entity<InputState>,
    style: StyleRefinement,
    prefix: Option<Icon>,
    size: Size,
    cleanable: bool,
    disabled: bool,
}

impl Input {
    /// Constructs an input for the given state.
    pub(crate) fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
            prefix: None,
            size: Size::Medium,
            cleanable: false,
            disabled: false,
        }
    }

    /// Shows an icon before the text.
    pub(crate) fn prefix(mut self, prefix: Icon) -> Self {
        self.prefix = Some(prefix);
        self
    }

    /// Shows a clear button while there is text.
    pub(crate) fn cleanable(mut self, cleanable: bool) -> Self {
        self.cleanable = cleanable;
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Sizable for Input {
    fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Input {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = *cx.tessera_theme();
        let state = self.state;
        let disabled = self.disabled;
        let focus_handle = state.read(cx).focus_handle.clone();
        let focused = !disabled && focus_handle.is_focused(window);
        let show_clear = self.cleanable && !disabled && !state.read(cx).content.is_empty();

        let (padding_x, padding_y, gap) = match self.size {
            Size::XSmall => (px(4.0), px(0.0), px(6.0)),
            Size::Small => (px(8.0), px(2.0), px(4.0)),
            Size::Medium => (px(10.0), px(8.0), px(6.0)),
        };

        let bg = palette::with_alpha(theme.border_strong, if disabled { 0.15 } else { 0.3 });

        let mut base = div()
            .id(("input", state.entity_id()))
            .flex()
            .items_center()
            .size_full()
            .px(padding_x)
            .py(padding_y)
            .gap(gap)
            .map(|this| match self.size {
                Size::XSmall => this.h_5().text_xs(),
                Size::Small => this.h_6().text_sm(),
                Size::Medium => this.h_8().text_sm(),
            })
            .line_height(relative(1.25))
            .bg(bg)
            .rounded(theme.radius)
            .border_1()
            .border_color(if focused {
                theme.primary
            } else {
                theme.border_strong
            })
            .when(focused, |this| {
                // The focus ring is a band outside the border.
                this.shadow(vec![crate::components::box_shadow(
                    px(0.0),
                    px(0.0),
                    px(0.0),
                    px(2.0),
                    palette::with_alpha(theme.primary, 0.5),
                )])
            });

        *base.style() = {
            let mut style = base.style().clone();
            gpui::Refineable::refine(&mut style, &self.style);
            style
        };

        base.when(!disabled, |this| {
            this.key_context(CONTEXT)
                .track_focus(&focus_handle)
                .cursor(CursorStyle::IBeam)
                .on_action(window.listener_for(&state, InputState::backspace))
                .on_action(window.listener_for(&state, InputState::delete))
                .on_action(window.listener_for(&state, InputState::left))
                .on_action(window.listener_for(&state, InputState::right))
                .on_action(window.listener_for(&state, InputState::select_left))
                .on_action(window.listener_for(&state, InputState::select_right))
                .on_action(window.listener_for(&state, InputState::select_all))
                .on_action(window.listener_for(&state, InputState::home))
                .on_action(window.listener_for(&state, InputState::end))
                .on_action(window.listener_for(&state, InputState::select_to_home))
                .on_action(window.listener_for(&state, InputState::select_to_end))
                .on_action(window.listener_for(&state, InputState::word_left))
                .on_action(window.listener_for(&state, InputState::word_right))
                .on_action(window.listener_for(&state, InputState::select_word_left))
                .on_action(window.listener_for(&state, InputState::select_word_right))
                .on_action(window.listener_for(&state, InputState::delete_word_left))
                .on_action(window.listener_for(&state, InputState::delete_word_right))
                .on_action(window.listener_for(&state, InputState::enter))
                .on_action(window.listener_for(&state, InputState::paste))
                .on_action(window.listener_for(&state, InputState::cut))
                .on_action(window.listener_for(&state, InputState::copy))
                .on_mouse_down(
                    MouseButton::Left,
                    window.listener_for(&state, InputState::on_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    window.listener_for(&state, InputState::on_mouse_up),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    window.listener_for(&state, InputState::on_mouse_up),
                )
                .on_mouse_move(window.listener_for(&state, InputState::on_mouse_move))
        })
        .when_some(self.prefix, |this, prefix| {
            this.child(div().when(disabled, |this| this.opacity(0.5)).child(prefix))
        })
        .child(TextElement {
            input: state.clone(),
            placeholder_color: theme.text_secondary.into(),
            selection_color: palette::with_alpha(theme.primary, 0.35).into(),
            cursor_color: theme.text.into(),
        })
        .when(show_clear, |this| {
            this.child(
                div()
                    .id("clear")
                    .flex_shrink_0()
                    .cursor_pointer()
                    .text_color(theme.text_secondary)
                    .hover(|this| this.text_color(theme.text))
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click({
                        let state = state.clone();
                        move |_, window, cx| {
                            state.update(cx, |state, cx| {
                                state.clean(window, cx);
                                window.focus(&state.focus_handle);
                            });
                        }
                    })
                    .child(Icon::new(IconName::CircleX).small()),
            )
        })
    }
}

/// How a character joins with its neighbours into a word, for word-wise movement and selection.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CharacterKind {
    Word,
    Whitespace,
    Other,
}

impl From<char> for CharacterKind {
    fn from(ch: char) -> Self {
        if ch == '_' || ch.is_alphanumeric() || is_grapheme_extend(ch) {
            CharacterKind::Word
        } else if ch.is_whitespace() {
            CharacterKind::Whitespace
        } else {
            CharacterKind::Other
        }
    }
}

/// The word (or whitespace run, or single symbol) at the offset, for double clicks.
fn word_range_at(text: &str, offset: usize) -> Option<Range<usize>> {
    let mut offset = offset.min(text.len());

    while !text.is_char_boundary(offset) {
        offset -= 1;
    }

    let ch = text[offset..].chars().next()?;
    let kind = CharacterKind::from(ch);
    let connects = |other: char| {
        matches!(
            (kind, CharacterKind::from(other)),
            (CharacterKind::Word, CharacterKind::Word)
                | (CharacterKind::Whitespace, CharacterKind::Whitespace)
        )
    };

    let start = text[..offset]
        .chars()
        .rev()
        .take_while(|ch| connects(*ch))
        .fold(offset, |offset, ch| offset - ch.len_utf8());
    let end = text[offset + ch.len_utf8()..]
        .chars()
        .take_while(|ch| connects(*ch))
        .fold(offset + ch.len_utf8(), |offset, ch| offset + ch.len_utf8());

    Some(start..end)
}

/// Whether a scalar extends the grapheme cluster before it rather than starting a new one:
/// combining marks, variation selectors, emoji modifiers and tags, and the zero width joiner.
fn is_grapheme_extend(ch: char) -> bool {
    matches!(
        ch,
        '\u{0300}'..='\u{036F}'
            | '\u{0483}'..='\u{0489}'
            | '\u{0591}'..='\u{05BD}'
            | '\u{05BF}'
            | '\u{05C1}'..='\u{05C2}'
            | '\u{05C4}'..='\u{05C5}'
            | '\u{05C7}'
            | '\u{0610}'..='\u{061A}'
            | '\u{064B}'..='\u{065F}'
            | '\u{0670}'
            | '\u{06D6}'..='\u{06DC}'
            | '\u{06DF}'..='\u{06E4}'
            | '\u{06E7}'..='\u{06E8}'
            | '\u{06EA}'..='\u{06ED}'
            | '\u{0900}'..='\u{0903}'
            | '\u{093A}'..='\u{094F}'
            | '\u{0951}'..='\u{0957}'
            | '\u{0962}'..='\u{0963}'
            | '\u{0E31}'
            | '\u{0E34}'..='\u{0E3A}'
            | '\u{0E47}'..='\u{0E4E}'
            | '\u{1AB0}'..='\u{1AFF}'
            | '\u{1DC0}'..='\u{1DFF}'
            | '\u{200C}'..='\u{200D}'
            | '\u{20D0}'..='\u{20FF}'
            | '\u{302A}'..='\u{302F}'
            | '\u{3099}'..='\u{309A}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FE20}'..='\u{FE2F}'
            | '\u{1F3FB}'..='\u{1F3FF}'
            | '\u{E0020}'..='\u{E007F}'
            | '\u{E0100}'..='\u{E01EF}'
    )
}

/// Whether a scalar is a regional indicator; pairs of them form one flag.
fn is_regional_indicator(ch: char) -> bool {
    matches!(ch, '\u{1F1E6}'..='\u{1F1FF}')
}

/// The byte offsets at which extended grapheme clusters start, approximately: a cluster is a
/// base scalar followed by any extending scalars, anything joined on by a zero width joiner, the
/// second of a pair of regional indicators, and the `\n` of a `\r\n`.
fn grapheme_starts(text: &str) -> impl Iterator<Item = usize> + '_ {
    let mut previous: Option<char> = None;
    let mut joined = false;
    // Whether the previous scalar was the first regional indicator of a pair.
    let mut open_flag = false;

    text.char_indices().filter_map(move |(index, ch)| {
        let extends = match previous {
            None => false,
            Some(_) if joined => true,
            Some(before) => {
                (before == '\r' && ch == '\n')
                    || is_grapheme_extend(ch)
                    || (open_flag && is_regional_indicator(ch))
            }
        };

        open_flag = is_regional_indicator(ch) && !(open_flag && extends);
        joined = ch == '\u{200D}';
        previous = Some(ch);

        (!extends).then_some(index)
    })
}

/// The grapheme boundary before the offset.
fn previous_grapheme(text: &str, offset: usize) -> usize {
    grapheme_starts(text)
        .take_while(|start| *start < offset)
        .last()
        .unwrap_or(0)
}

/// The grapheme boundary after the offset.
fn next_grapheme(text: &str, offset: usize) -> usize {
    grapheme_starts(text)
        .find(|start| *start > offset)
        .unwrap_or(text.len())
}
