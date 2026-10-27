use eframe::egui::{self, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use tabloom::{app::TabloomApp, persistence::Store, settings::AppPaths, ui::text};

fn harness(paths: &AppPaths) -> Harness<'static, TabloomApp> {
    let ctx = egui::Context::default();
    let app = TabloomApp::persistent(&ctx, Ok(paths.clone()));
    Harness::builder()
        .with_size((1100.0, 900.0))
        .allow_missing_glyphs()
        .build_ui_state(|ui, app: &mut TabloomApp| app.show(ui), app)
}

#[test]
fn real_widget_edits_and_structure_changes_are_saved_and_restored() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let mut h = harness(&paths);
    h.event(egui::Event::Text("{\"中文😀\":true}\r\n".into()));
    h.run();
    assert!(h.state().autosave().unwrap().dirty());
    let w = h.state().document.active().unwrap();
    let id = tabloom::ui::editor_region::editor_id(w.id);
    let start = w.region_offset(w.active_region).unwrap();
    let mut value = w.text();
    let end = value.len();
    tabloom::editor::commands::apply(
        &h.ctx,
        id,
        &mut value,
        &tabloom::editor::commands::TextCommand::SelectBytes(start..end),
    );
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run();
    h.get_by_label(text::DOCUMENT_EDITOR).click_secondary();
    h.run();
    h.get_by_label("JSON").click();
    h.run();
    h.get_by_label(text::EDIT).click();
    h.run();
    h.get_by_label(text::SPAN_TOOLS).click();
    h.run();
    h.get_by_label(text::NEW_REGION).click();
    h.run();
    h.event(egui::Event::Text("SELECT 1;".into()));
    h.run();
    let w = h.state().document.active().unwrap();
    let id = tabloom::ui::editor_region::editor_id(w.id);
    let start = w.region_offset(w.active_region).unwrap();
    let mut value = w.text();
    let end = value.len();
    tabloom::editor::commands::apply(
        &h.ctx,
        id,
        &mut value,
        &tabloom::editor::commands::TextCommand::SelectBytes(start..end),
    );
    h.ctx.memory_mut(|m| m.request_focus(id));
    h.run();
    h.get_by_label(text::DOCUMENT_EDITOR).click_secondary();
    h.run();
    h.get_by_label("SQL").scroll_to_me();
    h.run();
    h.get_by_label("SQL").click();
    h.run();
    h.get_by_label(text::NEW_WORKSPACE).click();
    h.run();
    h.event(egui::Event::Text("other tab".into()));
    h.run();
    h.state_mut().flush().unwrap();
    let before = h.state().document.clone();
    assert_eq!(before.workspaces[0].regions[0].syntax_id, "json");
    assert_eq!(before.workspaces[0].regions[1].syntax_id, "sql");
    assert!(!h.state().autosave().unwrap().dirty());
    drop(h);
    let mut restored = harness(&paths);
    assert_eq!(restored.state().document, before);
    restored.key_press_modifiers(Modifiers::COMMAND, Key::W);
    restored.run();
    restored.state_mut().flush().unwrap();
    let closed = restored.state().document.clone();
    drop(restored);
    let db = Store::open(paths).unwrap();
    assert_eq!(db.load().unwrap().unwrap().document, closed);
}

#[test]
fn failed_save_shows_error_keeps_editor_usable_and_retry_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let mut h = harness(&paths);
    std::fs::rename(paths.workspaces(), paths.root.join("workspaces-held")).unwrap();
    std::fs::write(paths.workspaces(), b"blocks workspace writes").unwrap();
    h.event(egui::Event::Text("retain this".into()));
    h.run();
    assert!(h.state_mut().flush().is_err());
    h.run();
    assert!(h.query_by_label(text::RETRY_SAVE).is_some());
    assert!(h.query_by_label(text::SAVE_FAILED).is_some());
    assert!(h.query_by_label(text::SAVED).is_none());
    // Restoring focus and typing still works while the error banner is visible.
    h.get_by_label(text::DOCUMENT_EDITOR).click();
    h.run();
    h.event(egui::Event::Text(" + continued".into()));
    h.run();
    assert_eq!(
        h.state().document.active().unwrap().regions[0].text,
        "retain this + continued"
    );
    std::fs::remove_file(paths.workspaces()).unwrap();
    std::fs::rename(paths.root.join("workspaces-held"), paths.workspaces()).unwrap();
    h.get_by_label(text::RETRY_SAVE).click();
    h.run();
    assert!(!h.state().autosave().unwrap().dirty());
    assert!(h.query_by_label(text::RETRY_SAVE).is_none());
    assert!(h.query_by_label(text::SAVED).is_some());
}

#[test]
fn normal_window_close_flushes_pending_text_without_a_save_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let mut h = harness(&paths);
    h.event(egui::Event::Text("close before debounce".into()));
    h.run();
    h.input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    h.run();
    assert!(h.query_by_label(text::CONFIRM_EXIT).is_none());
    assert!(!h.state().autosave().unwrap().dirty());
    drop(h);
    let db = Store::open(paths).unwrap();
    assert_eq!(
        db.load()
            .unwrap()
            .unwrap()
            .document
            .active()
            .unwrap()
            .regions[0]
            .text,
        "close before debounce"
    );
}

#[test]
fn close_with_write_failure_is_cancelled_and_recovery_copy_is_available() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let mut h = harness(&paths);
    std::fs::rename(paths.workspaces(), paths.root.join("workspaces-held")).unwrap();
    std::fs::write(paths.workspaces(), b"blocks workspace writes").unwrap();
    h.event(egui::Event::Text("unsaved but visible".into()));
    h.run();
    h.input_mut()
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    h.step();
    assert!(
        h.output()
            .viewport_output
            .values()
            .any(|v| v.commands.contains(&egui::ViewportCommand::CancelClose))
    );
    h.run();
    assert!(h.query_by_label(text::RETRY_AND_EXIT).is_some());
    assert!(h.state().autosave().unwrap().dirty());
    assert_eq!(
        h.state().document.active().unwrap().regions[0].text,
        "unsaved but visible"
    );
}

#[test]
fn second_instance_opens_isolated_editable_recovery_and_restores_it() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let first = Store::open(paths.clone()).unwrap();
    let mut h = harness(&paths);
    h.run();
    assert!(h.query_by_label(text::RECOVERY_EDITING).is_some());
    assert!(h.query_by_label(text::DOCUMENT_EDITOR).is_some());
    h.event(egui::Event::Text("second instance 中文😀\r\n".into()));
    h.run();
    h.state_mut().flush().unwrap();
    assert!(first.load().unwrap().is_none());
    drop(h);
    let restored = harness(&paths);
    assert_eq!(
        restored.state().document.active().unwrap().text(),
        "second instance 中文😀\r\n"
    );
}

#[test]
fn permanent_delete_requires_confirmation_and_is_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let mut h = harness(&paths);
    h.key_press_modifiers(Modifiers::COMMAND, Key::N);
    h.run();
    h.event(egui::Event::Text("to be closed".into()));
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::W);
    h.run();
    assert_eq!(h.state().document.workspaces.len(), 2);
    h.get_by_label(text::FILE).click();
    h.run();
    h.get_by_label(text::RECENTLY_CLOSED).click();
    h.run();
    h.get_by_label(text::PERMANENT_DELETE).click();
    h.run();
    assert_eq!(h.state().document.workspaces.len(), 2);
    h.get_by_label(text::CONFIRM_PERMANENT_DELETE).click();
    h.run();
    assert_eq!(h.state().document.workspaces.len(), 1);
    h.state_mut().flush().unwrap();
    drop(h);
    let db = Store::open(paths).unwrap();
    assert_eq!(db.load().unwrap().unwrap().document.workspaces.len(), 1);
}

#[test]
fn emergency_recovery_copy_contains_all_unsaved_text_as_json() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().to_path_buf());
    let mut h = harness(&paths);
    std::fs::rename(paths.workspaces(), paths.root.join("workspaces-held")).unwrap();
    std::fs::write(paths.workspaces(), b"blocks workspace writes").unwrap();
    h.event(egui::Event::Text("中文😀\t\r\nrecover me".into()));
    h.run();
    assert!(h.state_mut().flush().is_err());
    h.run();
    h.get_by_label(text::COPY_RESCUE).click();
    h.step();
    let json = h
        .output()
        .platform_output
        .commands
        .iter()
        .find_map(|c| {
            if let egui::OutputCommand::CopyText(text) = c {
                Some(text.clone())
            } else {
                None
            }
        })
        .expect("copy emits a plain text clipboard command");
    let snapshot: tabloom::persistence::Snapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(snapshot.document, h.state().document);
    assert_eq!(
        snapshot.document.active().unwrap().regions[0].text,
        "中文😀\t\r\nrecover me"
    );
}

#[test]
fn corrupt_primary_without_backup_still_opens_editor_and_preserves_original() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    std::fs::create_dir_all(&paths.root).unwrap();
    std::fs::write(paths.session(), b"damaged precious state").unwrap();
    let mut h = harness(&paths);
    h.run();
    assert!(h.query_by_label(text::RECOVERY_EDITING).is_some());
    h.event(egui::Event::Text("editable despite startup failure".into()));
    h.run();
    h.state_mut().flush().unwrap();
    assert_eq!(
        std::fs::read(paths.session()).unwrap(),
        b"damaged precious state"
    );
    assert_eq!(
        h.state().document.active().unwrap().text(),
        "editable despite startup failure"
    );
    drop(h);
    let h = harness(&paths);
    assert_eq!(
        h.state().document.active().unwrap().text(),
        "editable despite startup failure"
    );
}
#[test]
fn invalid_data_directory_and_newer_format_do_not_block_editing() {
    for blocked_directory in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(dir.path().join("data"));
        if blocked_directory {
            std::fs::write(&paths.root, b"blocks data directory").unwrap();
        } else {
            std::fs::create_dir_all(&paths.root).unwrap();
            std::fs::write(paths.session(), br#"{"version":99}"#).unwrap();
        }
        let mut h = harness(&paths);
        h.run();
        h.event(egui::Event::Text("still usable 中文😀".into()));
        h.run();
        h.state_mut().flush().unwrap();
        assert_eq!(
            h.state().document.active().unwrap().text(),
            "still usable 中文😀"
        );
        assert!(h.query_by_label(text::RECOVERY_EDITING).is_some());
    }
}
