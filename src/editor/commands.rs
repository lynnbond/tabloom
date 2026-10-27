//! Small adapters around egui's existing cursor and undo state, not a new editor core.
use super::clipboard::{self, char_to_byte};
use eframe::egui::{
    self, Id, TextEdit,
    text::{CCursor, CCursorRange},
};
use std::ops::Range;

#[derive(Debug, Clone)]
pub enum TextCommand {
    SelectBytes(Range<usize>),
    SelectAll,
    Copy,
    Cut,
    Paste(String),
    Undo,
    Redo,
    Replace { range: Range<usize>, text: String },
    Indent { backwards: bool },
}

pub fn selection(ctx: &egui::Context, id: Id, text: &str) -> CCursorRange {
    let count = text.chars().count();
    let mut range = TextEdit::load_state(ctx, id)
        .and_then(|s| s.cursor.char_range())
        .unwrap_or_else(|| CCursorRange::one(CCursor::new(count)));
    range.primary.index = range.primary.index.min(count.into());
    range.secondary.index = range.secondary.index.min(count.into());
    range
}

pub fn selected_bytes(ctx: &egui::Context, id: Id, text: &str) -> Range<usize> {
    let range = selection(ctx, id, text).as_sorted_char_range();
    char_to_byte(text, range.start.into())..char_to_byte(text, range.end.into())
}

pub fn byte_selection(text: &str, range: Range<usize>) -> Option<CCursorRange> {
    if range.start > range.end || text.get(range.clone()).is_none() {
        return None;
    }
    Some(CCursorRange::two(
        CCursor::new(text[..range.start].chars().count()),
        CCursor::new(text[..range.end].chars().count()),
    ))
}

pub fn apply(ctx: &egui::Context, id: Id, text: &mut String, command: &TextCommand) -> bool {
    let mut state = TextEdit::load_state(ctx, id).unwrap_or_default();
    let before_cursor = selection(ctx, id, text);
    let mut after_cursor = before_cursor;
    let range = selected_bytes(ctx, id, text);
    let edit = match command {
        TextCommand::SelectAll => {
            after_cursor = CCursorRange::select_all_str(text);
            None
        }
        TextCommand::SelectBytes(range) => {
            if let Some(cursor) = byte_selection(text, range.clone()) {
                after_cursor = cursor;
            }
            None
        }
        TextCommand::Copy => {
            if !range.is_empty() {
                ctx.copy_text(text[range].into());
            }
            None
        }
        TextCommand::Cut => {
            if range.is_empty() || !clipboard::guard_cut(ctx, &text[range.clone()]) {
                return false;
            }
            ctx.copy_text(text[range.clone()].into());
            Some((range, String::new(), None))
        }
        TextCommand::Paste(paste) => {
            if paste.is_empty() {
                return false;
            }
            Some((range, paste.clone(), None))
        }
        TextCommand::Replace { range, text } => Some((range.clone(), text.clone(), None)),
        TextCommand::Indent { backwards } => indent_lines(text, range, *backwards),
        TextCommand::Undo | TextCommand::Redo => {
            let current = (before_cursor, text.clone());
            let mut undoer = state.undoer();
            undoer.feed_state(ctx.input(|i| i.time), &current);
            let restored = if matches!(command, TextCommand::Undo) {
                undoer.undo(&current)
            } else {
                undoer.redo(&current)
            }
            .cloned();
            let changed = restored.as_ref().is_some_and(|(_, t)| t != text);
            if let Some((cursor, restored)) = restored {
                *text = restored;
                state.cursor.set_char_range(Some(cursor));
            }
            state.set_undoer(undoer);
            state.store(ctx, id);
            return changed;
        }
    };
    let mut changed = false;
    if let Some((range, replacement, select)) = edit {
        let Some(previous) = text.get(range.clone()) else {
            return false;
        };
        if previous != replacement {
            let mut undoer = state.undoer();
            let now = ctx.input(|i| i.time);
            let before = (before_cursor, text.clone());
            undoer.feed_state(now, &before);
            undoer.add_undo(&before);
            let end = text[..range.start].chars().count() + replacement.chars().count();
            text.replace_range(range, &replacement);
            after_cursor = select
                .and_then(|r| byte_selection(text, r))
                .unwrap_or_else(|| CCursorRange::one(CCursor::new(end)));
            let after = (after_cursor, text.clone());
            undoer.feed_state(now, &after);
            undoer.add_undo(&after);
            state.set_undoer(undoer);
            changed = true;
        }
    }
    state.cursor.set_char_range(Some(after_cursor));
    state.store(ctx, id);
    changed
}

type BlockEdit = (Range<usize>, String, Option<Range<usize>>);

/// Add/remove leading indentation without normalizing LF, CRLF, spaces or tabs.
fn indent_lines(text: &str, selected: Range<usize>, backwards: bool) -> Option<BlockEdit> {
    let start = text[..selected.start].rfind('\n').map_or(0, |i| i + 1);
    let end = if selected.end > selected.start && text[..selected.end].ends_with('\n') {
        selected.end
    } else {
        text[selected.end..]
            .find('\n')
            .map_or(text.len(), |i| selected.end + i + 1)
    };
    let mut replacement = String::new();
    for line in text[start..end].split_inclusive('\n') {
        if backwards {
            let count = if line.starts_with('\t') {
                1
            } else {
                line.chars().take(4).take_while(|c| *c == ' ').count()
            };
            replacement.push_str(&line[count..]);
        } else {
            replacement.push('\t');
            replacement.push_str(line);
        }
    }
    if start == end && !backwards {
        replacement.push('\t');
    }
    let select = if selected.is_empty() {
        let removed = (end - start).saturating_sub(replacement.len());
        let caret = selected.start.saturating_sub(removed).max(start);
        Some(caret..caret)
    } else {
        Some(start..start + replacement.len())
    };
    Some((start..end, replacement, select))
}
