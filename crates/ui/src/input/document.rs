//! Multi-line text surface: selectable, copyable, optionally editable.
//!
//! ponytail: a sibling of `text_input.rs`, not a generalisation of it. The
//! single-line one is a near-verbatim copy of gpui's `examples/input.rs` and is
//! worth keeping re-syncable; multi-line needs a different element (one
//! `ShapedLine` per visual line, own scroll offset), so the ~150 lines of
//! overlap are cheaper than one widget with a `multiline: bool` running through
//! every method.
//!
//! ponytail: no soft wrapping — long lines scroll horizontally instead. Wrapping
//! means a second coordinate space (buffer offset vs. display row) threaded
//! through selection painting, mouse hit-testing and up/down movement, which is
//! several times the code here. Monospaced JSON bodies also read better
//! unwrapped. Add it when someone actually asks.
use std::ops::Range;

use crate::theme;
use crate::tokens::{radius, space, text};
use gpui::{
    App, Bounds, ClipboardItem, ContentMask, Context, CursorStyle, ElementId, ElementInputHandler,
    Entity, EntityInputHandler, FocusHandle, Focusable, GlobalElementId, KeyBinding, LayoutId,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PaintQuad, Pixels, Point,
    ScrollWheelEvent, ShapedLine, SharedString, Style, TextRun, UTF16Selection, Window, actions,
    div, fill, point, prelude::*, px, relative, size,
};
use unicode_segmentation::UnicodeSegmentation;

/// What Tab inserts. ponytail: fixed, no tab-width setting until asked.
const TAB: &str = "  ";

actions!(
    editor,
    [
        Backspace,
        Copy,
        Cut,
        Delete,
        Down,
        End,
        Home,
        Left,
        Newline,
        PageDown,
        PageUp,
        Paste,
        Right,
        SelectAll,
        SelectDown,
        SelectEnd,
        SelectHome,
        SelectLeft,
        SelectPageDown,
        SelectPageUp,
        SelectRight,
        SelectUp,
        ShowCharacterPalette,
        Tab,
        Up,
    ]
);

/// Key bindings the multi-line editor needs. Call once at startup.
///
/// All bindings are scoped to the `Editor` key context so they cannot shadow
/// `text_input`'s context-free bindings.
pub fn bind_keys(cx: &mut App) {
    let ctx = Some("Editor");
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, ctx),
        KeyBinding::new("delete", Delete, ctx),
        KeyBinding::new("left", Left, ctx),
        KeyBinding::new("right", Right, ctx),
        KeyBinding::new("up", Up, ctx),
        KeyBinding::new("down", Down, ctx),
        KeyBinding::new("shift-left", SelectLeft, ctx),
        KeyBinding::new("shift-right", SelectRight, ctx),
        KeyBinding::new("shift-up", SelectUp, ctx),
        KeyBinding::new("shift-down", SelectDown, ctx),
        KeyBinding::new("home", Home, ctx),
        KeyBinding::new("end", End, ctx),
        KeyBinding::new("shift-home", SelectHome, ctx),
        KeyBinding::new("shift-end", SelectEnd, ctx),
        KeyBinding::new("pageup", PageUp, ctx),
        KeyBinding::new("pagedown", PageDown, ctx),
        KeyBinding::new("shift-pageup", SelectPageUp, ctx),
        KeyBinding::new("shift-pagedown", SelectPageDown, ctx),
        KeyBinding::new("enter", Newline, ctx),
        KeyBinding::new("tab", Tab, ctx),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, ctx),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, ctx),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, ctx),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, ctx),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, ctx),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, ctx),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, ctx),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, ctx),
        #[cfg(target_os = "macos")]
        KeyBinding::new("ctrl-cmd-space", ShowCharacterPalette, ctx),
    ]);
}

// ---------------------------------------------------------------------------
// Pure text/offset logic. Free functions so it is testable without a window.
// ---------------------------------------------------------------------------

/// Number of lines (always >= 1; a trailing newline yields a final empty line).
fn line_count(text: &str) -> usize {
    text.matches('\n').count() + 1
}

/// Byte offset -> (line, byte column within the line).
fn offset_to_point(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.matches('\n').count();
    let col = offset - before.rfind('\n').map_or(0, |i| i + 1);
    (line, col)
}

/// Byte range of a line, excluding its newline. Out-of-range lines clamp to the end.
fn line_range(text: &str, line: usize) -> Range<usize> {
    let mut start = 0;
    for _ in 0..line {
        match text[start..].find('\n') {
            Some(i) => start += i + 1,
            None => return text.len()..text.len(),
        }
    }
    let end = text[start..].find('\n').map_or(text.len(), |i| start + i);
    start..end
}

/// (line, byte column) -> byte offset, clamped to the line and to a char boundary.
fn point_to_offset(text: &str, line: usize, col: usize) -> usize {
    let range = line_range(text, line);
    let mut offset = range.end.min(range.start + col);
    while offset > range.start && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn previous_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .rev()
        .find_map(|(idx, _)| (idx < offset).then_some(idx))
        .unwrap_or(0)
}

fn next_boundary(text: &str, offset: usize) -> usize {
    text.grapheme_indices(true)
        .find_map(|(idx, _)| (idx > offset).then_some(idx))
        .unwrap_or(text.len())
}

/// Text + selection, with no dependency on gpui's window/entity machinery.
struct Buffer {
    content: SharedString,
    selected_range: Range<usize>,
    selection_reversed: bool,
    marked_range: Option<Range<usize>>,
    read_only: bool,
    /// Column that up/down tries to land on, so passing through short lines
    /// does not eat the column.
    desired_column: Option<usize>,
}

impl Buffer {
    fn cursor(&self) -> usize {
        if self.selection_reversed {
            self.selected_range.start
        } else {
            self.selected_range.end
        }
    }

    /// Move or extend to `offset` without touching the desired column.
    fn place(&mut self, offset: usize, extend: bool) {
        if !extend {
            self.selected_range = offset..offset;
            self.selection_reversed = false;
            return;
        }
        if self.selection_reversed {
            self.selected_range.start = offset;
        } else {
            self.selected_range.end = offset;
        }
        if self.selected_range.end < self.selected_range.start {
            self.selection_reversed = !self.selection_reversed;
            self.selected_range = self.selected_range.end..self.selected_range.start;
        }
    }

    /// Horizontal / absolute movement: forgets the desired column.
    fn move_to(&mut self, offset: usize, extend: bool) {
        self.desired_column = None;
        self.place(offset, extend);
    }

    fn move_vertical(&mut self, delta: isize, extend: bool) {
        let (line, col) = offset_to_point(&self.content, self.cursor());
        let col = self.desired_column.unwrap_or(col);
        let last = line_count(&self.content) as isize - 1;
        let target = (line as isize + delta).clamp(0, last) as usize;
        let offset = point_to_offset(&self.content, target, col);
        self.place(offset, extend);
        self.desired_column = Some(col);
    }

    fn selected_text(&self) -> &str {
        &self.content[self.selected_range.clone()]
    }

    /// Replace `range` with `new_text`. No-op when read-only — this is the one
    /// gate every edit path (typing, IME, paste, cut, delete) routes through.
    fn edit(&mut self, range: Range<usize>, new_text: &str) {
        if self.read_only {
            return;
        }
        self.content =
            (self.content[..range.start].to_owned() + new_text + &self.content[range.end..]).into();
        let cursor = range.start + new_text.len();
        self.selected_range = cursor..cursor;
        self.selection_reversed = false;
        self.marked_range = None;
        self.desired_column = None;
    }
}

pub struct Editor {
    focus_handle: FocusHandle,
    buf: Buffer,
    placeholder: SharedString,
    is_selecting: bool,
    scroll: Point<Pixels>,
    scroll_to_cursor: bool,
    // Filled in by the element each frame; needed for mouse hit-testing.
    last_bounds: Option<Bounds<Pixels>>,
    line_height: Pixels,
    visible_lines: Vec<ShapedLine>,
    first_visible_line: usize,
    page_lines: usize,
}

impl Editor {
    pub fn new(cx: &mut Context<Self>, placeholder: impl Into<SharedString>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            buf: Buffer {
                content: SharedString::default(),
                selected_range: 0..0,
                selection_reversed: false,
                marked_range: None,
                read_only: false,
                desired_column: None,
            },
            placeholder: placeholder.into(),
            is_selecting: false,
            scroll: Point::default(),
            scroll_to_cursor: false,
            last_bounds: None,
            line_height: px(20.),
            visible_lines: Vec::new(),
            first_visible_line: 0,
            page_lines: 1,
        }
    }

    /// Selectable and copyable, but not editable.
    pub fn read_only(mut self) -> Self {
        self.buf.read_only = true;
        self
    }

    pub fn text(&self) -> &str {
        &self.buf.content
    }

    pub fn set_text(&mut self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.buf.content = text.into();
        self.buf.selected_range = 0..0;
        self.buf.selection_reversed = false;
        self.buf.marked_range = None;
        self.buf.desired_column = None;
        self.scroll = Point::default();
        cx.notify();
    }

    fn moved(&mut self, cx: &mut Context<Self>) {
        self.scroll_to_cursor = true;
        cx.notify();
    }

    fn line_start(&self) -> usize {
        let (line, _) = offset_to_point(&self.buf.content, self.buf.cursor());
        line_range(&self.buf.content, line).start
    }

    fn line_end(&self) -> usize {
        let (line, _) = offset_to_point(&self.buf.content, self.buf.cursor());
        line_range(&self.buf.content, line).end
    }

    fn horizontal(&mut self, forward: bool, extend: bool, cx: &mut Context<Self>) {
        let target = if !extend && !self.buf.selected_range.is_empty() {
            if forward {
                self.buf.selected_range.end
            } else {
                self.buf.selected_range.start
            }
        } else if forward {
            next_boundary(&self.buf.content, self.buf.cursor())
        } else {
            previous_boundary(&self.buf.content, self.buf.cursor())
        };
        self.buf.move_to(target, extend);
        self.moved(cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(false, false, cx);
    }
    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(true, false, cx);
    }
    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(false, true, cx);
    }
    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.horizontal(true, true, cx);
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(-1, false);
        self.moved(cx);
    }
    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(1, false);
        self.moved(cx);
    }
    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(-1, true);
        self.moved(cx);
    }
    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(1, true);
        self.moved(cx);
    }

    fn page_up(&mut self, _: &PageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(-(self.page_lines as isize), false);
        self.moved(cx);
    }
    fn page_down(&mut self, _: &PageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(self.page_lines as isize, false);
        self.moved(cx);
    }
    fn select_page_up(&mut self, _: &SelectPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(-(self.page_lines as isize), true);
        self.moved(cx);
    }
    fn select_page_down(&mut self, _: &SelectPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_vertical(self.page_lines as isize, true);
        self.moved(cx);
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.line_start();
        self.buf.move_to(target, false);
        self.moved(cx);
    }
    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.line_end();
        self.buf.move_to(target, false);
        self.moved(cx);
    }
    fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.line_start();
        self.buf.move_to(target, true);
        self.moved(cx);
    }
    fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        let target = self.line_end();
        self.buf.move_to(target, true);
        self.moved(cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.move_to(0, false);
        self.buf.move_to(self.buf.content.len(), true);
        cx.notify();
    }

    fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        if self.buf.selected_range.is_empty() {
            let to = previous_boundary(&self.buf.content, self.buf.cursor());
            self.buf.move_to(to, true);
        }
        self.buf.edit(self.buf.selected_range.clone(), "");
        self.moved(cx);
    }

    fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        if self.buf.selected_range.is_empty() {
            let to = next_boundary(&self.buf.content, self.buf.cursor());
            self.buf.move_to(to, true);
        }
        self.buf.edit(self.buf.selected_range.clone(), "");
        self.moved(cx);
    }

    fn newline(&mut self, _: &Newline, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.edit(self.buf.selected_range.clone(), "\n");
        self.moved(cx);
    }

    fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        self.buf.edit(self.buf.selected_range.clone(), TAB);
        self.moved(cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.buf.selected_range.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.buf.selected_text().to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        if self.buf.selected_range.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(
            self.buf.selected_text().to_string(),
        ));
        self.buf.edit(self.buf.selected_range.clone(), "");
        self.moved(cx);
    }

    fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.buf.edit(self.buf.selected_range.clone(), &text);
            self.moved(cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.is_selecting = true;
        let offset = self.offset_for_position(event.position);
        self.buf.move_to(offset, event.modifiers.shift);
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let offset = self.offset_for_position(event.position);
            self.buf.move_to(offset, true);
            cx.notify();
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(self.line_height.max(px(1.)));
        self.scroll.x = (self.scroll.x - delta.x).max(px(0.));
        self.scroll.y = (self.scroll.y - delta.y).max(px(0.));
        // The element clamps against content size on the next prepaint.
        let _ = window;
        cx.notify();
    }

    /// Window coordinates -> byte offset. Drag past the top/bottom edge keeps
    /// selecting on the clamped line.
    fn offset_for_position(&self, position: Point<Pixels>) -> usize {
        let Some(bounds) = self.last_bounds else {
            return 0;
        };
        if self.buf.content.is_empty() {
            return 0;
        }
        let last_line = line_count(&self.buf.content) - 1;
        let y = (position.y - bounds.top() + self.scroll.y).max(px(0.));
        let line = ((y / self.line_height).floor() as usize).min(last_line);
        let range = line_range(&self.buf.content, line);
        match self
            .visible_lines
            .get(line.wrapping_sub(self.first_visible_line))
        {
            Some(shaped) => {
                let x = position.x - bounds.left() + self.scroll.x;
                range.start + shaped.closest_index_for_x(x)
            }
            None => range.start,
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;
        for ch in self.buf.content.chars() {
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
        for ch in self.buf.content.chars() {
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

    fn range_from_utf16(&self, range: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(range.start)..self.offset_from_utf16(range.end)
    }
}

impl EntityInputHandler for Editor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range_utf16);
        actual_range.replace(self.range_to_utf16(&range));
        Some(self.buf.content[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.buf.selected_range),
            reversed: self.buf.selection_reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.buf
            .marked_range
            .as_ref()
            .map(|range| self.range_to_utf16(range))
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.buf.marked_range = None;
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
            .map(|range| self.range_from_utf16(range))
            .or(self.buf.marked_range.clone())
            .unwrap_or(self.buf.selected_range.clone());
        self.buf.edit(range, new_text);
        self.moved(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.buf.read_only {
            return;
        }
        let range = range_utf16
            .as_ref()
            .map(|range| self.range_from_utf16(range))
            .or(self.buf.marked_range.clone())
            .unwrap_or(self.buf.selected_range.clone());

        self.buf.edit(range.clone(), new_text);
        self.buf.marked_range =
            (!new_text.is_empty()).then(|| range.start..range.start + new_text.len());
        if let Some(new_range) = new_selected_range_utf16 {
            let new_range = self.range_from_utf16(&new_range);
            self.buf.selected_range = (new_range.start + range.start).min(self.buf.content.len())
                ..(new_range.end + range.start).min(self.buf.content.len());
        }
        self.moved(cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range_utf16);
        let (line, col) = offset_to_point(&self.buf.content, range.start);
        let shaped = self
            .visible_lines
            .get(line.wrapping_sub(self.first_visible_line))?;
        let top = bounds.top() + self.line_height * line as f32 - self.scroll.y;
        let left = bounds.left() + shaped.x_for_index(col) - self.scroll.x;
        let (_, end_col) = offset_to_point(&self.buf.content, range.end);
        let right = bounds.left() + shaped.x_for_index(end_col) - self.scroll.x;
        Some(Bounds::from_corners(
            point(left, top),
            point(right.max(left), top + self.line_height),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.offset_to_utf16(self.offset_for_position(point)))
    }
}

// ---------------------------------------------------------------------------
// Element
// ---------------------------------------------------------------------------

struct EditorElement {
    editor: Entity<Editor>,
}

struct PrepaintState {
    /// Shaped visible lines, paired with the absolute line index of the first.
    lines: Vec<ShapedLine>,
    first_line: usize,
    line_height: Pixels,
    scroll: Point<Pixels>,
    selections: Vec<PaintQuad>,
    cursor: Option<PaintQuad>,
    page_lines: usize,
}

impl IntoElement for EditorElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for EditorElement {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let line_height = window.line_height();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let font = style.font();

        let t = theme::theme(cx);
        let editor = self.editor.read(cx);
        let read_only = editor.buf.read_only;
        let selected = editor.buf.selected_range.clone();
        let cursor_offset = editor.buf.cursor();
        let mut scroll = editor.scroll;
        let scroll_to_cursor = editor.scroll_to_cursor;
        let is_placeholder = editor.buf.content.is_empty();
        let text: SharedString = if is_placeholder {
            editor.placeholder.clone()
        } else {
            editor.buf.content.clone()
        };
        let color = if is_placeholder {
            t.text_muted().into()
        } else {
            style.color
        };

        // ponytail: line ranges are recomputed every frame (O(text)). Cache them
        // on the entity if response bodies ever get big enough to show up.
        let mut line_ranges: Vec<Range<usize>> = Vec::new();
        let mut start = 0;
        for line in text.split('\n') {
            line_ranges.push(start..start + line.len());
            start += line.len() + 1;
        }

        let shape = |window: &mut Window, text: &SharedString| {
            let runs = if text.is_empty() {
                Vec::new()
            } else {
                vec![TextRun {
                    len: text.len(),
                    font: font.clone(),
                    color,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }]
            };
            window
                .text_system()
                .shape_line(text.clone(), font_size, &runs, None)
        };

        let content_height = line_height * line_ranges.len() as f32;
        let max_scroll_y = (content_height - bounds.size.height).max(px(0.));

        // Keep the cursor on screen after a move.
        let (cursor_line, cursor_col) = offset_to_point(&text, cursor_offset);
        if scroll_to_cursor && !is_placeholder {
            let top = line_height * cursor_line as f32;
            if top < scroll.y {
                scroll.y = top;
            } else if top + line_height > scroll.y + bounds.size.height {
                scroll.y = top + line_height - bounds.size.height;
            }
            let range = line_ranges[cursor_line].clone();
            let line_text: SharedString = text[range.clone()].to_string().into();
            let x = shape(window, &line_text).x_for_index(cursor_col);
            if x < scroll.x {
                scroll.x = (x - px(24.)).max(px(0.));
            } else if x > scroll.x + bounds.size.width - px(8.) {
                scroll.x = x - bounds.size.width + px(8.);
            }
        }
        scroll.y = scroll.y.clamp(px(0.), max_scroll_y);

        let visible_count = (bounds.size.height / line_height).ceil() as usize + 1;
        let first_line = ((scroll.y / line_height).floor() as usize).min(line_ranges.len() - 1);
        let last_line = (first_line + visible_count).min(line_ranges.len());

        let mut lines = Vec::with_capacity(last_line - first_line);
        let mut widest = px(0.);
        for range in &line_ranges[first_line..last_line] {
            let line_text: SharedString = text[range.clone()].to_string().into();
            let shaped = shape(window, &line_text);
            widest = widest.max(shaped.width);
            lines.push(shaped);
        }
        scroll.x = scroll
            .x
            .clamp(px(0.), (widest + px(8.) - bounds.size.width).max(px(0.)));

        let selection_color = t.selection();
        let mut selections = Vec::new();
        let mut cursor = None;
        for (i, shaped) in lines.iter().enumerate() {
            let index = first_line + i;
            let range = line_ranges[index].clone();
            let top = bounds.top() + line_height * index as f32 - scroll.y;

            if !selected.is_empty()
                && !is_placeholder
                && selected.start <= range.end
                && selected.end > range.start
            {
                let sx = if selected.start <= range.start {
                    px(0.)
                } else {
                    shaped.x_for_index(selected.start - range.start)
                };
                // A selection running past the line end covers the newline; show
                // a stub so blank lines are visibly selected too.
                let ex = if selected.end > range.end {
                    shaped.width + px(4.)
                } else {
                    shaped.x_for_index(selected.end - range.start)
                };
                if ex > sx {
                    selections.push(fill(
                        Bounds::from_corners(
                            point(bounds.left() + sx - scroll.x, top),
                            point(bounds.left() + ex - scroll.x, top + line_height),
                        ),
                        selection_color,
                    ));
                }
            }

            // No caret in a read-only view: it is selectable and focusable, but
            // a blinking insertion point there would promise editing.
            if selected.is_empty() && index == cursor_line && !is_placeholder && !read_only {
                cursor = Some(fill(
                    Bounds::new(
                        point(
                            bounds.left() + shaped.x_for_index(cursor_col) - scroll.x,
                            top,
                        ),
                        size(px(2.), line_height),
                    ),
                    t.accent,
                ));
            }
        }
        if is_placeholder && !read_only {
            cursor = Some(fill(
                Bounds::new(
                    point(bounds.left(), bounds.top()),
                    size(px(2.), line_height),
                ),
                t.accent,
            ));
        }

        PrepaintState {
            lines,
            first_line,
            line_height,
            scroll,
            selections,
            cursor,
            page_lines: ((bounds.size.height / line_height).floor() as usize).max(1),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        let line_height = prepaint.line_height;
        let scroll = prepaint.scroll;
        let first_line = prepaint.first_line;
        let lines = std::mem::take(&mut prepaint.lines);
        let focused = focus_handle.is_focused(window);

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            for quad in prepaint.selections.drain(..) {
                window.paint_quad(quad);
            }
            for (i, line) in lines.iter().enumerate() {
                let origin = point(
                    bounds.left() - scroll.x,
                    bounds.top() + line_height * (first_line + i) as f32 - scroll.y,
                );
                line.paint(origin, line_height, gpui::TextAlign::Left, None, window, cx).ok();
            }
            if focused && let Some(cursor) = prepaint.cursor.take() {
                window.paint_quad(cursor);
            }
        });

        self.editor.update(cx, |editor, _| {
            editor.visible_lines = lines;
            editor.first_visible_line = first_line;
            editor.line_height = line_height;
            editor.last_bounds = Some(bounds);
            editor.scroll = scroll;
            editor.scroll_to_cursor = false;
            editor.page_lines = prepaint.page_lines;
        });
    }
}

impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = theme::theme(cx);
        let focused = self.focus_handle.is_focused(window);
        // The border is always drawn, so gaining focus never shifts the text by a
        // pixel; what changes is its colour.
        //
        // At rest a read-only editor has none. It is content — tool output, a
        // reference table, a response body — and a bordered rounded box with an
        // I-beam reads as a field you are failing to type into. It still takes
        // focus, and then shows the ring, because selecting and copying with the
        // keyboard has to remain possible.
        let mut frame = div()
            .border_1()
            .border_color(match (focused, self.buf.read_only) {
                (true, _) => t.focus(),
                (false, true) => gpui::transparent_black().into(),
                (false, false) => t.divider,
            });
        if !focused && !self.buf.read_only {
            frame = frame.hover(|s| s.border_color(t.border_hover()));
        }
        frame
            .key_context("Editor")
            .track_focus(&self.focus_handle(cx))
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::newline))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::show_character_palette))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .size_full()
            // `textarea.input { min-height: 90px }`. No token for it — see report.
            .min_h(px(90.))
            .px(space::S3)
            .py(space::S2)
            // A read-only body sits on the window ground rather than the raised
            // input surface, so "you cannot type here" is visible before you try.
            .bg(if self.buf.read_only { t.bg } else { t.surface })
            // Not a pill: this is a code area several hundred pixels tall, where
            // the system's 999-px override would swallow the first and last
            // lines. `radius::MD` is the softest corner that still clears them.
            .rounded(radius::MD)
            // Request bodies, response bodies and tool output: the design loads
            // a mono face for exactly these. Resolved rather than named, since
            // gpui substitutes a missing family silently.
            .font_family(crate::fonts::fonts(cx).mono)
            .text_size(text::CONTROL)
            .line_height(text::BODY_LINE)
            .text_color(t.text)
            .overflow_hidden()
            .child(EditorElement {
                editor: cx.entity(),
            })
    }
}

impl Focusable for Editor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(text: &str) -> Buffer {
        Buffer {
            content: text.to_string().into(),
            selected_range: 0..0,
            selection_reversed: false,
            marked_range: None,
            read_only: false,
            desired_column: None,
        }
    }

    const TEXT: &str = "hello\nab\nlonger line here";

    #[test]
    fn offsets_map_to_points_and_back() {
        assert_eq!(offset_to_point(TEXT, 0), (0, 0));
        assert_eq!(offset_to_point(TEXT, 5), (0, 5)); // end of "hello"
        assert_eq!(offset_to_point(TEXT, 6), (1, 0)); // start of "ab"
        assert_eq!(offset_to_point(TEXT, 8), (1, 2));
        assert_eq!(offset_to_point(TEXT, 9), (2, 0));
        assert_eq!(offset_to_point(TEXT, TEXT.len()), (2, 16));
        // past the end clamps
        assert_eq!(offset_to_point(TEXT, TEXT.len() + 10), (2, 16));

        for offset in 0..=TEXT.len() {
            let (line, col) = offset_to_point(TEXT, offset);
            assert_eq!(point_to_offset(TEXT, line, col), offset, "offset {offset}");
        }

        assert_eq!(line_count(TEXT), 3);
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a\n"), 2); // trailing newline = empty last line
        assert_eq!(line_range(TEXT, 1), 6..8);
        assert_eq!(line_range(TEXT, 9), TEXT.len()..TEXT.len());
    }

    #[test]
    fn point_to_offset_clamps_to_line_and_char_boundary() {
        assert_eq!(point_to_offset(TEXT, 1, 99), 8); // clamped to end of "ab"
        let multi = "aé\nx"; // é is 2 bytes
        assert_eq!(point_to_offset(multi, 0, 2), 1); // mid-char snaps back
        assert_eq!(point_to_offset(multi, 0, 3), 3);
    }

    #[test]
    fn vertical_movement_preserves_desired_column() {
        let mut buf = buffer(TEXT);
        // cursor at column 5 on line 0 ("hello|")
        buf.move_to(5, false);
        buf.move_vertical(1, false); // line 1 is only 2 wide
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (1, 2));
        buf.move_vertical(1, false); // long line: column 5 comes back
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (2, 5));
        buf.move_vertical(-1, false);
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (1, 2));
        buf.move_vertical(-1, false);
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (0, 5));

        // a horizontal move resets the desired column
        buf.move_to(0, false);
        buf.move_vertical(1, false);
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (1, 0));

        // clamped at the ends, no panic
        buf.move_vertical(-100, false);
        assert_eq!(buf.cursor(), 0);
        buf.move_vertical(100, false);
        assert_eq!(offset_to_point(&buf.content, buf.cursor()), (2, 0));
    }

    #[test]
    fn selection_spans_line_boundaries() {
        let mut buf = buffer(TEXT);
        buf.move_to(2, false); // inside "hello"
        buf.move_vertical(2, true); // extend down two lines
        assert_eq!(buf.selected_range, 2..11);
        assert_eq!(buf.selected_text(), "llo\nab\nlo");
        assert!(!buf.selection_reversed);

        // shrinking back onto the anchor empties the selection
        buf.move_vertical(-2, true);
        assert_eq!(buf.selected_range, 2..2);

        // extending backwards past the anchor flips direction
        buf.move_vertical(2, true);
        assert_eq!(buf.selected_range, 2..11);
        buf.move_to(0, true);
        assert!(buf.selection_reversed);
        assert_eq!(buf.selected_range, 0..2);
        assert_eq!(buf.cursor(), 0);
        // ...and extending forward again from the reversed end
        buf.move_to(7, true);
        assert!(!buf.selection_reversed);
        assert_eq!(buf.selected_range, 2..7);
        assert_eq!(buf.selected_text(), "llo\na");

        // select-all equivalent
        buf.move_to(0, false);
        buf.move_to(buf.content.len(), true);
        assert_eq!(buf.selected_text(), TEXT);
    }

    #[test]
    fn read_only_ignores_edits() {
        let mut buf = buffer(TEXT);
        buf.read_only = true;
        buf.move_to(0, false);
        buf.move_to(5, true);
        buf.edit(buf.selected_range.clone(), ""); // delete selection
        buf.edit(0..0, "typed");
        buf.edit(0..0, "\n");
        assert_eq!(buf.content.as_ref(), TEXT);
        assert_eq!(buf.selected_range, 0..5, "selection survives");

        let mut writable = buffer(TEXT);
        writable.move_to(0, false);
        writable.move_to(5, true);
        writable.edit(writable.selected_range.clone(), "HI");
        assert_eq!(writable.content.as_ref(), "HI\nab\nlonger line here");
        assert_eq!(writable.cursor(), 2);
    }
}
