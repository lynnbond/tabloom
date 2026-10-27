use eframe::egui;
use tabloom::{
    app::TabloomApp,
    persistence::Store,
    settings::{self, AppPaths},
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
fn choosing_empty_directory_carries_all_tabs_and_restores_them() {
    let dir = tempfile::tempdir().unwrap();
    let source = AppPaths::at(dir.path().join("source"));
    let target = dir.path().join("我的工作区 folder");
    let mut a = app(&source);
    edit(&mut a, "中文😀\r\nfirst");
    a.document.create_workspace("tab", "region");
    edit(&mut a, "SELECT 1;\n");
    let before = a.snapshot();
    let ctx = egui::Context::default();
    a.switch_workspace_directory(&ctx, &target).unwrap();
    assert_eq!(a.snapshot(), before);
    assert_eq!(
        a.workspace_directory().unwrap(),
        target.canonicalize().unwrap()
    );
    drop(a);
    assert_eq!(app(&AppPaths::at(target)).snapshot(), before);
}
#[test]
fn opening_existing_directory_saves_current_and_does_not_overwrite_target() {
    let dir = tempfile::tempdir().unwrap();
    let source = AppPaths::at(dir.path().join("source"));
    let target = AppPaths::at(dir.path().join("other"));
    {
        let mut b = app(&target);
        edit(&mut b, "other directory text");
        b.flush().unwrap();
    }
    let mut a = app(&source);
    edit(&mut a, "unsaved source text");
    let before = a.snapshot();
    a.switch_workspace_directory(&egui::Context::default(), &target.root)
        .unwrap();
    assert_eq!(a.document.active().unwrap().text(), "other directory text");
    assert_eq!(app(&source).snapshot(), before);
    drop(a);
    assert_eq!(
        app(&target).document.active().unwrap().text(),
        "other directory text"
    );
}
#[test]
fn locked_or_invalid_target_keeps_current_document_and_saving_location() {
    let dir = tempfile::tempdir().unwrap();
    let source = AppPaths::at(dir.path().join("source"));
    let target = AppPaths::at(dir.path().join("busy"));
    let _busy = Store::open(target.clone()).unwrap();
    let mut a = app(&source);
    edit(&mut a, "retain unsaved text");
    let before = a.snapshot();
    let path = a.workspace_directory().unwrap().to_path_buf();
    assert!(
        a.switch_workspace_directory(&egui::Context::default(), &target.root)
            .is_err()
    );
    assert_eq!(a.snapshot(), before);
    assert_eq!(a.workspace_directory().unwrap(), path);
    let invalid = dir.path().join("not-directory");
    std::fs::write(&invalid, b"a file").unwrap();
    assert!(
        a.switch_workspace_directory(&egui::Context::default(), &invalid)
            .is_err()
    );
    assert_eq!(a.snapshot(), before);
}
#[test]
fn adopting_active_recovery_directory_releases_warning_without_touching_primary() {
    let dir = tempfile::tempdir().unwrap();
    let primary = AppPaths::at(dir.path().join("primary"));
    let _busy = Store::open(primary.clone()).unwrap();
    let mut a = app(&primary);
    edit(&mut a, "recovery work");
    let recovery = a.workspace_directory().unwrap().to_path_buf();
    a.switch_workspace_directory(&egui::Context::default(), &recovery)
        .unwrap();
    assert_eq!(a.document.active().unwrap().text(), "recovery work");
    assert!(!primary.session().exists());
    assert!(recovery.join("session.json").exists());
}
#[test]
fn directory_choice_is_atomic_unicode_safe_and_readable_on_restart() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config/location.json");
    let data = dir.path().join("用户目录 folder");
    std::fs::create_dir(&data).unwrap();
    assert!(settings::read_directory_choice(&config).unwrap().is_none());
    settings::write_directory_choice(&config, &data).unwrap();
    assert_eq!(
        settings::read_directory_choice(&config).unwrap(),
        Some(data.canonicalize().unwrap())
    );
    let original = std::fs::read(&config).unwrap();
    assert!(settings::write_directory_choice(&config, &dir.path().join("missing")).is_err());
    assert_eq!(std::fs::read(&config).unwrap(), original);
    std::fs::write(&config, br#"{"version":1,"directory":"relative"}"#).unwrap();
    assert!(settings::read_directory_choice(&config).is_err());
}
#[test]
fn memory_only_document_can_be_saved_to_an_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = TabloomApp::default();
    edit(&mut a, "memory text");
    let target = dir.path().join("saved");
    a.switch_workspace_directory(&egui::Context::default(), &target)
        .unwrap();
    a.flush().unwrap();
    drop(a);
    assert_eq!(
        app(&AppPaths::at(target)).document.active().unwrap().text(),
        "memory text"
    );
}

#[test]
fn failed_source_save_prevents_opening_other_data_but_allows_copy_to_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let source = AppPaths::at(dir.path().join("source"));
    let existing = AppPaths::at(dir.path().join("existing"));
    {
        let mut b = app(&existing);
        edit(&mut b, "existing contents");
        b.flush().unwrap();
    }
    let before_target = std::fs::read(existing.session()).unwrap();
    let mut a = app(&source);
    edit(&mut a, "source pending edits 中文😀");
    std::fs::rename(source.workspaces(), source.root.join("held")).unwrap();
    std::fs::write(source.workspaces(), b"blocks writes").unwrap();
    assert!(
        a.switch_workspace_directory(&egui::Context::default(), &existing.root)
            .is_err()
    );
    assert_eq!(
        a.document.active().unwrap().text(),
        "source pending edits 中文😀"
    );
    assert_eq!(std::fs::read(existing.session()).unwrap(), before_target);
    let rescue = dir.path().join("rescue");
    a.switch_workspace_directory(&egui::Context::default(), &rescue)
        .unwrap();
    drop(a);
    assert_eq!(
        app(&AppPaths::at(rescue)).document.active().unwrap().text(),
        "source pending edits 中文😀"
    );
}
