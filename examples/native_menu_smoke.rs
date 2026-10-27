//! Real AppKit menu and accelerator exercise; uses only a temporary workspace.
#[cfg(target_os = "macos")]
mod mac {
    use eframe::egui::{self, Event};
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{
        NSApplication, NSEvent, NSEventModifierFlags, NSEventType, NSMenu, NSMenuItem,
    };
    use std::time::{Duration, Instant};
    use tabloom::{
        app::TabloomApp,
        persistence::Store,
        settings::{AppPaths, Appearance},
        ui::{find_replace, text},
    };

    fn root() -> Retained<NSMenu> {
        NSApplication::sharedApplication(MainThreadMarker::new().unwrap())
            .mainMenu()
            .unwrap()
    }
    fn item(path: &[&str]) -> (Retained<NSMenu>, isize, Retained<NSMenuItem>) {
        let mut menu = root();
        for (depth, title) in path.iter().enumerate() {
            let index = (0..menu.numberOfItems())
                .find(|&i| menu.itemAtIndex(i).unwrap().title().to_string() == *title)
                .unwrap_or_else(|| panic!("missing native menu: {path:?}"));
            let entry = menu.itemAtIndex(index).unwrap();
            if depth == path.len() - 1 {
                return (menu, index, entry);
            }
            menu = entry.submenu().unwrap();
        }
        unreachable!()
    }
    fn click(path: &[&str]) {
        let (menu, index, entry) = item(path);
        assert!(entry.isEnabled(), "disabled native item: {path:?}");
        menu.performActionForItemAtIndex(index);
    }
    fn accelerator(path: &[&str]) {
        let (_, _, entry) = item(path);
        let key = entry.keyEquivalent();
        let characters = if entry
            .keyEquivalentModifierMask()
            .contains(NSEventModifierFlags::Shift)
        {
            key.uppercaseString()
        } else {
            key.clone()
        };
        let event = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown, Default::default(), entry.keyEquivalentModifierMask(), 0., 0, None, &characters, &key, false, match key.to_string().to_lowercase().as_str() { "z" => 6, "n" => 45, "f" => 3, "q" => 12, _ => 0 },
        ).unwrap();
        assert!(
            root().performKeyEquivalent(&event),
            "accelerator was not consumed"
        );
    }

    struct Smoke {
        app: TabloomApp,
        step: usize,
        started: Instant,
        next_step: Instant,
        paths: AppPaths,
    }
    impl eframe::App for Smoke {
        fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
            // AppKit may dispatch a menu accelerator after its highlight animation.
            // Keep the main loop running instead of assuming delivery in the next frame.
            if Instant::now() < self.next_step {
                return;
            }
            self.next_step = Instant::now() + Duration::from_millis(250);
            self.step += 1;
            let value = || self.app.document.active().unwrap().text();
            match self.step {
                2 => raw.events.push(Event::Text("first 中文😀\r\n".into())),
                3 => {
                    assert_eq!(value(), "first 中文😀\r\n");
                    accelerator(&[text::FILE, text::NEW_WORKSPACE]);
                }
                4 => {
                    assert_eq!(self.app.document.workspaces.len(), 2);
                    raw.events.push(Event::Text("second\tSQL".into()));
                }
                5 => {
                    assert_eq!(value(), "second\tSQL");
                    click(&[text::FILE, text::CLOSE_WORKSPACE]);
                }
                6 => {
                    assert_eq!(value(), "first 中文😀\r\n");
                    click(&[text::FILE, text::RECENTLY_CLOSED, "临时工作区 2"]);
                }
                7 => {
                    assert_eq!(value(), "second\tSQL");
                    accelerator(&[text::EDIT, text::SELECT_ALL]);
                }
                8 => raw.events.push(Event::Text("replacement".into())),
                9 => {
                    assert_eq!(value(), "replacement");
                    accelerator(&[text::EDIT, text::UNDO]);
                }
                10 => {
                    assert_eq!(value(), "second\tSQL");
                    accelerator(&[text::EDIT, text::REDO]);
                }
                11 => {
                    assert_eq!(value(), "replacement");
                    click(&[text::EDIT, text::UNDO]);
                }
                12 => {
                    assert_eq!(value(), "second\tSQL");
                    accelerator(&[text::EDIT, text::FIND]);
                }
                13 => {
                    assert!(ctx.memory(|m| m.has_focus(find_replace::query_id())));
                    accelerator(&[text::EDIT, text::FIND_REPLACE]);
                    click(&[text::VIEW, text::THEME_OCEAN]);
                }
                14 => {
                    assert!(
                        egui::TextEdit::load_state(ctx, find_replace::replacement_id()).is_some()
                    );
                    assert!(
                        !NSApplication::sharedApplication(MainThreadMarker::new().unwrap())
                            .isHidden()
                    );
                    click(&[text::FILE, text::SAVE_FILE]);
                }
                16 => {
                    self.app.flush().unwrap();
                    std::fs::rename(
                        self.paths.workspaces(),
                        self.paths.root.join("held-workspaces"),
                    )
                    .unwrap();
                    std::fs::write(self.paths.workspaces(), b"write failure probe").unwrap();
                    self.app.mark_changed();
                    accelerator(&[text::APP_NAME, text::QUIT_APP]);
                }
                18 => {
                    assert!(self.app.flush().is_err());
                    assert!(!item(&[text::FILE, text::NEW_WORKSPACE]).2.isEnabled());
                    std::fs::remove_file(self.paths.workspaces()).unwrap();
                    std::fs::rename(
                        self.paths.root.join("held-workspaces"),
                        self.paths.workspaces(),
                    )
                    .unwrap();
                }
                20 => accelerator(&[text::APP_NAME, text::QUIT_APP]),
                _ => {}
            }
        }
        fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
            assert!(
                self.started.elapsed() < Duration::from_secs(20),
                "native menu exercise timed out"
            );
            self.app.show(ui);
            ui.ctx().request_repaint_after(Duration::from_millis(40));
        }
        fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
            self.app.flush().unwrap();
        }
    }

    pub fn run() -> eframe::Result {
        let scratch = tempfile::tempdir().unwrap();
        let paths = AppPaths::at(scratch.path().to_path_buf());
        let open_paths = paths.clone();
        let paths_for_app = paths.clone();
        eframe::run_native(
            "Tabloom native menu probe",
            eframe::NativeOptions {
                renderer: eframe::Renderer::Glow,
                clipboard_shortcuts: false,
                viewport: egui::ViewportBuilder::default().with_icon(egui::IconData::default()),
                ..Default::default()
            },
            Box::new(move |cc| {
                let mut app = TabloomApp::persistent(&cc.egui_ctx, Ok(open_paths));
                app.install_native_menu(&cc.egui_ctx);
                Ok(Box::new(Smoke {
                    app,
                    step: 0,
                    started: Instant::now(),
                    next_step: Instant::now(),
                    paths: paths_for_app,
                }))
            }),
        )?;
        let stored = Store::open(paths).unwrap().load().unwrap().unwrap();
        assert_eq!(stored.document.workspaces.len(), 2);
        assert_eq!(stored.document.active().unwrap().text(), "second\tSQL");
        assert_eq!(stored.document.workspaces[0].text(), "first 中文😀\r\n");
        assert_eq!(stored.preferences.appearance(), Appearance::Ocean);
        println!(
            "MACOS_NATIVE_MENU_PASS: AppKit menus, consumed accelerators, new/close/reopen, select/undo/redo/find, theme, save, failed-save quit cancellation, safe quit and disk restore"
        );
        Ok(())
    }
}
#[cfg(target_os = "macos")]
fn main() -> eframe::Result {
    mac::run()
}
#[cfg(not(target_os = "macos"))]
fn main() {
    println!("Native macOS menu probe requires macOS; other platforms use the in-window menus.");
}
