use eframe::egui::{self, Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use std::fs;
use tabloom::{
    app::TabloomApp,
    file_io,
    model::TextEncoding,
    persistence::Store,
    settings::AppPaths,
    ui::{editor_region::editor_id, text},
};

fn app(paths: &AppPaths) -> TabloomApp {
    TabloomApp::persistent(&egui::Context::default(), Ok(paths.clone()))
}
fn edit(app: &mut TabloomApp, value: &str) {
    let w = app.document.active_mut().unwrap();
    w.regions[0].text = value.into();
    w.regions[0].touch();
    w.touch();
    app.mark_changed();
}

#[test]
fn imported_files_keep_recovery_copy_and_association_across_restart_without_implicit_disk_writes() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("配置.json");
    let original = "{\r\n\t\"中文😀\":true\n}\r";
    fs::write(&source, file_io::encode(original, TextEncoding::Utf16Be)).unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    let before = a.document.workspaces[0].clone();
    assert_eq!(a.document.active().unwrap().regions[0].syntax_id, "json");
    edit(&mut a, "local 中文😀\r\n\t");
    a.flush().unwrap();
    assert_eq!(file_io::open(&source).unwrap().0, original);
    drop(a);
    let mut a = app(&paths);
    assert_eq!(
        a.document.active().unwrap().regions[0].text,
        "local 中文😀\r\n\t"
    );
    assert_eq!(a.document.workspaces[0], before);
    a.save_current_file().unwrap();
    assert_eq!(file_io::open(&source).unwrap().0, "local 中文😀\r\n\t");
    assert_eq!(
        file_io::open(&source).unwrap().1.encoding,
        TextEncoding::Utf16Be
    );
    edit(&mut a, "second save");
    a.save_current_file().unwrap();
    assert_eq!(file_io::open(&source).unwrap().0, "second save");
    drop(a);
    let mut a = app(&paths);
    a.save_current_file().unwrap();
}
#[test]
fn temporary_save_never_demands_a_filename_and_export_does_not_associate() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let mut a = app(&paths);
    edit(&mut a, "temporary\t中文😀\r\n");
    assert!(a.save_current_file().unwrap().contains("自动保存"));
    let dest = dir.path().join("export.txt");
    a.export_current_to(&dest).unwrap();
    assert_eq!(fs::read_to_string(dest).unwrap(), "temporary\t中文😀\r\n");
    assert!(a.document.active().unwrap().regions[0].file_path.is_none());
}
#[test]
fn failed_open_does_not_create_tabs_and_internal_data_cannot_be_exported_over() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let mut a = app(&paths);
    fs::write(dir.path().join("bad.bin"), [0xff, 0xff]).unwrap();
    let before = a.snapshot();
    assert!(a.open_file_path(&dir.path().join("bad.bin")).is_err());
    assert_eq!(a.snapshot(), before);
    assert!(a.export_current_to(&paths.session()).is_err());
    assert!(a.open_file_path(&paths.session()).is_err());
    a.flush().unwrap();
}
#[test]
fn external_conflict_after_restart_preserves_both_disk_and_recovery_text() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("a.rs");
    fs::write(&source, "original").unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    edit(&mut a, "local");
    a.flush().unwrap();
    drop(a);
    fs::write(&source, "external").unwrap();
    let mut a = app(&paths);
    assert!(a.save_current_file().unwrap_err().contains("外部修改"));
    assert_eq!(fs::read_to_string(&source).unwrap(), "external");
    assert_eq!(a.document.active().unwrap().regions[0].text, "local");
    let copy = dir.path().join("local.rs");
    a.export_current_to(&copy).unwrap();
    assert_eq!(fs::read_to_string(copy).unwrap(), "local");
    assert!(a.export_current_to(&source).is_err());
}
#[test]
fn storage_failure_blocks_original_write_but_allows_rescue_export() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("a.txt");
    fs::write(&source, "original").unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    edit(&mut a, "local");
    fs::rename(paths.workspaces(), paths.root.join("held")).unwrap();
    fs::write(paths.workspaces(), b"blocks workspace writes").unwrap();
    assert!(
        a.save_current_file()
            .unwrap_err()
            .contains("恢复副本保存失败")
    );
    assert_eq!(fs::read_to_string(source).unwrap(), "original");
    assert!(a.autosave().unwrap().dirty());
    let rescue = dir.path().join("rescue.txt");
    assert!(a.export_current_to(&rescue).unwrap().contains("仍未保存"));
    assert_eq!(fs::read_to_string(rescue).unwrap(), "local");
    assert!(a.autosave().unwrap().dirty());
    fs::remove_file(paths.workspaces()).unwrap();
    fs::rename(paths.root.join("held"), paths.workspaces()).unwrap();
    a.save_current_file().unwrap();
}
#[test]
fn save_shortcut_targets_active_region_even_when_find_field_has_focus() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("a.txt");
    fs::write(&source, "original").unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    let mut h = Harness::builder()
        .with_os(egui::os::OperatingSystem::Mac)
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, a: &mut TabloomApp| a.show(ui), a);
    h.event(egui::Event::Text(" EDIT".into()));
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::F);
    h.run();
    h.event(egui::Event::Text("query".into()));
    h.run();
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    assert_eq!(fs::read_to_string(source).unwrap(), "original EDIT");
    assert!(h.query_by_label(text::FIND_QUERY).is_some());
    assert!(
        h.ctx
            .memory(|m| m.has_focus(tabloom::ui::find_replace::query_id()))
    );
    let w = h.state().document.active().unwrap();
    assert!(!h.ctx.memory(|m| m.has_focus(editor_id(w.id))));
}

#[test]
fn save_failure_is_visible_in_ui_and_recoverable_text_stays_intact() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("a.txt");
    fs::write(&source, "original").unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    edit(&mut a, "local text");
    fs::write(&source, "external text").unwrap();
    let mut h = Harness::builder()
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .build_ui_state(|ui, a: &mut TabloomApp| a.show(ui), a);
    h.key_press_modifiers(Modifiers::COMMAND, Key::S);
    h.run();
    assert!(
        h.query_by_label(&file_io::FileError::Conflict.to_string())
            .is_some()
    );
    assert_eq!(
        h.state().document.active().unwrap().regions[0].text,
        "local text"
    );
    assert_eq!(fs::read_to_string(source).unwrap(), "external text");
    h.state_mut().flush().unwrap();
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
        "local text"
    );
}

#[test]
fn extracting_lines_keeps_the_entire_associated_file_on_save_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    let source = dir.path().join("source.txt");
    let original = "before\r\n中文😀\r\nafter";
    fs::write(&source, file_io::encode(original, TextEncoding::Utf16Le)).unwrap();
    let mut a = app(&paths);
    a.open_file_path(&source).unwrap();
    let selected = a
        .document
        .active_mut()
        .unwrap()
        .extract_lines(8..18, "区域")
        .unwrap();
    a.mark_changed();
    a.flush().unwrap();
    drop(a);
    let mut a = app(&paths);
    let w = a.document.active_mut().unwrap();
    assert_eq!(w.active_region, selected);
    assert_eq!(w.text(), original);
    assert_eq!(w.regions.len(), 3);
    w.apply_text_at("before\r\nchanged 中文😀\r\nafter", Some(8..8));
    a.mark_changed();
    a.save_current_file().unwrap();
    let (value, state) = file_io::open(&source).unwrap();
    assert_eq!(value, "before\r\nchanged 中文😀\r\nafter");
    assert_eq!(state.encoding, TextEncoding::Utf16Le);
    let copy = dir.path().join("selected.txt");
    a.export_current_to(&copy).unwrap();
    assert_eq!(fs::read_to_string(copy).unwrap(), "changed 中文😀\r\n");
    assert!(
        a.document
            .active()
            .unwrap()
            .regions
            .iter()
            .all(|r| r.file_state.as_ref() == Some(&state))
    );
}
