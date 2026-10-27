use crate::{
    editor::{
        commands::{self, TextCommand},
        search::{SearchState, replace_matches},
    },
    model::EditorRegion,
    ui::{editor_region::editor_id, text},
};
use eframe::egui::{self, Id};

pub fn query_id() -> Id {
    Id::unique("find-query")
}
pub fn replacement_id() -> Id {
    Id::unique("find-replacement")
}

#[derive(Clone, Copy)]
pub enum FindAction {
    Next,
    Previous,
    ReplaceOne,
    ReplaceAll,
    Close,
}

pub struct FindOutput {
    pub action: Option<FindAction>,
    pub target_focus: Option<Id>,
    pub text_changed: bool,
}

// egui collapses inactive selections when IME ownership moves to a menu.
// Keep them so menu copy/cut/paste still targets the selection the user made.
fn field(ui: &mut egui::Ui, id: Id, value: &mut String, hint: &str) -> egui::Response {
    let selected = commands::selection(ui.ctx(), id, value);
    let mut output = egui::TextEdit::singleline(value)
        .id(id)
        .desired_width(210.0)
        .hint_text(hint)
        .show(ui);
    if !ui.memory(|m| m.has_focus(id)) && !output.response.changed() {
        output.state.cursor.set_char_range(Some(selected));
        output.state.store(ui.ctx(), id);
    }
    egui::Response::clone(&output.response).on_hover_text(hint)
}

pub fn show(
    ui: &mut egui::Ui,
    search: &mut SearchState,
    region: &EditorRegion,
    command: Option<(Id, TextCommand)>,
) -> FindOutput {
    let mut result = FindOutput {
        action: None,
        target_focus: None,
        text_changed: false,
    };
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} · {}", text::FIND_SCOPE, region.title));
        let command_query = command.as_ref().filter(|(id, _)| *id == query_id());
        if let Some((_, cmd)) = command_query {
            result.text_changed |= commands::apply(ui.ctx(), query_id(), &mut search.query, cmd);
        }
        let response = field(ui, query_id(), &mut search.query, text::FIND_QUERY);
        if search.request_query_focus || command_query.is_some() {
            response.request_focus();
            search.request_query_focus = false;
        }
        if response.has_focus() {
            result.target_focus = Some(query_id());
        }
        ui.checkbox(&mut search.options.case_sensitive, text::CASE_SENSITIVE);
        ui.checkbox(&mut search.options.whole_word, text::WHOLE_WORD);
        search.refresh(region.id, &region.text);
        let valid = !search.matches.is_empty();
        if ui
            .add_enabled(valid, egui::Button::new(text::FIND_PREVIOUS))
            .clicked()
        {
            result.action = Some(FindAction::Previous);
        }
        if ui
            .add_enabled(valid, egui::Button::new(text::FIND_NEXT))
            .clicked()
        {
            result.action = Some(FindAction::Next);
        }
        ui.label(format!(
            "{}/{} {}",
            search.current_index().map_or(0, |i| i + 1),
            search.matches.len(),
            text::MATCHES
        ));
        if ui.button(text::CLOSE_FIND).clicked() {
            result.action = Some(FindAction::Close);
        }
    });
    if search.show_replace {
        ui.horizontal_wrapped(|ui| {
            let command_replace = command.as_ref().filter(|(id, _)| *id == replacement_id());
            if let Some((_, cmd)) = command_replace {
                result.text_changed |=
                    commands::apply(ui.ctx(), replacement_id(), &mut search.replacement, cmd);
            }
            let response = field(
                ui,
                replacement_id(),
                &mut search.replacement,
                text::REPLACEMENT,
            );
            if command_replace.is_some() {
                response.request_focus();
            }
            if response.has_focus() {
                result.target_focus = Some(replacement_id());
            }
            if ui
                .add_enabled(
                    search.current.is_some(),
                    egui::Button::new(text::REPLACE_CURRENT),
                )
                .clicked()
            {
                result.action = Some(FindAction::ReplaceOne);
            }
            if ui
                .add_enabled(
                    !search.matches.is_empty(),
                    egui::Button::new(text::REPLACE_ALL),
                )
                .clicked()
            {
                result.action = Some(FindAction::ReplaceAll);
            }
            if let Some(count) = search.replaced {
                ui.label(format!("{} {count} {}", text::REPLACED, text::MATCHES));
            }
        });
    }
    if let Some(error) = &search.error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!("{}{error}", text::FIND_ERROR),
        );
    }
    result
}

/// Build a single atomic, undoable edit and the post-edit search anchor.
pub fn replacement(
    search: &SearchState,
    region: &EditorRegion,
    all: bool,
) -> Option<(TextCommand, usize, usize)> {
    if all {
        if search.matches.is_empty() {
            return None;
        }
        let replacement = replace_matches(&region.text, &search.matches, &search.replacement)?;
        Some((
            TextCommand::Replace {
                range: 0..region.text.len(),
                text: replacement,
            },
            0,
            search.matches.len(),
        ))
    } else {
        let range = search.current.clone()?;
        let anchor = range.start + search.replacement.len();
        Some((
            TextCommand::Replace {
                range,
                text: search.replacement.clone(),
            },
            anchor,
            1,
        ))
    }
}

pub fn apply_action(
    ctx: &egui::Context,
    search: &mut SearchState,
    region: &mut EditorRegion,
    action: FindAction,
) -> (bool, bool) {
    search.refresh(region.id, &region.text);
    match action {
        FindAction::Close => {
            search.open = false;
            (false, true)
        }
        FindAction::Next | FindAction::Previous => {
            if let Some(range) = search.navigate(matches!(action, FindAction::Previous)) {
                commands::apply(
                    ctx,
                    editor_id(region.id),
                    &mut region.text,
                    &TextCommand::SelectBytes(range),
                );
            }
            (false, true)
        }
        FindAction::ReplaceOne | FindAction::ReplaceAll => {
            let Some((command, anchor, count)) =
                replacement(search, region, matches!(action, FindAction::ReplaceAll))
            else {
                return (false, false);
            };
            let changed = commands::apply(ctx, editor_id(region.id), &mut region.text, &command);
            if changed {
                region.touch();
            }
            search.refresh(region.id, &region.text);
            search.select_from(anchor);
            search.replaced = Some(if changed { count } else { 0 });
            (changed, true)
        }
    }
}
