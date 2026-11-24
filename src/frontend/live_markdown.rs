//! Live markdown rendering: one surface that paints styled source.
//!
//! The page body string stays the source of truth. This module builds a
//! layout of blocks and runs with byte offsets so a later pass can map
//! caret, clicks, and edits onto that source. Preview mode stays on
//! `egui_commonmark`; this path is for interactive rendered editing.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{EventFilter, FontFamily, FontId, Galley, Sense, TextStyle};

use crate::backend::Project;
use crate::frontend::UiAction;

/// Per-session caret and focus for the live markdown surface.
#[derive(Debug, Clone)]
pub struct LiveMarkdownState {
    /// Byte offset into the page body.
    pub cursor: usize,
    /// True after the surface was clicked this session.
    pub focused: bool,
    /// Preferred X for up/down moves, in galley-local coordinates.
    preferred_x: Option<f32>,
}

impl Default for LiveMarkdownState {
    fn default() -> Self {
        Self {
            cursor: 0,
            focused: false,
            preferred_x: None,
        }
    }
}

/// Inclusive-exclusive byte range into the source body.
pub type SourceRange = Range<usize>;

/// Inline style of a contiguous source slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunKind {
    Plain,
    Emphasis,
    Strong,
    Code,
    WikiLink,
}

/// One styled span inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextRun {
    pub range: SourceRange,
    pub kind: RunKind,
}

/// Block-level markdown shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    Empty,
}

/// One block with its inline runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub range: SourceRange,
    pub kind: BlockKind,
    pub runs: Vec<TextRun>,
}

/// Layout rebuilt from the current body each frame (or when dirty).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentLayout {
    pub blocks: Vec<Block>,
}

/// Draw the live markdown surface and apply basic source edits.
pub(crate) fn show(
    ui: &mut egui::Ui,
    _project: &Project,
    _parent: &Path,
    body: &mut String,
    state: &mut LiveMarkdownState,
) -> UiAction {
    // Apply keystrokes before rebuilding layout so this frame paints the
    // same source the caret/ranges describe (avoids a one-frame vertical jerk).
    if state.focused {
        apply_text_input(ui, body, state);
    }
    state.cursor = state.cursor.min(body.len());

    let wrap_width = (ui.available_width() - 8.0).max(0.0);
    let spacing_y = ui.spacing().item_spacing.y;
    let layout = build_layout(body);
    let mut galleys = Vec::with_capacity(layout.blocks.len());
    let mut content_height = spacing_y;
    for block in &layout.blocks {
        let galley =
            ui.fonts_mut(|fonts| fonts.layout_job(layout_job(body, block, ui, wrap_width)));
        content_height += galley.size().y + spacing_y;
        galleys.push(galley);
    }

    let desired = egui::vec2(ui.available_width(), content_height.max(120.0));
    let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
    if response.clicked() {
        state.focused = true;
        state.preferred_x = None;
        response.request_focus();
        if let Some(pos) = response.interact_pointer_pos() {
            state.cursor = cursor_at_pointer(body, &layout, &galleys, rect, pos, spacing_y);
        }
    } else if response.lost_focus() {
        state.focused = false;
        state.preferred_x = None;
    }

    if state.focused {
        apply_vertical_navigation(ui, body, state, &layout, &galleys, spacing_y);
        // Keep arrow keys on the caret instead of moving egui focus elsewhere.
        ui.memory_mut(|memory| {
            memory.set_focus_lock_filter(
                response.id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    tab: false,
                    escape: false,
                },
            );
        });
    }

    let painter = ui.painter_at(rect);
    let mut cursor_y = rect.top() + spacing_y;
    // A cursor on a line boundary belongs to exactly one block (the later one),
    // otherwise it would be drawn at the end of one line and the start of the next.
    let caret_block = state
        .focused
        .then(|| block_index_for_cursor(&layout, state.cursor))
        .flatten();
    for (index, (block, galley)) in layout.blocks.iter().zip(&galleys).enumerate() {
        let pos = egui::pos2(rect.left() + 4.0, cursor_y);
        painter.galley(pos, galley.clone(), ui.visuals().text_color());

        if caret_block == Some(index) {
            let local = state.cursor.saturating_sub(block.range.start);
            let caret = galley.pos_from_cursor(egui::text::CCursor {
                index: byte_to_char_index(&body[block.range.clone()], local),
                prefer_next_row: false,
            });
            let x = pos.x + caret.min.x;
            let y = pos.y + caret.min.y;
            painter.vline(
                x,
                y..=(y + caret.height()),
                egui::Stroke::new(1.0_f32, ui.visuals().text_color()),
            );
        }

        cursor_y += galley.size().y + spacing_y;
    }

    UiAction::None
}

/// Split `source` into blocks and inline runs with byte offsets.
pub(crate) fn build_layout(source: &str) -> DocumentLayout {
    let mut blocks = Vec::new();
    let mut offset = 0usize;
    for line in source.split_inclusive('\n') {
        let len = line.len();
        let range = offset..offset + len;
        let trimmed_end = line.trim_end_matches(['\r', '\n']);
        let block = if trimmed_end.is_empty() {
            Block {
                range: range.clone(),
                kind: BlockKind::Empty,
                runs: Vec::new(),
            }
        } else if let Some(heading) = parse_heading(trimmed_end) {
            let marker_len = heading.level as usize + 1;
            let text_start = range.start + marker_len.min(trimmed_end.len());
            let text_end = range.start + trimmed_end.len();
            let mut runs = Vec::new();
            if text_start > range.start {
                runs.push(TextRun {
                    range: range.start..text_start,
                    kind: RunKind::Plain,
                });
            }
            runs.extend(inline_runs(source, text_start..text_end));
            Block {
                range: range.clone(),
                kind: BlockKind::Heading(heading.level),
                runs,
            }
        } else {
            let text_end = range.start + trimmed_end.len();
            Block {
                range: range.clone(),
                kind: BlockKind::Paragraph,
                runs: inline_runs(source, range.start..text_end),
            }
        };
        blocks.push(block);
        offset += len;
    }
    // An empty document, or one ending in a newline, has a final empty line
    // where the caret can sit.
    if source.is_empty() || source.ends_with('\n') {
        blocks.push(Block {
            range: source.len()..source.len(),
            kind: BlockKind::Empty,
            runs: Vec::new(),
        });
    }
    DocumentLayout { blocks }
}

struct HeadingLine {
    level: u8,
}

fn parse_heading(line: &str) -> Option<HeadingLine> {
    let bytes = line.as_bytes();
    let mut level = 0u8;
    while (level as usize) < bytes.len() && bytes[level as usize] == b'#' && level < 6 {
        level += 1;
    }
    if level == 0 || bytes.get(level as usize) != Some(&b' ') {
        return None;
    }
    Some(HeadingLine { level })
}

fn inline_runs(source: &str, range: SourceRange) -> Vec<TextRun> {
    let slice = &source[range.clone()];
    let base = range.start;
    let mut runs = Vec::new();
    let mut i = 0usize;
    while i < slice.len() {
        if let Some(run) = match_wiki(slice, i).or_else(|| match_wrapped(slice, i, '`', RunKind::Code))
            .or_else(|| match_wrapped(slice, i, '*', RunKind::Emphasis))
        {
            if run.0 > i {
                runs.push(TextRun {
                    range: base + i..base + run.0,
                    kind: RunKind::Plain,
                });
            }
            runs.push(TextRun {
                range: base + run.0..base + run.1,
                kind: run.2,
            });
            i = run.1;
            continue;
        }
        let next = next_special(slice, i).unwrap_or(slice.len());
        if next > i {
            runs.push(TextRun {
                range: base + i..base + next,
                kind: RunKind::Plain,
            });
        }
        i = next.max(i + 1);
    }
    if runs.is_empty() && !range.is_empty() {
        runs.push(TextRun {
            range,
            kind: RunKind::Plain,
        });
    }
    runs
}

fn match_wiki(slice: &str, start: usize) -> Option<(usize, usize, RunKind)> {
    if !slice[start..].starts_with("[[") {
        return None;
    }
    let rest = &slice[start + 2..];
    let end = rest.find("]]")?;
    Some((start, start + 2 + end + 2, RunKind::WikiLink))
}

fn match_wrapped(slice: &str, start: usize, marker: char, kind: RunKind) -> Option<(usize, usize, RunKind)> {
    let bytes = slice.as_bytes();
    if bytes.get(start).copied() != Some(marker as u8) {
        return None;
    }
    // Prefer **strong** over single *
    if marker == '*' && bytes.get(start + 1).copied() == Some(b'*') {
        let rest = &slice[start + 2..];
        let end = rest.find("**")?;
        return Some((start, start + 2 + end + 2, RunKind::Strong));
    }
    let rest = &slice[start + 1..];
    let end = rest.find(marker)?;
    if end == 0 {
        return None;
    }
    Some((start, start + 1 + end + 1, kind))
}

fn next_special(slice: &str, start: usize) -> Option<usize> {
    slice[start..]
        .char_indices()
        .find(|(_, ch)| matches!(ch, '[' | '`' | '*'))
        .map(|(idx, _)| start + idx)
}

fn layout_job(source: &str, block: &Block, ui: &egui::Ui, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    if matches!(block.kind, BlockKind::Empty) {
        job.append(
            " ",
            0.0,
            TextFormat {
                font_id: font_id(ui, block.kind),
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
        return job;
    }
    if block.runs.is_empty() {
        job.append(
            &source[block.range.clone()],
            0.0,
            TextFormat {
                font_id: font_id(ui, block.kind),
                color: ui.visuals().text_color(),
                ..Default::default()
            },
        );
        return job;
    }
    for run in &block.runs {
        let text = &source[run.range.clone()];
        job.append(text, 0.0, format_for(ui, block.kind, run.kind));
    }
    job
}

fn font_id(ui: &egui::Ui, kind: BlockKind) -> FontId {
    match kind {
        BlockKind::Heading(1) => FontId::new(28.0, FontFamily::Proportional),
        BlockKind::Heading(2) => FontId::new(24.0, FontFamily::Proportional),
        BlockKind::Heading(_) => FontId::new(20.0, FontFamily::Proportional),
        BlockKind::Paragraph | BlockKind::Empty => TextStyle::Body.resolve(ui.style()),
    }
}

fn format_for(ui: &egui::Ui, block: BlockKind, run: RunKind) -> TextFormat {
    let mut format = TextFormat {
        font_id: font_id(ui, block),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    match run {
        RunKind::Plain => {}
        RunKind::Emphasis => format.italics = true,
        RunKind::Strong => format.color = ui.visuals().strong_text_color(),
        RunKind::Code => {
            format.font_id = TextStyle::Monospace.resolve(ui.style());
            format.background = ui.visuals().code_bg_color;
        }
        RunKind::WikiLink => format.color = ui.visuals().hyperlink_color,
    }
    format
}

fn cursor_at_pointer(
    source: &str,
    layout: &DocumentLayout,
    galleys: &[Arc<Galley>],
    rect: egui::Rect,
    pos: egui::Pos2,
    spacing_y: f32,
) -> usize {
    let mut cursor_y = rect.top() + spacing_y;
    for (index, (block, galley)) in layout.blocks.iter().zip(galleys).enumerate() {
        let height = galley.size().y + spacing_y;
        let is_last = index + 1 == layout.blocks.len();
        if pos.y <= cursor_y + height || is_last {
            let local = egui::vec2(pos.x - rect.left() - 4.0, pos.y - cursor_y);
            let cursor = galley.cursor_from_pos(local);
            let byte = char_to_byte_index(&source[block.range.clone()], cursor.index);
            return (block.range.start + byte).min(content_end(source, block));
        }
        cursor_y += height;
    }
    source.len()
}

fn apply_text_input(ui: &egui::Ui, body: &mut String, state: &mut LiveMarkdownState) {
    ui.input(|input| {
        for event in &input.events {
            match event {
                egui::Event::Text(text) => {
                    state.preferred_x = None;
                    body.insert_str(state.cursor, text);
                    state.cursor += text.len();
                }
                egui::Event::Key {
                    key: egui::Key::Backspace,
                    pressed: true,
                    ..
                } => {
                    state.preferred_x = None;
                    if state.cursor > 0 {
                        let start = prev_char_boundary(body, state.cursor);
                        body.replace_range(start..state.cursor, "");
                        state.cursor = start;
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Delete,
                    pressed: true,
                    ..
                } => {
                    state.preferred_x = None;
                    if state.cursor < body.len() {
                        let end = next_char_boundary(body, state.cursor);
                        body.replace_range(state.cursor..end, "");
                    }
                }
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    ..
                } => {
                    state.preferred_x = None;
                    body.insert(state.cursor, '\n');
                    state.cursor += 1;
                }
                egui::Event::Key {
                    key: egui::Key::ArrowLeft,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.any() => {
                    state.preferred_x = None;
                    state.cursor = prev_char_boundary(body, state.cursor);
                }
                egui::Event::Key {
                    key: egui::Key::ArrowRight,
                    pressed: true,
                    modifiers,
                    ..
                } if !modifiers.any() => {
                    state.preferred_x = None;
                    state.cursor = next_char_boundary(body, state.cursor);
                }
                _ => {}
            }
        }
    });
}

fn apply_vertical_navigation(
    ui: &egui::Ui,
    body: &str,
    state: &mut LiveMarkdownState,
    layout: &DocumentLayout,
    galleys: &[Arc<Galley>],
    spacing_y: f32,
) {
    let (up, down) = ui.input(|input| {
        let mut up = 0usize;
        let mut down = 0usize;
        for event in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
                && !modifiers.any()
            {
                match key {
                    egui::Key::ArrowUp => up += 1,
                    egui::Key::ArrowDown => down += 1,
                    _ => {}
                }
            }
        }
        (up, down)
    });
    for _ in 0..up {
        move_cursor_vertically(body, state, layout, galleys, spacing_y, -1);
    }
    for _ in 0..down {
        move_cursor_vertically(body, state, layout, galleys, spacing_y, 1);
    }
}

fn move_cursor_vertically(
    body: &str,
    state: &mut LiveMarkdownState,
    layout: &DocumentLayout,
    galleys: &[Arc<Galley>],
    spacing_y: f32,
    direction: i32,
) {
    let Some(block_index) = block_index_for_cursor(layout, state.cursor) else {
        return;
    };
    let block = &layout.blocks[block_index];
    let galley = &galleys[block_index];
    let local_byte = state.cursor.saturating_sub(block.range.start);
    let char_index =
        byte_to_char_index(&body[block.range.clone()], local_byte).min(galley.end().index);
    let ccursor = egui::text::CCursor {
        index: char_index,
        prefer_next_row: false,
    };
    let caret = galley.pos_from_cursor(ccursor);
    let h_pos = state.preferred_x.unwrap_or_else(|| caret.center().x);
    state.preferred_x = Some(h_pos);

    let row_height = caret.height().max(1.0);
    let mut block_top = spacing_y;
    for galley in galleys.iter().take(block_index) {
        block_top += galley.size().y + spacing_y;
    }
    let target = egui::pos2(
        h_pos,
        block_top + caret.center().y + direction as f32 * row_height,
    );
    state.cursor = cursor_at_content_pos(body, layout, galleys, target, spacing_y);
}

fn block_index_for_cursor(layout: &DocumentLayout, cursor: usize) -> Option<usize> {
    for (index, block) in layout.blocks.iter().enumerate() {
        let last = index + 1 == layout.blocks.len();
        if cursor >= block.range.start
            && (cursor < block.range.end || (last && cursor <= block.range.end))
        {
            return Some(index);
        }
    }
    layout.blocks.len().checked_sub(1)
}

fn cursor_at_content_pos(
    source: &str,
    layout: &DocumentLayout,
    galleys: &[Arc<Galley>],
    pos: egui::Pos2,
    spacing_y: f32,
) -> usize {
    let mut cursor_y = spacing_y;
    for (index, (block, galley)) in layout.blocks.iter().zip(galleys).enumerate() {
        let height = galley.size().y + spacing_y;
        let is_last = index + 1 == layout.blocks.len();
        if pos.y <= cursor_y + height || is_last {
            let local = egui::vec2(pos.x, (pos.y - cursor_y).max(0.0));
            let cursor = galley.cursor_from_pos(local);
            let byte = char_to_byte_index(&source[block.range.clone()], cursor.index);
            return (block.range.start + byte).min(content_end(source, block));
        }
        cursor_y += height;
    }
    source.len()
}

/// End of a block's text, before its trailing line break.
///
/// Clicks and vertical moves clamp here so they never land on the next line's start.
fn content_end(source: &str, block: &Block) -> usize {
    block.range.start + source[block.range.clone()].trim_end_matches(['\r', '\n']).len()
}

fn prev_char_boundary(text: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let mut i = index - 1;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut i = index + 1;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn byte_to_char_index(slice: &str, byte: usize) -> usize {
    let byte = byte.min(slice.len());
    slice[..byte].chars().count()
}

fn char_to_byte_index(slice: &str, chars: usize) -> usize {
    slice
        .char_indices()
        .nth(chars)
        .map(|(idx, _)| idx)
        .unwrap_or(slice.len())
}

#[cfg(test)]
mod tests {
    use super::{block_index_for_cursor, build_layout, BlockKind, RunKind};

    #[test]
    fn line_boundary_cursor_belongs_to_one_block() {
        let source = "ab\ncd\n";
        let layout = build_layout(source);
        // "ab\n", "cd\n", and a trailing empty line.
        assert_eq!(layout.blocks.len(), 3);
        assert_eq!(block_index_for_cursor(&layout, 2), Some(0));
        assert_eq!(block_index_for_cursor(&layout, 3), Some(1));
        assert_eq!(block_index_for_cursor(&layout, 5), Some(1));
        assert_eq!(block_index_for_cursor(&layout, 6), Some(2));
    }

    #[test]
    fn layout_marks_headings_and_wiki_links() {
        let source = "# Rome\nSee [[Caesar]].\n";
        let layout = build_layout(source);
        assert_eq!(layout.blocks[0].kind, BlockKind::Heading(1));
        assert!(layout.blocks[1].runs.iter().any(|run| run.kind == RunKind::WikiLink));
        let wiki = layout.blocks[1]
            .runs
            .iter()
            .find(|run| run.kind == RunKind::WikiLink)
            .expect("wiki run");
        assert_eq!(&source[wiki.range.clone()], "[[Caesar]]");
    }
}
