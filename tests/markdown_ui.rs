use eframe::egui::{self, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use tabloom::{
    app::TabloomApp,
    editor::commands::{self, TextCommand},
    ui::{markdown, text},
};

fn harness() -> Harness<'static, TabloomApp> {
    let mut app = TabloomApp::default();
    let w = app.document.active_mut().unwrap();
    w.regions[0].text = "# 标题 😀\r\n\r\n**粗体** 与 `code`\r\n\r\n- one\r\n- two\r\n".into();
    w.regions[0].syntax_id = "markdown".into();
    let plain = w.add_region("区域");
    w.regions.iter_mut().find(|r| r.id == plain).unwrap().text = "plain note".into();
    Harness::builder()
        .with_step_dt(1. / 60.)
        .with_max_steps(32)
        .with_os(egui::os::OperatingSystem::from_target_os())
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut TabloomApp| app.show(ui), app)
}
fn edit(h: &mut Harness<'_, TabloomApp>) {
    h.run_steps(30);
    h.get_by_label(text::MARKDOWN_PREVIEW).click();
    h.get_by_label(text::MARKDOWN_PREVIEW).click();
    h.run();
    assert!(h.query_by_label(text::MARKDOWN_SOURCE).is_some());
}
#[test]
fn preview_is_default_and_double_click_edits_source_then_blur_renders() {
    let mut h = harness();
    let before = h.state().document.active().unwrap().text();
    assert!(h.query_by_label(text::MARKDOWN_PREVIEW).is_some());
    assert!(h.query_by_label(text::MARKDOWN_SOURCE).is_none());
    edit(&mut h);
    h.get_by_label(text::MARKDOWN_SOURCE).click();
    h.run();
    h.event(egui::Event::Text("new 中文\t".into()));
    h.run();
    assert_ne!(h.state().document.active().unwrap().text(), before);
    let edited = h.state().document.active().unwrap().text();
    h.get_by_label(text::REGION_SOURCE).click();
    h.run();
    assert!(h.query_by_label(text::MARKDOWN_PREVIEW).is_some());
    assert!(h.query_by_label(text::MARKDOWN_SOURCE).is_none());
    assert_eq!(h.state().document.active().unwrap().text(), edited);
}
#[test]
fn source_toggle_and_escape_never_rewrite_markdown() {
    let mut h = harness();
    let before = h.state().document.active().unwrap().clone();
    h.get_by_label(text::DOCUMENT_SOURCE).click();
    h.run();
    assert!(h.query_by_label(text::DOCUMENT_EDITOR).is_some());
    h.get_by_label(text::DOCUMENT_PREVIEW).click();
    h.run();
    edit(&mut h);
    h.key_press(Key::Escape);
    h.run();
    assert!(h.query_by_label(text::MARKDOWN_PREVIEW).is_some());
    let after = h.state().document.active().unwrap();
    assert_eq!(after.text(), before.text());
    assert_eq!(after.regions[0].syntax_id, "markdown");
}
#[test]
fn local_undo_and_plain_neighbor_are_independent() {
    let mut h = harness();
    edit(&mut h);
    let w = h.state().document.active().unwrap();
    let before = w.regions[0].text.clone();
    let id = markdown::source_id(w.regions[0].id);
    let mut value = before.clone();
    commands::apply(&h.ctx, id, &mut value, &TextCommand::SelectAll);
    h.event(egui::Event::Text("# replacement".into()));
    h.run();
    assert_eq!(
        h.state().document.active().unwrap().regions[0].text,
        "# replacement"
    );
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run();
    assert_eq!(h.state().document.active().unwrap().regions[0].text, before);
    assert_eq!(
        h.state().document.active().unwrap().regions[1].text,
        "plain note"
    );
}
#[test]
fn preview_context_copy_outputs_original_markdown_only() {
    let mut h = harness();
    let original = h.state().document.active().unwrap().regions[0].text.clone();
    let clipboard = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    struct Capture(std::sync::Arc<std::sync::Mutex<String>>);
    impl tabloom::editor::clipboard::TextClipboard for Capture {
        fn get_text(&self) -> Result<String, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn set_text(&self, value: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = value.into();
            Ok(())
        }
    }
    h.ctx
        .add_plugin(tabloom::editor::clipboard::PlainTextClipboard::new(
            Capture(clipboard.clone()),
        ));
    h.get_by_label(text::MARKDOWN_PREVIEW).click_secondary();
    h.run();
    h.get_by_label(text::COPY_MARKDOWN).click();
    h.run();
    assert_eq!(*clipboard.lock().unwrap(), original);
}

#[test]
fn markdown_edits_persist_as_source_and_restart_in_preview() {
    use tabloom::{persistence::Store, settings::AppPaths};
    let directory = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(directory.path().to_path_buf());
    let fixture = harness().state().document.clone();
    let mut store = Store::open(paths.clone()).unwrap();
    store.save(&fixture, &Default::default()).unwrap();
    drop(store);
    let app = TabloomApp::persistent(&egui::Context::default(), Ok(paths.clone()));
    let mut h = Harness::builder()
        .with_step_dt(1. / 60.)
        .with_max_steps(32)
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut TabloomApp| app.show(ui), app);
    edit(&mut h);
    h.event(egui::Event::Text("\r\n## saved 中文😀\t".into()));
    h.run();
    assert!(h.state().autosave().unwrap().dirty());
    h.get_by_label(text::REGION_SOURCE).click();
    h.run();
    let expected = h.state().document.clone();
    h.state_mut().flush().unwrap();
    drop(h);
    let app = TabloomApp::persistent(&egui::Context::default(), Ok(paths));
    assert_eq!(app.document, expected);
    let mut restored = Harness::builder()
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut TabloomApp| app.show(ui), app);
    restored.run();
    assert!(restored.query_by_label(text::MARKDOWN_PREVIEW).is_some());
    assert_eq!(
        restored.state().document.active().unwrap().text(),
        expected.active().unwrap().text()
    );
}

#[test]
fn multiple_markdown_spans_switch_edit_target_without_changing_other_sources() {
    let mut h = harness();
    let w = h.state_mut().document.active_mut().unwrap();
    let id = w.add_region("区域");
    let r = w.regions.iter_mut().find(|r| r.id == id).unwrap();
    r.syntax_id = "markdown".into();
    r.text = "## second markdown".into();
    h.run();
    h.run_steps(30);
    h.get_all_by_label(text::MARKDOWN_PREVIEW)
        .next()
        .unwrap()
        .click();
    h.get_all_by_label(text::MARKDOWN_PREVIEW)
        .next()
        .unwrap()
        .click();
    h.run();
    assert_eq!(h.get_all_by_label(text::MARKDOWN_SOURCE).count(), 1);
    h.run_steps(30);
    h.get_by_label(text::MARKDOWN_PREVIEW).click();
    h.get_by_label(text::MARKDOWN_PREVIEW).click();
    h.run();
    assert_eq!(h.get_all_by_label(text::MARKDOWN_SOURCE).count(), 1);
    h.event(egui::Event::Text(" added".into()));
    h.run();
    assert!(
        h.state()
            .document
            .active()
            .unwrap()
            .regions
            .last()
            .unwrap()
            .text
            .contains("added")
    );
    assert!(
        h.state().document.active().unwrap().regions[0]
            .text
            .starts_with("# 标题")
    );
}

#[test]
fn empty_trailing_markdown_has_no_placeholder_preview_or_text_mutation() {
    let mut h = harness();
    let w = h.state_mut().document.active_mut().unwrap();
    let id = w.add_region("区域");
    let region = w.regions.iter_mut().find(|r| r.id == id).unwrap();
    region.syntax_id = "markdown".into();
    region.text = "\r\n\t ".into();
    let original = w.text();
    h.run();
    assert_eq!(h.get_all_by_label(text::MARKDOWN_PREVIEW).count(), 1);
    assert!(h.query_by_label("空 Markdown · 双击开始编辑").is_none());
    assert_eq!(h.state().document.active().unwrap().text(), original);
}

#[test]
fn entirely_empty_markdown_accepts_continuous_typing_without_becoming_preview() {
    let mut h = harness();
    let w = h.state_mut().document.active_mut().unwrap();
    w.regions.truncate(1);
    w.regions[0].text.clear();
    w.active_region = w.regions[0].id;
    h.run();
    assert!(h.query_by_label(text::MARKDOWN_PREVIEW).is_none());
    h.get_by_label(text::MARKDOWN_SOURCE).click();
    h.run();
    h.event(egui::Event::Text("# first".into()));
    h.run();
    h.event(egui::Event::Text(" second".into()));
    h.run();
    assert_eq!(
        h.state().document.active().unwrap().text(),
        "# first second"
    );
    assert!(h.query_by_label(text::MARKDOWN_SOURCE).is_some());
}
