use eframe::egui::{self, Id, Key, Modifiers, os::OperatingSystem};
use egui_kittest::{Harness, kittest::Queryable};
use std::sync::{Arc, Mutex};
use tabloom::{
    app::TabloomApp,
    editor::{
        clipboard::{PlainTextClipboard, TextClipboard},
        commands,
    },
    settings::AppPaths,
    ui::{editor_region::editor_id, text},
};

#[derive(Default)]
struct ClipData {
    text: String,
    writes: usize,
    reads: usize,
    fail_write: bool,
    fail_read: bool,
}
#[derive(Clone, Default)]
struct MemoryClipboard(Arc<Mutex<ClipData>>);
impl TextClipboard for MemoryClipboard {
    fn get_text(&self) -> Result<String, String> {
        let mut state = self.0.lock().unwrap();
        state.reads += 1;
        if state.fail_read {
            Err("test clipboard read error".into())
        } else {
            Ok(state.text.clone())
        }
    }
    fn set_text(&self, value: &str) -> Result<(), String> {
        let mut state = self.0.lock().unwrap();
        state.writes += 1;
        if state.fail_write {
            Err("test clipboard write error".into())
        } else {
            state.text = value.into();
            Ok(())
        }
    }
}

fn harness(os: OperatingSystem) -> (Harness<'static, TabloomApp>, MemoryClipboard) {
    with_app(os, TabloomApp::default())
}
fn with_app(
    os: OperatingSystem,
    app: TabloomApp,
) -> (Harness<'static, TabloomApp>, MemoryClipboard) {
    let clipboard = MemoryClipboard::default();
    let backend = clipboard.clone();
    let harness = Harness::builder()
        .with_os(os)
        .with_size((1100.0, 900.0))
        .allow_missing_glyphs()
        .with_context_setup(move |ctx| ctx.add_plugin(PlainTextClipboard::new(backend)))
        .build_ui_state(|ui, app: &mut TabloomApp| app.show(ui), app);
    (harness, clipboard)
}
fn active_id(h: &Harness<'_, TabloomApp>) -> Id {
    editor_id(h.state().document.active().unwrap().id)
}
fn active_text<'a>(h: &'a Harness<'_, TabloomApp>) -> &'a str {
    let w = h.state().document.active().unwrap();
    &w.regions
        .iter()
        .find(|r| r.id == w.active_region)
        .unwrap()
        .text
}
fn type_text(h: &mut Harness<'_, TabloomApp>, text: &str) {
    h.event(egui::Event::Text(text.into()));
    h.run();
}
fn key(h: &mut Harness<'_, TabloomApp>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run();
}
fn replace_panel(h: &mut Harness<'_, TabloomApp>) {
    let modifiers = tabloom::ui::shortcuts::replace_modifiers(h.ctx.os() == OperatingSystem::Mac);
    key(h, modifiers, Key::H);
}

#[test]
fn keyboard_copy_cut_paste_are_raw_text_and_are_consumed_once() {
    let (mut h, clipboard) = harness(OperatingSystem::Mac);
    let original = "{\r\n\t\"中文😀\": \"<b>literal</b>\"\n}\r";
    type_text(&mut h, original);
    h.state_mut().document.active_mut().unwrap().regions[0].syntax_id = "json".into();
    h.run();
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::COMMAND, Key::C);
    assert_eq!(clipboard.0.lock().unwrap().text, original);
    assert_eq!(clipboard.0.lock().unwrap().writes, 1);
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(active_text(&h), "");
    assert_eq!(
        clipboard.0.lock().unwrap().writes,
        2,
        "cut must not issue two system writes"
    );
    key(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(active_text(&h), original);
    assert_eq!(clipboard.0.lock().unwrap().reads, 1);
}

#[test]
fn copy_with_no_selection_does_not_include_titles_or_the_entire_region() {
    let (mut h, clipboard) = harness(OperatingSystem::Nix);
    type_text(&mut h, "hello");
    key(&mut h, Modifiers::COMMAND, Key::C);
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(clipboard.0.lock().unwrap().writes, 0);
    assert_eq!(active_text(&h), "hello");
}

#[test]
fn clipboard_write_failure_does_not_delete_the_cut_selection() {
    let (mut h, clipboard) = harness(OperatingSystem::Windows);
    type_text(&mut h, "must not lose 中文😀");
    key(&mut h, Modifiers::COMMAND, Key::A);
    clipboard.0.lock().unwrap().fail_write = true;
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(active_text(&h), "must not lose 中文😀");
    assert!(h.query_by_label(text::CLEAR_CLIPBOARD_ERROR).is_some());
    clipboard.0.lock().unwrap().fail_write = false;
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(active_text(&h), "");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(active_text(&h), "must not lose 中文😀");
}

#[test]
fn paste_error_is_visible_and_does_not_change_text() {
    let (mut h, clipboard) = harness(OperatingSystem::Nix);
    type_text(&mut h, "original");
    clipboard.0.lock().unwrap().fail_read = true;
    key(&mut h, Modifiers::COMMAND, Key::V);
    assert_eq!(active_text(&h), "original");
    assert!(h.query_by_label(text::CLEAR_CLIPBOARD_ERROR).is_some());
}

#[test]
fn replace_one_and_all_are_individually_undoable_and_literal() {
    let (mut h, _) = harness(OperatingSystem::Mac);
    let original = "cat 中文 cat\r\ncat";
    type_text(&mut h, original);
    replace_panel(&mut h);
    type_text(&mut h, "cat");
    h.get_by_label(text::REPLACEMENT).click();
    h.run();
    type_text(&mut h, "$1\\猫😀");
    assert!(h.query_by_label("1/3 处匹配").is_some());
    h.get_by_label(text::REPLACE_CURRENT).click();
    h.run();
    assert_eq!(active_text(&h), "$1\\猫😀 中文 cat\r\ncat");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(active_text(&h), original);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    assert_eq!(active_text(&h), "$1\\猫😀 中文 cat\r\ncat");
    h.get_by_label(text::REPLACE_ALL).click();
    h.run();
    assert_eq!(active_text(&h), "$1\\猫😀 中文 $1\\猫😀\r\n$1\\猫😀");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(active_text(&h), "$1\\猫😀 中文 cat\r\ncat");
}

#[test]
fn find_navigation_selects_unicode_matches_and_wraps() {
    let (mut h, _) = harness(OperatingSystem::Nix);
    type_text(&mut h, "猫😀 x 猫😀");
    key(&mut h, Modifiers::COMMAND, Key::F);
    type_text(&mut h, "猫😀");
    h.get_by_label(text::FIND_NEXT).click();
    h.run();
    let selected = commands::selected_bytes(&h.ctx, active_id(&h), active_text(&h));
    assert_eq!(selected, 10..17);
    assert_eq!(&active_text(&h)[selected], "猫😀");
    key(&mut h, Modifiers::NONE, Key::F3);
    assert_eq!(
        commands::selected_bytes(&h.ctx, active_id(&h), active_text(&h)),
        0..7
    );
    key(&mut h, Modifiers::SHIFT, Key::F3);
    assert_eq!(
        commands::selected_bytes(&h.ctx, active_id(&h), active_text(&h)),
        10..17
    );
}

#[test]
fn find_field_clipboard_and_undo_commands_do_not_edit_the_region() {
    let (mut h, clipboard) = harness(OperatingSystem::Windows);
    type_text(&mut h, "keep editor unchanged");
    key(&mut h, Modifiers::COMMAND, Key::F);
    type_text(&mut h, "query");
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(clipboard.0.lock().unwrap().text, "query");
    assert_eq!(active_text(&h), "keep editor unchanged");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(
        h.get_by_label(text::FIND_QUERY).value(),
        Some("query".into())
    );
    assert_eq!(active_text(&h), "keep editor unchanged");
    key(&mut h, Modifiers::NONE, Key::Escape);
    assert!(h.query_by_label(text::FIND_QUERY).is_none());
    assert!(h.ctx.memory(|m| m.has_focus(active_id(&h))));
}

#[test]
fn find_and_replace_operate_on_the_whole_document_across_syntax_spans() {
    let (mut h, _) = harness(OperatingSystem::Nix);
    type_text(&mut h, "one one\none");
    let mut value = h.state().document.active().unwrap().text();
    commands::apply(
        &h.ctx,
        active_id(&h),
        &mut value,
        &commands::TextCommand::SelectBytes(8..11),
    );
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label(text::EXTRACT_SELECTION).click();
    h.run();
    assert_eq!(h.state().document.active().unwrap().regions.len(), 2);
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    replace_panel(&mut h);
    key(&mut h, Modifiers::COMMAND, Key::A);
    type_text(&mut h, "one");
    assert!(h.query_by_label("1/3 处匹配").is_some());
    h.get_by_label(text::REPLACEMENT).click();
    h.run();
    type_text(&mut h, "two");
    h.get_by_label(text::REPLACE_ALL).click();
    h.run();
    assert_eq!(h.state().document.active().unwrap().text(), "two two\ntwo");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(h.state().document.active().unwrap().text(), "one one\none");
}

#[test]
fn page_up_down_and_shift_selection_move_without_mutating_text() {
    let (mut h, _) = harness(OperatingSystem::Windows);
    let original = (0..80)
        .map(|i| format!("line {i:02} 中文😀\r\n"))
        .collect::<String>();
    type_text(&mut h, &original);
    key(&mut h, Modifiers::CTRL, Key::Home);
    let initial = commands::selection(&h.ctx, active_id(&h), active_text(&h))
        .primary
        .index;
    key(&mut h, Modifiers::NONE, Key::PageDown);
    let next = commands::selection(&h.ctx, active_id(&h), active_text(&h))
        .primary
        .index;
    assert!(next > initial);
    key(&mut h, Modifiers::SHIFT, Key::PageDown);
    assert!(!commands::selection(&h.ctx, active_id(&h), active_text(&h)).is_empty());
    key(&mut h, Modifiers::NONE, Key::PageUp);
    assert_eq!(active_text(&h), original);
}

#[test]
fn selected_lines_indent_and_outdent_preserve_crlf_and_undo() {
    let (mut h, _) = harness(OperatingSystem::Nix);
    let original = "a\r\n  中文😀\n\tlast";
    type_text(&mut h, original);
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::NONE, Key::Tab);
    assert_eq!(active_text(&h), "\ta\r\n\t  中文😀\n\t\tlast");
    key(&mut h, Modifiers::SHIFT, Key::Tab);
    assert_eq!(active_text(&h), original);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(active_text(&h), "\ta\r\n\t  中文😀\n\t\tlast");
}

#[test]
fn font_shortcuts_are_clamped_persisted_and_do_not_insert_symbols() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let ctx = egui::Context::default();
    let app = TabloomApp::persistent(&ctx, Ok(paths.clone()));
    let (mut h, _) = with_app(OperatingSystem::Mac, app);
    key(&mut h, Modifiers::COMMAND | Modifiers::SHIFT, Key::Equals);
    assert_eq!(h.state().snapshot().preferences.font_size, 16.0);
    key(&mut h, Modifiers::COMMAND, Key::Minus);
    assert_eq!(h.state().snapshot().preferences.font_size, 15.0);
    assert_eq!(active_text(&h), "");
    h.state_mut().flush().unwrap();
    drop(h);
    let db = tabloom::persistence::Store::open(paths).unwrap();
    assert_eq!(db.load().unwrap().unwrap().preferences.font_size, 15.0);
}

#[test]
fn platform_find_replace_bindings_and_clipboard_target_are_consistent() {
    for os in [
        OperatingSystem::Mac,
        OperatingSystem::Windows,
        OperatingSystem::Nix,
    ] {
        let (mut h, clipboard) = harness(os);
        type_text(&mut h, "Cat cat cats");
        replace_panel(&mut h);
        type_text(&mut h, "cat");
        assert!(h.query_by_label("1/3 处匹配").is_some());
        h.get_by_label(text::CASE_SENSITIVE).click();
        h.run();
        assert!(h.query_by_label("1/2 处匹配").is_some());
        h.get_by_label(text::WHOLE_WORD).click();
        h.run();
        assert!(h.query_by_label("1/1 处匹配").is_some());
        h.get_by_label(text::FIND_QUERY).click();
        h.run();
        key(&mut h, Modifiers::COMMAND, Key::A);
        // Menu commands must keep the last text field as their target.
        h.get_by_label(text::EDIT).click();
        h.run();
        h.get_by_label(&format!(
            "{} {}",
            text::COPY,
            tabloom::ui::shortcuts::command_label(os, "C")
        ))
        .click();
        h.run();
        assert_eq!(clipboard.0.lock().unwrap().text, "cat");
        assert_eq!(active_text(&h), "Cat cat cats");
    }
}

#[test]
fn home_end_delete_backspace_and_arrows_edit_only_the_current_region() {
    let (mut h, _) = harness(OperatingSystem::Windows);
    type_text(&mut h, "first\n中文😀last");
    key(&mut h, Modifiers::NONE, Key::Home);
    key(&mut h, Modifiers::NONE, Key::Delete);
    key(&mut h, Modifiers::NONE, Key::ArrowRight);
    key(&mut h, Modifiers::NONE, Key::Backspace);
    key(&mut h, Modifiers::NONE, Key::End);
    type_text(&mut h, "!");
    assert_eq!(active_text(&h), "first\n😀last!");
}

#[test]
fn replacement_is_saved_and_recovered_with_other_regions_intact() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let app = TabloomApp::persistent(&egui::Context::default(), Ok(paths.clone()));
    let (mut h, _) = with_app(OperatingSystem::Nix, app);
    type_text(&mut h, "one\r\n中文😀 one");
    replace_panel(&mut h);
    type_text(&mut h, "one");
    h.get_by_label(text::REPLACEMENT).click();
    h.run();
    type_text(&mut h, "two");
    h.get_by_label(text::REPLACE_ALL).click();
    h.run();
    h.state_mut().flush().unwrap();
    drop(h);
    let db = tabloom::persistence::Store::open(paths).unwrap();
    let snapshot = db.load().unwrap().unwrap();
    assert_eq!(
        snapshot.document.active().unwrap().regions[0].text,
        "two\r\n中文😀 two"
    );
}

#[test]
fn edit_menu_preserves_region_selection_and_cut_is_undoable() {
    let (mut h, clipboard) = harness(OperatingSystem::Mac);
    let original = "\t中文😀\r\nraw";
    type_text(&mut h, original);
    key(&mut h, Modifiers::COMMAND, Key::A);
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label("剪切 ⌘X").click();
    h.run();
    assert_eq!(active_text(&h), "");
    assert_eq!(clipboard.0.lock().unwrap().text, original);
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(active_text(&h), original);
}

#[test]
fn copy_cut_paste_and_undo_span_the_entire_continuous_document() {
    let (mut h, clipboard) = harness(OperatingSystem::Mac);
    let original = "α\n中文😀\r\nlast";
    type_text(&mut h, original);
    let mut value = original.to_owned();
    commands::apply(
        &h.ctx,
        active_id(&h),
        &mut value,
        &commands::TextCommand::SelectBytes(3..13),
    );
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label(text::EXTRACT_SELECTION).click();
    h.run();
    assert_eq!(h.state().document.active().unwrap().regions.len(), 3);
    key(&mut h, Modifiers::COMMAND, Key::A);
    key(&mut h, Modifiers::COMMAND, Key::C);
    assert_eq!(clipboard.0.lock().unwrap().text, original);
    key(&mut h, Modifiers::COMMAND, Key::X);
    assert_eq!(h.state().document.active().unwrap().text(), "");
    key(&mut h, Modifiers::COMMAND, Key::Z);
    assert_eq!(h.state().document.active().unwrap().text(), original);
    assert_eq!(h.state().document.active().unwrap().regions.len(), 3);
}
