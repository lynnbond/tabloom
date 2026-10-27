use std::fs;
use tabloom::{
    file_io::{self, FileError},
    model::TextEncoding,
};
const TEXT: &str = "{\r\n\t\"中文😀\": true\n}\r";

#[test]
fn all_encodings_roundtrip_bom_unicode_and_mixed_line_endings() {
    for encoding in [
        TextEncoding::Utf8,
        TextEncoding::Utf8Bom,
        TextEncoding::Utf16Le,
        TextEncoding::Utf16Be,
    ] {
        let bytes = file_io::encode(TEXT, encoding);
        assert_eq!(file_io::decode(&bytes).unwrap(), (TEXT.into(), encoding));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("配置.json");
        fs::write(&path, &bytes).unwrap();
        let (text, state) = file_io::open(&path).unwrap();
        let updated = file_io::save(&path, &format!("{text}\t末尾"), &state).unwrap();
        assert_eq!(
            file_io::decode(&fs::read(&path).unwrap()).unwrap(),
            (format!("{TEXT}\t末尾"), encoding)
        );
        assert_eq!(updated.disk_hash, file_io::hash(&fs::read(&path).unwrap()));
    }
}
#[test]
fn malformed_and_binary_files_are_rejected_without_lossy_conversion() {
    for bytes in [
        &[0xff, 0xff][..],
        &[0xff, 0xfe, 0x00],
        &[0xff, 0xfe, 0, 0xd8],
        b"text\0binary",
        &[0xff, 0xfe, 0, 0, 0, 0, 0, 0],
        &[0, 0, 0xfe, 0xff],
    ] {
        assert!(file_io::decode(bytes).is_err(), "{bytes:?}");
    }
    assert_eq!(
        file_io::decode(b"").unwrap(),
        ("".into(), TextEncoding::Utf8)
    );
    assert_eq!(
        file_io::decode(&[0xef, 0xbb, 0xbf]).unwrap(),
        ("".into(), TextEncoding::Utf8Bom)
    );
}
#[test]
fn external_changes_or_deletion_never_get_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, "original").unwrap();
    let (_, state) = file_io::open(&path).unwrap();
    fs::write(&path, "external").unwrap();
    assert!(matches!(
        file_io::save(&path, "local", &state),
        Err(FileError::Conflict)
    ));
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    fs::remove_file(&path).unwrap();
    assert!(matches!(
        file_io::save(&path, "local", &state),
        Err(FileError::Conflict)
    ));
    assert!(!path.exists());
}
#[test]
fn export_is_exact_utf8_and_failures_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("copy.txt");
    file_io::export(&path, TEXT).unwrap();
    assert_eq!(fs::read(&path).unwrap(), TEXT.as_bytes());
    file_io::export(&path, "replacement").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "replacement");
    assert!(file_io::export(&dir.path().join("missing/copy.txt"), TEXT).is_err());
    assert!(file_io::export(dir.path(), TEXT).is_err());
    assert!(file_io::open(&dir.path().join("missing")).is_err());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
}
#[test]
fn syntax_suggestions_are_extension_only_and_unknown_files_remain_plain() {
    for (path, syntax) in [
        ("a.JSON", "json"),
        ("note.log", "plain"),
        ("foo.rs", "rust"),
        ("x.ps1", "powershell"),
        ("file", "plain"),
        ("script.sh", "shell"),
    ] {
        assert_eq!(
            file_io::suggested_syntax(std::path::Path::new(path)),
            syntax
        );
    }
}
#[test]
fn data_directory_and_symlink_aliases_are_protected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("data");
    fs::create_dir(&root).unwrap();
    assert!(matches!(
        file_io::guard_path(&root.join("session.json"), Some(&root)),
        Err(FileError::InternalData)
    ));
    assert!(file_io::guard_path(&dir.path().join("export.txt"), Some(&root)).is_ok());
    #[cfg(unix)]
    {
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&root, &alias).unwrap();
        assert!(matches!(
            file_io::guard_path(&alias.join("backup.txt"), Some(&root)),
            Err(FileError::InternalData)
        ));
    }
}
#[cfg(unix)]
#[test]
fn saving_preserves_permissions_and_writes_through_the_resolved_symlink_target() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("script.sh");
    fs::write(&path, TEXT).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o750)).unwrap();
    let link = dir.path().join("link");
    symlink(&path, &link).unwrap();
    let target = file_io::resolve(&link).unwrap();
    let (_, state) = file_io::open(&target).unwrap();
    file_io::save(&target, "updated", &state).unwrap();
    assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    assert_eq!(
        fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o750
    );
}
