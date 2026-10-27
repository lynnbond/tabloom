//! Markdown presentation is ephemeral; the model always contains source bytes.
use crate::{
    editor::{
        commands::{self, TextCommand},
        highlighting::{HighlightCache, HighlightOptions},
    },
    model::Workspace,
    ui::{
        editor_region::{self, DocumentOutput, DocumentView},
        text,
    },
};
use eframe::egui::{self, FontId, Id, Key, Modifiers};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use uuid::Uuid;

pub fn source_id(region: Uuid) -> Id {
    Id::unique(("region-source", region))
}
pub fn has_markdown(w: &Workspace) -> bool {
    w.regions.iter().any(|r| r.syntax_id == "markdown")
}

#[derive(Default)]
pub struct MarkdownState {
    editing: Option<(Uuid, Uuid)>,
    pub source_workspace: Option<Uuid>,
    workspace: Option<Uuid>,
    regions: Vec<Uuid>,
    heights: std::collections::HashMap<Uuid, f32>,
    cache: CommonMarkCache,
    highlights: HighlightCache,
}
impl MarkdownState {
    pub fn finish_editing(&mut self) {
        self.editing = None;
    }
    pub fn mixed(&self, w: &Workspace) -> bool {
        has_markdown(w) && self.source_workspace != Some(w.id)
    }
}

pub fn show(
    ui: &mut egui::Ui,
    w: &mut Workspace,
    view: DocumentView<'_>,
    command: Option<(Id, TextCommand)>,
) -> DocumentOutput {
    let before = w.text();
    let editing_event = command.is_some()
        || ui.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Cut
                        | egui::Event::Ime(_)
                        | egui::Event::Key {
                            key: Key::Backspace
                                | Key::Delete
                                | Key::Tab
                                | Key::Enter
                                | Key::Z
                                | Key::Y,
                            pressed: true,
                            ..
                        }
                )
            })
        });
    if editing_event {
        view.state.record(w);
    }
    let state = &mut view.state.markdown;
    let region_ids: Vec<_> = w.regions.iter().map(|r| r.id).collect();
    if state.workspace != Some(w.id) || state.regions != region_ids {
        state.editing = None;
        state.workspace = Some(w.id);
        state.regions = region_ids;
        state.heights.clear();
        state.cache = CommonMarkCache::default();
        state.highlights = HighlightCache::default();
    }
    if !ui.ctx().input(|i| i.viewport().focused.unwrap_or(true)) {
        state.editing = None;
    }
    let mut output = DocumentOutput {
        changed: false,
        format: None,
    };
    let mut offset = 0;
    let mut global_selection = None;
    let mut active = w.active_region;
    let has_content = w.regions.iter().any(|r| !r.text.trim().is_empty());
    egui::ScrollArea::both()
        .id_salt(("markdown-document", w.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(28, 16))
                .show(ui, |ui| {
                    for region in &mut w.regions {
                        let top = ui.cursor().min.y;
                        let id = source_id(region.id);
                        let md = region.syntax_id == "markdown";
                        let pending = command
                            .as_ref()
                            .filter(|(target, _)| *target == id)
                            .map(|(_, cmd)| cmd);
                        let editing = state.editing == Some((w.id, region.id));
                        let empty = region.text.trim().is_empty();
                        // Empty syntax metadata is not document content: don't create a
                        // selectable preview/placeholder that Backspace cannot remove.
                        if md
                            && empty
                            && !editing
                            && pending.is_none()
                            && (has_content || active != region.id)
                        {
                            state.heights.remove(&region.id);
                            offset += region.text.len();
                            continue;
                        }
                        let too_large = region.text.len() > 256 * 1024;
                        let mut focus = view.request_focus && (!md || empty) && active == region.id;
                        let mut value = region.text.clone();
                        // A menu command still targets the original plain-text selection, even
                        // if its source editor lost focus while opening that menu.
                        let command_changed = pending
                            .is_some_and(|cmd| commands::apply(ui.ctx(), id, &mut value, cmd));
                        if command_changed
                            || (pending.is_some() && !matches!(pending, Some(TextCommand::Copy)))
                        {
                            if md {
                                state.editing = Some((w.id, region.id));
                            }
                            focus = true;
                        }
                        if md && !empty && !editing && !focus && !too_large {
                            let preview = ui
                                .push_id(region.id, |ui| {
                                    ui.style_mut().interaction.selectable_labels = false;
                                    ui.style_mut().text_styles.insert(
                                        egui::TextStyle::Body,
                                        FontId::proportional(view.font_size),
                                    );
                                    CommonMarkViewer::new().show(ui, &mut state.cache, &value);
                                    ui.allocate_space(egui::vec2(ui.available_width(), 6.));
                                })
                                .response;
                            let preview = ui
                                .interact(
                                    preview.rect,
                                    ui.scope_id().with(("markdown-preview", region.id)),
                                    egui::Sense::click(),
                                )
                                .on_hover_text(text::MARKDOWN_PREVIEW);
                            preview.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::Role::Button,
                                    true,
                                    text::MARKDOWN_PREVIEW,
                                )
                            });
                            if preview.clicked() {
                                active = region.id;
                            }
                            if preview.double_clicked() {
                                state.editing = Some((w.id, region.id));
                                active = region.id;
                                ui.ctx().request_repaint();
                                ui.memory_mut(|m| m.request_focus(id));
                            }
                            preview.context_menu(|ui| {
                                if ui.button(text::EDIT_MARKDOWN).clicked() {
                                    state.editing = Some((w.id, region.id));
                                    active = region.id;
                                    ui.memory_mut(|m| m.request_focus(id));
                                    ui.ctx().request_repaint();
                                    ui.close();
                                }
                                if ui.button(text::COPY_MARKDOWN).clicked() {
                                    ui.ctx().copy_text(value.clone());
                                    ui.close();
                                }
                            });
                        } else {
                            if md && too_large {
                                ui.weak(text::MARKDOWN_LARGE);
                            }
                            let intended = commands::selection(ui.ctx(), id, &value);
                            if ui.memory(|m| m.has_focus(id)) {
                                let multiline = value
                                    [commands::selected_bytes(ui.ctx(), id, &value)]
                                .contains('\n');
                                let indent = ui.input_mut(|i| {
                                    if i.consume_key(Modifiers::SHIFT, Key::Tab) {
                                        Some(true)
                                    } else if multiline && i.consume_key(Modifiers::NONE, Key::Tab)
                                    {
                                        Some(false)
                                    } else {
                                        None
                                    }
                                });
                                if let Some(backwards) = indent {
                                    commands::apply(
                                        ui.ctx(),
                                        id,
                                        &mut value,
                                        &TextCommand::Indent { backwards },
                                    );
                                }
                            }
                            let mut layout =
                                |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _: f32| {
                                    state.highlights.layout(
                                        ui,
                                        view.highlighter,
                                        region.id,
                                        buffer.as_str(),
                                        HighlightOptions {
                                            syntax: &region.syntax_id,
                                            size: view.font_size,
                                            matched: None,
                                        },
                                    )
                                };
                            let mut editor = egui::TextEdit::multiline(&mut value)
                                .id(id)
                                .code_editor()
                                .font(FontId::monospace(view.font_size))
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width())
                                .desired_rows(1)
                                .layouter(&mut layout)
                                .show(ui);
                            egui::Response::clone(&editor.response).on_hover_text(if md {
                                text::MARKDOWN_SOURCE
                            } else {
                                text::REGION_SOURCE
                            });
                            if focus {
                                editor.response.request_focus();
                            }
                            if (focus
                                || pending.is_some()
                                || egui::Popup::is_any_open(ui.ctx())
                                || ui.input(|i| {
                                    i.pointer.button_down(egui::PointerButton::Secondary)
                                })
                                || editor.response.secondary_clicked())
                                && !editor.response.changed()
                            {
                                editor.state.cursor.set_char_range(Some(intended));
                            }
                            editor.state.clone().store(ui.ctx(), id);
                            if editor.response.has_focus() || focus {
                                if md {
                                    state.editing = Some((w.id, region.id));
                                }
                                active = region.id;
                                let selected = commands::selected_bytes(ui.ctx(), id, &value);
                                global_selection =
                                    Some(offset + selected.start..offset + selected.end);
                            }
                            if md
                                && state.editing == Some((w.id, region.id))
                                && (editor.response.lost_focus()
                                    || ui.input(|i| i.key_pressed(Key::Escape)))
                                && !egui::Popup::is_any_open(ui.ctx())
                            {
                                state.editing = None;
                                editor.response.surrender_focus();
                                ui.ctx().request_repaint();
                            }
                            let selected = commands::selected_bytes(ui.ctx(), id, &value);
                            editor.response.context_menu(|ui| {
                                ui.add_enabled_ui(
                                    crate::model::document::whole_lines(&value, selected.clone())
                                        .is_some(),
                                    |ui| {
                                        ui.label(text::FORMAT_SELECTION);
                                        for language in view.highlighter.languages() {
                                            if ui.button(&language.label).clicked() {
                                                output.format = Some((
                                                    offset + selected.start..offset + selected.end,
                                                    language.id.clone(),
                                                ));
                                                ui.close();
                                            }
                                        }
                                    },
                                );
                            });
                            if let Some(error) = state.highlights.error(region.id) {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    format!("{}{error}", text::HIGHLIGHT_ERROR),
                                );
                            }
                        }
                        if md && !value.trim().is_empty() {
                            // Keep later spans stationary while preview/source focus changes.
                            let height = ui.cursor().min.y - top;
                            let reserved = state.heights.entry(region.id).or_default();
                            *reserved = reserved.max(height);
                            ui.add_space((*reserved - height).max(0.));
                        } else {
                            state.heights.remove(&region.id);
                        }
                        if value != region.text {
                            region.text = value;
                            region.touch();
                            output.changed = true;
                        }
                        offset += region.text.len();
                    }
                });
        });
    if active != w.active_region {
        w.active_region = active;
        output.changed = true;
    }
    if output.changed {
        w.touch();
    }
    let global = editor_region::editor_id(w.id);
    let after = w.text();
    if before != after {
        // Keep the whole-document undo history coherent when entering source mode.
        let mut editor = egui::TextEdit::load_state(ui.ctx(), global).unwrap_or_default();
        let mut undo = editor.undoer();
        undo.add_undo(&(commands::selection(ui.ctx(), global, &before), before));
        undo.add_undo(&(commands::selection(ui.ctx(), global, &after), after.clone()));
        editor.set_undoer(undo);
        editor.store(ui.ctx(), global);
        view.state.record(w);
    }
    if let Some(range) = global_selection {
        let mut value = after;
        commands::apply(
            ui.ctx(),
            global,
            &mut value,
            &TextCommand::SelectBytes(range),
        );
    }
    output
}
