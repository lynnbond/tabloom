//! macOS only: open real rfd dialogs and cancel through our own AppKit modal loop.
//! Does not use Accessibility privileges, modify files, or control another app.
#[cfg(target_os = "macos")]
fn main() {
    use dispatch2::DispatchQueue;
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    fn cancel_own_panel() {
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(800));
            DispatchQueue::main().exec_async(|| {
                let mtm = MainThreadMarker::new().expect("AppKit main queue");
                let app = NSApplication::sharedApplication(mtm);
                assert!(
                    app.modalWindow().is_some(),
                    "native file dialog must be visible"
                );
                app.abortModal();
            });
        });
    }
    let mtm = MainThreadMarker::new().expect("AppKit main thread");
    let _app = NSApplication::sharedApplication(mtm);
    cancel_own_panel();
    assert!(
        rfd::FileDialog::new()
            .set_title("Tabloom 打开文件验证（自动取消）")
            .pick_file()
            .is_none()
    );
    cancel_own_panel();
    assert!(
        rfd::FileDialog::new()
            .set_title("Tabloom 导出验证（自动取消）")
            .set_file_name("tabloom-dialog-test.txt")
            .save_file()
            .is_none()
    );
    println!(
        "NATIVE_FILE_DIALOG_OK: actual AppKit open/save panels presented and cancelled without writes"
    );
}
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This AppKit-specific probe requires macOS; test native pickers on your desktop.");
}
