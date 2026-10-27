use std::{fs, time::Duration};
use tabloom::{
    model::{Document, WorkspaceStatus},
    persistence::{Persistence, Snapshot, StorageError, Store, autosave::Autosave},
    settings::{AppPaths, Preferences},
};
fn paths(dir: &tempfile::TempDir) -> AppPaths {
    AppPaths::at(dir.path().join("data"))
}
fn sample() -> (Document, Preferences) {
    let mut d = Document::default();
    d.create_workspace("工作区", "区域");
    let w = d.active_mut().unwrap();
    w.regions[0].text = "{\r\n\t\"中文😀\":true\n}\r".into();
    w.regions[0].syntax_id = "json".into();
    let sql = w.add_region("区域");
    w.regions[1].text = "SELECT ' 中文 ';\n".into();
    w.regions[1].syntax_id = "sql".into();
    w.regions[1].resize(435.5);
    w.move_region(sql, -1);
    d.create_workspace("工作区", "区域");
    d.active_mut().unwrap().regions[0].text = "independent\t第二页".into();
    (
        d,
        Preferences {
            dark: false,
            appearance: None,
            font_size: 19.,
            window_size: [950., 710.],
        },
    )
}
#[test]
fn initialize_save_destroy_and_restore_complete_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, preferences) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        assert!(s.load().unwrap().is_none());
        s.save(&d, &preferences).unwrap();
        s.maintain().unwrap();
    }
    let s = Store::open(p.clone()).unwrap();
    assert_eq!(
        s.load().unwrap().unwrap(),
        Snapshot {
            document: d,
            preferences
        }
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(p.session()).unwrap()).unwrap()["version"],
        1
    );
}
#[test]
fn close_reopen_and_permanent_delete_do_not_damage_other_tabs() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (mut d, prefs) = sample();
    let first = d.workspaces[0].id;
    let other = d.workspaces[1].clone();
    d.close(first);
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
    }
    let mut s = Store::open(p).unwrap();
    let mut d = s.load().unwrap().unwrap().document;
    assert_eq!(d.workspaces[0].status, WorkspaceStatus::Closed);
    assert!(d.reopen(first));
    assert_eq!(d.active_workspace, Some(first));
    assert!(!d.delete_closed(first));
    d.close(first);
    assert!(d.delete_closed(first));
    s.save(&d, &prefs).unwrap();
    s.maintain().unwrap();
    assert_eq!(s.load().unwrap().unwrap().document.workspaces, vec![other]);
}
#[test]
fn failed_commit_preserves_previous_manifest_and_dirty_revision() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (mut d, prefs) = sample();
    let (mut persistence, _) = Persistence::open(p.clone()).unwrap();
    persistence.save(&d, &prefs, Duration::ZERO).unwrap();
    let before = persistence.store.load().unwrap().unwrap();
    // Fail final manifest replacement after new workspace files have been written.
    fs::rename(p.session(), p.root.join("held-session.json")).unwrap();
    fs::create_dir(p.session()).unwrap();
    d.workspaces[0].regions[0].text = "new unsaved text".into();
    persistence.autosave.changed(Duration::ZERO);
    assert!(persistence.save(&d, &prefs, Duration::ZERO).is_err());
    assert!(persistence.autosave.dirty());
    assert!(persistence.autosave.error.is_some());
    fs::remove_dir(p.session()).unwrap();
    fs::rename(p.root.join("held-session.json"), p.session()).unwrap();
    assert_eq!(persistence.store.load().unwrap().unwrap(), before);
    persistence
        .save(&d, &prefs, Duration::from_secs(3))
        .unwrap();
    assert!(!persistence.autosave.dirty());
    assert_eq!(persistence.store.load().unwrap().unwrap().document, d);
}
#[test]
fn second_instance_rejected_and_released_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let s = Store::open(p.clone()).unwrap();
    assert!(matches!(
        Store::open(p.clone()),
        Err(StorageError::AlreadyRunning(_))
    ));
    drop(s);
    assert!(Store::open(p).is_ok());
}
#[test]
fn unknown_version_and_io_errors_do_not_trigger_destructive_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    fs::create_dir_all(&p.root).unwrap();
    let b = br#"{"version":99,"precious":"text"}"#;
    fs::write(p.session(), b).unwrap();
    assert!(matches!(
        Store::open(p.clone()),
        Err(StorageError::UnsupportedVersion(99))
    ));
    assert_eq!(fs::read(p.session()).unwrap(), b);
    assert!(!p.recovery().exists());
    fs::remove_file(p.session()).unwrap();
    fs::create_dir(p.session()).unwrap();
    assert!(Store::open(p.clone()).is_err());
    assert!(!p.recovery().exists());
}
#[test]
fn backups_are_independent_rotate_and_survive_revision_cleanup() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (mut d, prefs) = sample();
    let mut s = Store::open(p.clone()).unwrap();
    for n in 0..5 {
        d.workspaces[0].regions[0].text = format!("snapshot {n} 中文\r\n");
        s.save(&d, &prefs).unwrap();
        s.maintain().unwrap();
        let b = s.backup(true).unwrap().unwrap();
        let v: serde_json::Value = serde_json::from_slice(&fs::read(b).unwrap()).unwrap();
        let snapshot: Snapshot = serde_json::from_value(v["snapshot"].clone()).unwrap();
        assert_eq!(snapshot.document, d);
    }
    assert_eq!(fs::read_dir(p.backups()).unwrap().count(), 3);
    assert_eq!(
        fs::read_dir(p.workspaces()).unwrap().count(),
        d.workspaces.len()
    );
    assert!(s.backup(false).unwrap().is_none());
}
#[test]
fn corrupt_main_recovers_backup_and_archives_original() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
        s.backup(true).unwrap();
    }
    fs::write(p.session(), b"precious damaged main").unwrap();
    let s = Store::open(p.clone()).unwrap();
    assert_eq!(s.load().unwrap().unwrap().document, d);
    assert_eq!(
        fs::read(s.recovery_archive.unwrap().join("session.json")).unwrap(),
        b"precious damaged main"
    );
}
#[test]
fn missing_workspace_recovers_complete_backup() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
        s.backup(true).unwrap();
    }
    let entry = fs::read_dir(p.workspaces())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    fs::remove_file(entry.path()).unwrap();
    let s = Store::open(p).unwrap();
    assert_eq!(s.load().unwrap().unwrap().document, d);
    assert!(s.recovery_archive.is_some());
}
#[test]
fn checksum_damage_is_repaired_from_backup_with_original_archived() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
        s.backup(true).unwrap();
    }
    let entry = fs::read_dir(p.workspaces())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    fs::write(entry.path(), b"damaged span").unwrap();
    let s = Store::open(p).unwrap();
    assert_eq!(s.load().unwrap().unwrap().document, d);
    assert_eq!(
        fs::read(
            s.recovery_archive
                .unwrap()
                .join("workspaces")
                .join(entry.file_name())
        )
        .unwrap(),
        b"damaged span"
    );
}
#[test]
fn corrupt_newest_backup_skipped_and_missing_manifest_restored() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
        s.backup(true).unwrap();
    }
    let bad = p.backups().join("backup-99999999999999999999-invalid.json");
    fs::write(&bad, b"broken backup").unwrap();
    fs::remove_file(p.session()).unwrap();
    let s = Store::open(p).unwrap();
    assert_eq!(s.load().unwrap().unwrap().document, d);
    assert_eq!(fs::read(bad).unwrap(), b"broken backup");
}
#[test]
fn corruption_without_backup_is_preserved_and_never_reset() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    fs::create_dir_all(&p.root).unwrap();
    fs::write(p.session(), b"valuable broken state").unwrap();
    assert!(matches!(
        Store::open(p.clone()),
        Err(StorageError::NoBackup { .. })
    ));
    assert_eq!(fs::read(p.session()).unwrap(), b"valuable broken state");
}
#[test]
fn orphan_revision_is_not_treated_as_a_new_install() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    {
        let mut s = Store::open(p.clone()).unwrap();
        s.save(&d, &prefs).unwrap();
    }
    fs::remove_file(p.session()).unwrap();
    assert!(matches!(
        Store::open(p.clone()),
        Err(StorageError::NoBackup { .. })
    ));
    assert!(!p.session().exists());
}
#[test]
fn invalid_model_does_not_replace_state_or_write_associated_file() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (mut d, prefs) = sample();
    let external = dir.path().join("original.txt");
    fs::write(&external, b"original\r\n").unwrap();
    d.workspaces[0].regions[0].file_path = Some(external.clone());
    let mut s = Store::open(p).unwrap();
    s.save(&d, &prefs).unwrap();
    d.workspaces[0].regions[0].text = "local only".into();
    s.save(&d, &prefs).unwrap();
    assert_eq!(fs::read(external).unwrap(), b"original\r\n");
    let before = s.load().unwrap().unwrap();
    d.workspaces[0].active_region = uuid::Uuid::new_v4();
    assert!(s.save(&d, &prefs).is_err());
    assert_eq!(s.load().unwrap().unwrap(), before);
}
#[test]
fn backup_error_does_not_misreport_committed_content_and_retry_works() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (d, prefs) = sample();
    let (mut s, _) = Persistence::open(p.clone()).unwrap();
    s.save(&d, &prefs, Duration::ZERO).unwrap();
    fs::rename(p.backups(), p.root.join("held")).unwrap();
    fs::write(p.backups(), b"block backups").unwrap();
    s.backup_error = Some(s.store.backup(true).unwrap_err().to_string());
    s.save(&d, &prefs, Duration::from_secs(1)).unwrap();
    assert!(s.backup_error.is_some());
    assert!(!s.autosave.dirty());
    fs::remove_file(p.backups()).unwrap();
    fs::rename(p.root.join("held"), p.backups()).unwrap();
    s.finish(&d, &prefs, Duration::from_secs(2)).unwrap();
    assert!(s.backup_error.is_none());
}
#[test]
fn debounce_max_wait_and_retry_are_deterministic() {
    let mut autosave = Autosave::default();
    autosave.changed(Duration::ZERO);
    assert!(!autosave.due(Duration::from_millis(399)));
    autosave.changed(Duration::from_millis(300));
    assert_eq!(autosave.deadline(), Some(Duration::from_millis(700)));
    for millis in [600, 900, 1200, 1500, 1800] {
        autosave.changed(Duration::from_millis(millis));
    }
    assert!(!autosave.due(Duration::from_millis(1999)));
    assert!(autosave.due(Duration::from_secs(2)));
    let old = autosave.revision();
    autosave.changed(Duration::from_millis(2100));
    autosave.saved(old);
    assert!(
        autosave.dirty(),
        "a stale completion must not clear a newer edit"
    );
    autosave.failed(Duration::from_secs(3), "disk full".into());
    autosave.changed(Duration::from_millis(3100));
    assert_eq!(autosave.deadline(), Some(Duration::from_secs(6)));
    assert!(!autosave.due(Duration::from_secs(5)));
    assert!(autosave.due(Duration::from_secs(6)));
    autosave.saved(autosave.revision());
    assert!(!autosave.dirty());
    assert!(autosave.error.is_none());
    assert!(autosave.deadline().is_none());
}

#[test]
fn unchanged_workspace_files_are_reused_and_unsafe_references_are_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    let (mut d, prefs) = sample();
    let mut store = Store::open(p.clone()).unwrap();
    store.save(&d, &prefs).unwrap();
    let before: serde_json::Value =
        serde_json::from_slice(&fs::read(p.session()).unwrap()).unwrap();
    d.workspaces[0].regions[0].text.push_str("changed");
    store.save(&d, &prefs).unwrap();
    let after: serde_json::Value = serde_json::from_slice(&fs::read(p.session()).unwrap()).unwrap();
    assert_ne!(before["workspaces"][0], after["workspaces"][0]);
    assert_eq!(before["workspaces"][1], after["workspaces"][1]);
    let mut unsafe_manifest = after;
    unsafe_manifest["workspaces"][0] = "../private.json".into();
    fs::write(p.session(), serde_json::to_vec(&unsafe_manifest).unwrap()).unwrap();
    assert!(matches!(store.load(), Err(StorageError::Corrupt(_))));
}
#[test]
fn unrelated_files_do_not_block_json_initialization_and_are_not_modified() {
    let dir = tempfile::tempdir().unwrap();
    let p = paths(&dir);
    fs::create_dir_all(p.backups()).unwrap();
    for file in [
        "unrelated.bin",
        "old-export.json",
        "backups/unrelated.backup",
    ] {
        fs::write(p.root.join(file), b"unrelated original bytes").unwrap();
    }
    let mut store = Store::open(p.clone()).unwrap();
    assert!(store.load().unwrap().is_none());
    let (document, prefs) = sample();
    store.save(&document, &prefs).unwrap();
    assert_eq!(store.load().unwrap().unwrap().document, document);
    for file in [
        "unrelated.bin",
        "old-export.json",
        "backups/unrelated.backup",
    ] {
        assert_eq!(
            fs::read(p.root.join(file)).unwrap(),
            b"unrelated original bytes"
        );
    }
}
