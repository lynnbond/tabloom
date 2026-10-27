//! A single borderless TextEdit per workspace. Regions are invisible syntax spans.
use crate::{
    editor::{
        clipboard::char_to_byte,
        commands::{self, TextCommand},
        highlighting::{Highlighter, highlighting_limited},
    },
    model::{EditorRegion, Workspace, document::whole_lines},
    ui::{text, theme},
};
use eframe::egui::{self, Color32, FontId, Id, Key, Modifiers, text::LayoutJob};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::Range,
    sync::Arc,
};
use uuid::Uuid;

pub fn editor_id(workspace: Uuid) -> Id {
    Id::unique(("document-editor", workspace))
}

struct CachedLayout {
    spans: Vec<(Uuid, usize, String)>,
    size: f32,
    appearance: crate::settings::Appearance,
    scale: f32,
    matched: Option<Range<usize>>,
    galley: Arc<egui::Galley>,
    error: Option<String>,
}
#[derive(Clone)]
struct SpanSnapshot {
    digest: [u8; 32],
    spans: Vec<(EditorRegion, usize)>,
}
#[derive(Default)]
pub struct DocumentState {
    pub markdown: crate::ui::markdown::MarkdownState,
    layouts: HashMap<Uuid, CachedLayout>,
    history: HashMap<Uuid, VecDeque<SpanSnapshot>>,
}
impl DocumentState {
    pub fn error(&self, id: Uuid) -> Option<&str> {
        self.layouts.get(&id).and_then(|c| c.error.as_deref())
    }
    pub fn remove(&mut self, id: Uuid) {
        self.layouts.remove(&id);
        self.history.remove(&id);
    }
    pub fn record(&mut self, w: &Workspace) {
        let digest = crate::file_io::hash(w.text().as_bytes());
        let history = self.history.entry(w.id).or_default();
        if history.back().is_some_and(|s| s.digest == digest) {
            history.pop_back();
        }
        let spans = w
            .regions
            .iter()
            .map(|r| {
                let len = r.text.len();
                let mut metadata = r.clone();
                metadata.text = String::new();
                (metadata, len)
            })
            .collect();
        history.push_back(SpanSnapshot { digest, spans });
    }
    fn prune(&mut self, ctx: &egui::Context, w: &Workspace) {
        let Some(history) = self.history.get_mut(&w.id) else {
            return;
        };
        if history.len() <= 2048 {
            return;
        }
        // Preserve every checkpoint still reachable through egui undo/redo; only
        // discard old transient typing versions. Snapshots contain lengths, not text copies.
        let value = w.text();
        let id = editor_id(w.id);
        let state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let mut undoer = state.undoer();
        let mut current = (commands::selection(ctx, id, &value), value);
        let mut keep = HashSet::new();
        keep.insert(crate::file_io::hash(current.1.as_bytes()));
        while let Some(previous) = undoer.undo(&current).cloned() {
            current = previous;
            keep.insert(crate::file_io::hash(current.1.as_bytes()));
        }
        while let Some(next) = undoer.redo(&current).cloned() {
            current = next;
            keep.insert(crate::file_io::hash(current.1.as_bytes()));
        }
        let cutoff = history.len().saturating_sub(1024);
        let mut index = 0;
        history.retain(|snapshot| {
            let retain = index >= cutoff || keep.contains(&snapshot.digest);
            index += 1;
            retain
        });
    }
    pub fn apply(
        &mut self,
        w: &mut Workspace,
        new: &str,
        restore: bool,
        hint: Option<Range<usize>>,
    ) {
        self.record(w);
        let digest = crate::file_io::hash(new.as_bytes());
        let snapshot = restore
            .then(|| {
                self.history
                    .get(&w.id)
                    .and_then(|h| h.iter().rev().find(|s| s.digest == digest))
                    .cloned()
            })
            .flatten();
        if let Some(snapshot) = snapshot {
            let mut offset = 0;
            let mut regions = Vec::new();
            for (mut r, len) in snapshot.spans {
                r.text = new[offset..offset + len].into();
                offset += len;
                regions.push(r);
            }
            for r in &mut regions {
                if let Some(current) = w.regions.iter().find(|c| c.id == r.id) {
                    r.syntax_id = current.syntax_id.clone();
                }
                if let Some(path) = &r.file_path
                    && let Some(current) = w
                        .regions
                        .iter()
                        .find(|c| c.file_path.as_ref() == Some(path))
                {
                    r.file_state = current.file_state.clone();
                }
                r.touch();
            }
            w.regions = regions;
            if !w.regions.iter().any(|r| r.id == w.active_region) {
                w.active_region = w.regions[0].id;
            }
            w.touch();
        } else {
            w.apply_text_at(new, hint);
        }
        self.record(w);
    }
    fn layout(
        &mut self,
        ui: &egui::Ui,
        h: &Highlighter,
        w: &Workspace,
        value: &str,
        size: f32,
        matched: Option<Range<usize>>,
    ) -> Arc<egui::Galley> {
        // Only the visible document needs a retained galley. Undo metadata is
        // independent and stays available when switching back or reopening a tab.
        self.layouts.retain(|id, _| *id == w.id);
        let spans: Vec<_> = w
            .regions
            .iter()
            .map(|r| (r.id, r.text.len(), r.syntax_id.clone()))
            .collect();
        let appearance = theme::current(ui.ctx());
        let palette = theme::palette(appearance);
        let dark = appearance.dark();
        let scale = ui.ctx().pixels_per_point();
        if let Some(c) = self.layouts.get(&w.id)
            && c.galley.job.text == value
            && c.spans == spans
            && c.size == size
            && c.appearance == appearance
            && c.scale == scale
            && c.matched == matched
        {
            return c.galley.clone();
        }
        // During an input pass TextEdit can call the layouter with text newer than the model.
        let projected = if w
            .regions
            .iter()
            .flat_map(|r| r.text.bytes())
            .eq(value.bytes())
        {
            std::borrow::Cow::Borrowed(w)
        } else {
            let mut edited = w.clone();
            edited.apply_text(value);
            std::borrow::Cow::Owned(edited)
        };
        let mut job = LayoutJob::default();
        let mut error = None;
        for r in &projected.regions {
            let syntax = &r.syntax_id;
            let part = match h.layout_job_styled(&r.text, syntax, size, dark, palette.syntax) {
                Ok(part) => part,
                Err(message) => {
                    error = Some(message);
                    LayoutJob::simple(
                        r.text.clone(),
                        FontId::monospace(size),
                        ui.visuals().text_color(),
                        f32::INFINITY,
                    )
                }
            };
            let offset = job.text.len();
            job.text.push_str(&part.text);
            for mut s in part.sections {
                s.byte_range = (usize::from(s.byte_range.start) + offset).into()
                    ..(usize::from(s.byte_range.end) + offset).into();
                job.sections.push(s);
            }
        }
        if let Some(range) = &matched
            && value.get(range.clone()).is_some()
        {
            let mut sections = Vec::new();
            for section in job.sections {
                let start = usize::from(section.byte_range.start);
                let end = usize::from(section.byte_range.end);
                let a = start.max(range.start);
                let b = end.min(range.end);
                if a >= b {
                    sections.push(section);
                    continue;
                }
                if start < a {
                    sections.push(egui::text::LayoutSection {
                        byte_range: start.into()..a.into(),
                        ..section.clone()
                    });
                }
                let mut marked = section.clone();
                marked.byte_range = a.into()..b.into();
                marked.format.background = if dark {
                    Color32::from_rgb(65, 78, 110)
                } else {
                    Color32::from_rgb(250, 222, 145)
                };
                sections.push(marked);
                if b < end {
                    sections.push(egui::text::LayoutSection {
                        byte_range: b.into()..end.into(),
                        ..section
                    });
                }
            }
            job.sections = sections;
        }
        debug_assert_eq!(job.text, value);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        if error.is_some() {
            ui.ctx().request_repaint();
        }
        self.layouts.insert(
            w.id,
            CachedLayout {
                spans,
                size,
                appearance,
                scale,
                matched,
                galley: galley.clone(),
                error,
            },
        );
        galley
    }
}

pub struct DocumentView<'a> {
    pub highlighter: &'a Highlighter,
    pub state: &'a mut DocumentState,
    pub font_size: f32,
    pub request_focus: bool,
    pub command: Option<&'a TextCommand>,
    pub matched: Option<Range<usize>>,
}
pub struct DocumentOutput {
    pub changed: bool,
    pub format: Option<(Range<usize>, String)>,
}

pub fn show(ui: &mut egui::Ui, w: &mut Workspace, view: DocumentView<'_>) -> DocumentOutput {
    let id = editor_id(w.id);
    let mut value = w.text();
    let before = value.clone();
    let spans_changed = view.state.layouts.get(&w.id).is_some_and(|c| {
        c.spans.len() != w.regions.len()
            || c.spans
                .iter()
                .zip(&w.regions)
                .any(|((id, _, syntax), r)| *id != r.id || *syntax != r.syntax_id)
    });
    if spans_changed {
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        let mut undoer = state.undoer();
        undoer.add_undo(&(commands::selection(ui.ctx(), id, &value), value.clone()));
        state.set_undoer(undoer);
        state.store(ui.ctx(), id);
    }

    if !commands::selected_bytes(ui.ctx(), id, &value).is_empty()
        && ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Text(_))))
    {
        let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
        let mut undoer = state.undoer();
        undoer.add_undo(&(commands::selection(ui.ctx(), id, &value), value.clone()));
        state.set_undoer(undoer);
        state.store(ui.ctx(), id);
    }
    let mut edit_hint = commands::selected_bytes(ui.ctx(), id, &value);
    let mut restore = matches!(view.command, Some(TextCommand::Undo | TextCommand::Redo));
    if let Some(command) = view.command {
        commands::apply(ui.ctx(), id, &mut value, command);
    }
    let focused = ui.memory(|m| m.has_focus(id));
    let mut page = None;
    if focused {
        restore|=ui.input(|i|i.events.iter().any(|e|matches!(e,egui::Event::Key{key:Key::Z|Key::Y,modifiers,pressed:true,..} if modifiers.command)));
        let bytes = commands::selected_bytes(ui.ctx(), id, &value);
        let multiline = value[bytes].contains('\n');
        let indent = ui.input_mut(|i| {
            if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                Some(true)
            } else if multiline && i.consume_key(Modifiers::NONE, Key::Tab) {
                Some(false)
            } else {
                None
            }
        });
        if let Some(backwards) = indent {
            commands::apply(ui.ctx(), id, &mut value, &TextCommand::Indent { backwards });
        }
        page = ui.input_mut(|i| {
            let found = i.events.iter().find_map(|e| {
                if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } = e
                {
                    (matches!(key, Key::PageUp | Key::PageDown)
                        && !modifiers.command
                        && !modifiers.ctrl
                        && !modifiers.alt)
                        .then_some((*key, modifiers.shift))
                } else {
                    None
                }
            });
            if found.is_some() {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key {
                            key: Key::PageUp | Key::PageDown,
                            pressed: true,
                            ..
                        }
                    )
                });
            }
            found
        });
    }
    let height = ui.available_height().max(100.0);
    let appearance = theme::current(ui.ctx());
    let palette = theme::palette(appearance);
    let gutter = appearance != crate::settings::Appearance::Sand;
    let mut format = None;
    egui::ScrollArea::both()
        .id_salt(("document-scroll", w.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let intended = commands::selection(ui.ctx(), id, &value);
            let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                view.state.layout(
                    ui,
                    view.highlighter,
                    w,
                    buffer.as_str(),
                    view.font_size,
                    view.matched.clone(),
                )
            };
            let mut editor = egui::TextEdit::multiline(&mut value)
                .id(id)
                .code_editor()
                .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                    left: if gutter { 48 } else { 28 },
                    right: 16,
                    top: 20,
                    bottom: 20,
                }))
                .font(FontId::monospace(view.font_size))
                .desired_width(ui.available_width())
                .min_size(egui::vec2(ui.available_width(), height))
                .hint_text(text::EMPTY_HINT)
                .layouter(&mut layouter)
                .show(ui);
            if gutter {
                let clip = ui.clip_rect();
                for (index, row) in editor.galley.rows.iter().enumerate() {
                    let rect = row.rect().translate(editor.galley_pos.to_vec2());
                    if rect.top() > clip.bottom() {
                        break;
                    }
                    if rect.bottom() < clip.top() {
                        continue;
                    }
                    ui.painter().text(
                        egui::pos2(editor.response.rect.left() + 30., rect.center().y),
                        egui::Align2::RIGHT_CENTER,
                        (index + 1).to_string(),
                        FontId::monospace(11.),
                        palette.faint,
                    );
                }
            }
            egui::Response::clone(&editor.response).on_hover_text(text::DOCUMENT_EDITOR);
            if view.request_focus || view.command.is_some() {
                editor.response.request_focus();
            }
            // Preserve selected lines throughout secondary press/release and popup interaction.
            if (view.request_focus
                || view.command.is_some()
                || !ui.memory(|m| m.has_focus(id))
                || egui::Popup::is_any_open(ui.ctx())
                || editor.response.secondary_clicked()
                || ui.input(|i| i.pointer.button_down(egui::PointerButton::Secondary)))
                && !editor.response.changed()
            {
                editor.state.cursor.set_char_range(Some(intended));
            }
            if let Some((key, shift)) = page {
                let mut range = editor.state.cursor.char_range().unwrap_or_default();
                let pos = editor.galley.pos_from_cursor(range.primary);
                let x = range.h_pos.unwrap_or(pos.min.x);
                range.primary = editor.galley.cursor_from_pos(egui::vec2(
                    x,
                    pos.center().y
                        + (height - pos.height()).max(pos.height())
                            * if key == Key::PageUp { -1. } else { 1. },
                ));
                if !shift {
                    range.secondary = range.primary;
                }
                range.h_pos = Some(x);
                editor.state.cursor.set_char_range(Some(range));
            }
            editor.state.clone().store(ui.ctx(), id);
            if view.request_focus || view.command.is_some() || page.is_some() {
                if let Some(range) = editor.state.cursor.char_range() {
                    let rect = editor
                        .galley
                        .pos_from_cursor(range.primary)
                        .translate(editor.galley_pos.to_vec2());
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                }
                ui.ctx().request_repaint();
            }
            let selected = commands::selected_bytes(ui.ctx(), id, &value);
            editor.response.context_menu(|ui| {
                ui.add_enabled_ui(whole_lines(&value, selected.clone()).is_some(), |ui| {
                    ui.label(text::FORMAT_SELECTION);
                    egui::ScrollArea::vertical()
                        .max_height(320.)
                        .show(ui, |ui| {
                            for language in view.highlighter.languages() {
                                if ui.button(&language.label).clicked() {
                                    format = Some((selected.clone(), language.id.clone()));
                                    ui.close();
                                }
                            }
                        });
                })
                .response
                .on_hover_text(text::WHOLE_LINE_HINT);
            });
        });
    let changed = before != value;
    if changed {
        if edit_hint.is_empty() && value.len() < before.len() {
            let after = commands::selection(ui.ctx(), id, &value).primary.index;
            let after = char_to_byte(&value, after.into());
            edit_hint = if after < edit_hint.start {
                after..edit_hint.start
            } else {
                edit_hint.start..edit_hint.start + before.len() - value.len()
            };
        }
        view.state.apply(w, &value, restore, Some(edit_hint));
    }
    let selected = commands::selected_bytes(ui.ctx(), id, &value);
    let cursor = commands::selection(ui.ctx(), id, &value).primary.index;
    let byte = if selected.is_empty() {
        char_to_byte(&value, cursor.into())
    } else {
        selected.start
    };
    let active = if selected.is_empty()
        && w.regions
            .iter()
            .find(|r| r.id == w.active_region)
            .is_some_and(|r| w.region_offset(r.id).unwrap() + r.text.len() == byte)
    {
        w.active_region
    } else {
        w.region_at(byte)
    };
    let active_changed = active != w.active_region;
    if active_changed {
        w.active_region = active;
        w.touch();
    }
    view.state.prune(ui.ctx(), w);
    DocumentOutput {
        changed: changed || active_changed,
        format,
    }
}

pub fn limited(w: &Workspace) -> bool {
    w.regions
        .iter()
        .any(|r| highlighting_limited(&r.text, &r.syntax_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_documents_keeps_one_layout_and_preserves_undo_metadata() {
        let mut first = Workspace::new("first", "region");
        first.regions[0].text = "first 中文😀".into();
        let mut second = Workspace::new("second", "region");
        second.regions[0].text = "second SQL".into();
        let mut cache = DocumentState::default();
        cache.record(&first);
        cache.record(&second);
        let h = Highlighter::default();
        let docs = [first, second];
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .build_ui_state(
                move |ui, (cache, index): &mut (DocumentState, usize)| {
                    let w = &docs[*index];
                    let galley = cache.layout(ui, &h, w, &w.text(), 15., None);
                    assert_eq!(galley.job.text, w.text());
                    assert_eq!(cache.layouts.len(), 1);
                    assert!(cache.layouts.contains_key(&w.id));
                },
                (cache, 0),
            );
        harness.state_mut().1 = 1;
        harness.run();
        assert_eq!(harness.state().0.history.len(), 2);
        harness.state_mut().1 = 0;
        harness.run();
        assert_eq!(harness.state().0.history.len(), 2);
    }
    #[test]
    fn history_pruning_keeps_span_metadata_for_reachable_undo_checkpoints() {
        let ctx = egui::Context::default();
        let mut w = Workspace::new("document", "region");
        w.regions[0].text = "one\ntwo\nthree".into();
        w.extract_lines(4..7, "region").unwrap();
        w.regions[1].syntax_id = "sql".into();
        let original = w.clone();
        let mut cache = DocumentState::default();
        cache.record(&w);
        let mut state = egui::text_edit::TextEditState::default();
        let mut undoer = state.undoer();
        undoer.add_undo(&(egui::text::CCursorRange::default(), w.text()));
        undoer.add_undo(&(egui::text::CCursorRange::default(), String::new()));
        state.set_undoer(undoer);
        state.store(&ctx, editor_id(w.id));
        for n in 0..2100 {
            w.apply_text(&format!("transient {n}"));
            cache.record(&w);
        }
        cache.prune(&ctx, &w);
        assert!(cache.history[&w.id].len() < 1100);
        cache.apply(&mut w, &original.text(), true, None);
        assert_eq!(w.text(), original.text());
        assert_eq!(
            w.regions
                .iter()
                .map(|r| (r.id, &r.syntax_id))
                .collect::<Vec<_>>(),
            original
                .regions
                .iter()
                .map(|r| (r.id, &r.syntax_id))
                .collect::<Vec<_>>()
        );
    }
}
