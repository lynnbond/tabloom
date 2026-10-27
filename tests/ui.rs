use eframe::egui::{self, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use tabloom::{
    app::TabloomApp,
    editor::commands::{self, TextCommand},
    ui::{editor_region::editor_id, text},
};
fn harness() -> Harness<'static, TabloomApp> {
    Harness::builder()
        .with_os(egui::os::OperatingSystem::from_target_os())
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, a: &mut TabloomApp| a.show(ui), TabloomApp::default())
}
fn input(h: &mut Harness<'_, TabloomApp>, value: &str) {
    h.event(egui::Event::Text(value.into()));
    h.run();
}
fn select(h: &mut Harness<'_, TabloomApp>, range: std::ops::Range<usize>) {
    let w = h.state().document.active().unwrap();
    let id = editor_id(w.id);
    let mut value = w.text();
    commands::apply(&h.ctx, id, &mut value, &TextCommand::SelectBytes(range));
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run();
}
fn extract(h: &mut Harness<'_, TabloomApp>) {
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label(text::EXTRACT_SELECTION).click();
    h.run();
}
fn syntax(h: &mut Harness<'_, TabloomApp>, name: &str) {
    h.get_by_label(text::DOCUMENT_EDITOR).click_secondary();
    h.run();
    h.get_by_label(name).scroll_to_me();
    h.run();
    h.get_by_label(name).click();
    h.run();
}
fn span_menu(h: &mut Harness<'_, TabloomApp>) {
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label(text::SPAN_TOOLS).click();
    h.run();
}

#[test]
fn typing_new_tabs_and_switching_keep_contents_separate() {
    let mut h = harness();
    let original = "first 中文😀\t\r\nnext\nlast\r";
    input(&mut h, original);
    let first = h.state().document.active_workspace;
    h.get_by_label(text::NEW_WORKSPACE).click();
    h.run();
    input(&mut h, "second");
    h.get_by_label("临时工作区 1").click();
    h.run();
    assert_eq!(h.state().document.active_workspace, first);
    assert_eq!(h.state().document.active().unwrap().text(), original);
    assert!(h.ctx.memory(|m| m.has_focus(editor_id(first.unwrap()))));
}
#[test]
fn selected_lines_become_invisible_spans_with_exact_text_and_one_editor() {
    let mut h = harness();
    let value = "first\r\n中文😀\tdata\r\nlast";
    input(&mut h, value);
    let start = "first\r\n".len();
    let end = start + "中文😀\tdata".len();
    select(&mut h, start..end);
    extract(&mut h);
    let w = h.state().document.active().unwrap();
    assert_eq!(w.text(), value);
    assert_eq!(w.regions.len(), 3);
    assert_eq!(w.regions[1].text, "中文😀\tdata\r\n");
    assert_eq!(w.active_region, w.regions[1].id);
    assert_eq!(h.get_all_by_label(text::DOCUMENT_EDITOR).count(), 1);
    assert!(h.query_by_label(text::RESIZE).is_none());
    assert!(h.query_by_label(text::FOCUS).is_none());
    assert!(h.query_by_label(text::NEW_REGION).is_none());
}
#[test]
fn language_changes_are_local_to_selected_line_spans() {
    let mut h = harness();
    input(&mut h, "json\nsql\nnote");
    select(&mut h, 0..4);
    syntax(&mut h, "JSON");
    select(&mut h, 5..8);
    syntax(&mut h, "SQL");
    let w = h.state().document.active().unwrap();
    assert_eq!(w.text(), "json\nsql\nnote");
    assert_eq!(
        w.regions
            .iter()
            .map(|r| r.syntax_id.as_str())
            .collect::<Vec<_>>(),
        ["json", "sql", "plain"]
    );
}
#[test]
fn document_undo_redo_crosses_spans_and_preserves_their_languages() {
    let mut h = harness();
    input(&mut h, "first\nsecond\nlast");
    select(&mut h, 6..12);
    syntax(&mut h, "SQL");
    let original = h.state().document.active().unwrap().text();
    let ids = h
        .state()
        .document
        .active()
        .unwrap()
        .regions
        .iter()
        .map(|r| (r.id, r.syntax_id.clone()))
        .collect::<Vec<_>>();
    select(&mut h, 0..original.len());
    input(&mut h, "replacement");
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    let w = h.state().document.active().unwrap();
    assert_eq!(w.text(), original);
    assert_eq!(
        w.regions
            .iter()
            .map(|r| (r.id, r.syntax_id.clone()))
            .collect::<Vec<_>>(),
        ids
    );
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z);
    h.run();
    assert_eq!(h.state().document.active().unwrap().text(), "replacement");
}
#[test]
fn deleting_the_last_span_requires_confirmation_and_keeps_the_document_editable() {
    let mut h = harness();
    input(&mut h, "retain until confirmed");
    span_menu(&mut h);
    h.get_by_label(text::DELETE).click();
    h.run();
    assert_eq!(
        h.state().document.active().unwrap().text(),
        "retain until confirmed"
    );
    h.get_by_label(text::CONFIRM_DELETE).click();
    h.run();
    input(&mut h, "still editable");
    assert_eq!(
        h.state().document.active().unwrap().text(),
        "still editable"
    );
}
#[test]
fn workspace_shortcuts_do_not_insert_a_literal_tab() {
    let mut h = harness();
    let first = h.state().document.active_workspace;
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run();
    h.key_press_modifiers(
        if cfg!(target_os = "macos") {
            Modifiers::CTRL
        } else {
            Modifiers::COMMAND
        },
        Key::Tab,
    );
    h.run();
    assert_eq!(h.state().document.active_workspace, first);
    assert_eq!(h.state().document.active().unwrap().text(), "");
    h.key_press(Key::Tab);
    h.run();
    assert_eq!(h.state().document.active().unwrap().text(), "\t");
}
#[test]
fn narrow_window_keeps_the_document_usable() {
    let mut h = Harness::builder()
        .with_size((480., 360.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, a: &mut TabloomApp| a.show(ui), TabloomApp::default());
    input(&mut h, "small window");
    assert_eq!(h.state().document.active().unwrap().text(), "small window");
    assert!(h.get_by_label(text::DOCUMENT_EDITOR).rect().width() > 100.);
}
#[test]
fn large_text_fallback_is_visible_and_does_not_rewrite_language_or_content() {
    let mut h = harness();
    let original = "fn main() { println!(\"中文😀\"); }\r\n".repeat(9000);
    let r = &mut h.state_mut().document.active_mut().unwrap().regions[0];
    r.text = original.clone();
    r.syntax_id = "rust".into();
    h.run();
    assert!(h.query_by_label(text::LARGE_TEXT_NOTE).is_some());
    assert_eq!(h.state().document.active().unwrap().text(), original);
    h.state_mut().document.active_mut().unwrap().regions[0].text = "fn main() {}".into();
    h.run();
    assert!(h.query_by_label(text::LARGE_TEXT_NOTE).is_none());
}

#[test]
fn partial_line_cannot_be_formatted_and_footer_is_read_only() {
    let mut h = harness();
    input(&mut h, "first\nsecond");
    select(&mut h, 1..4);
    h.get_by_label(text::DOCUMENT_EDITOR).click_secondary();
    h.run();
    h.get_by_label("JSON").click();
    h.run();
    assert_eq!(h.state().document.active().unwrap().regions.len(), 1);
    assert!(h.query_by_label(text::SYNTAX).is_none());
}

#[test]
fn right_click_format_preserves_unicode_crlf_and_reports_mixed_selection() {
    let mut h = harness();
    let value = "first\r\n中文😀\tdata\r\nlast";
    input(&mut h, value);
    select(&mut h, 7..7 + "中文😀\tdata".len());
    syntax(&mut h, "JSON");
    assert_eq!(h.state().document.active().unwrap().text(), value);
    assert_eq!(
        h.state().document.active().unwrap().regions[1].syntax_id,
        "json"
    );
    assert!(h.query_by_label("格式：JSON").is_some());
    select(&mut h, 0..value.len());
    assert!(h.query_by_label("格式：混合格式").is_some());
    syntax(&mut h, "SQL");
    assert_eq!(h.state().document.active().unwrap().text(), value);
    assert!(
        h.state()
            .document
            .active()
            .unwrap()
            .regions
            .iter()
            .all(|r| r.syntax_id == "sql")
    );
}
