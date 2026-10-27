//! Runs a real native renderer with synthetic egui input (not OS keyboard input).
//! Run via scripts/verify-native-files.py. Uses only temporary files and databases.
//! The system picker itself is exercised separately; this probe calls path operations.
use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{app::TabloomApp, file_io, settings::AppPaths};

const ORIGINAL: &str = "{\r\n\t\"中文😀\": true\n}\r";
const EDIT: &str = "\n// 本地修改😀\r\n";

struct NativeSmoke {
    app: TabloomApp,
    root: PathBuf,
    read: bool,
    step: usize,
    started: Instant,
    screenshot: PathBuf,
    finished: bool,
    image_sender: mpsc::Sender<Arc<egui::ColorImage>>,
    image_receiver: mpsc::Receiver<Arc<egui::ColorImage>>,
}

fn key(raw: &mut egui::RawInput, modifiers: Modifiers, key: Key) {
    raw.events.push(Event::ModifiersChanged(modifiers));
    for pressed in [true, false] {
        raw.events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers,
        });
    }
    raw.events.push(Event::ModifiersChanged(Modifiers::NONE));
}

impl eframe::App for NativeSmoke {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        self.step += 1;
        if self.read {
            return;
        }
        match self.step {
            2 => raw.events.push(Event::Text(EDIT.into())),
            3 => {
                self.app.flush().unwrap();
                assert_eq!(
                    file_io::open(&self.root.join("source.json")).unwrap().0,
                    ORIGINAL
                );
            }
            4 => key(raw, Modifiers::COMMAND, Key::F),
            5 => raw.events.push(Event::Text("中文😀".into())),
            6 => key(raw, Modifiers::COMMAND, Key::S),
            7 => {
                self.app
                    .export_current_to(&self.root.join("copy.txt"))
                    .unwrap();
            }
            _ => {}
        }
        let _ = ctx;
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.finished {
            return;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "native smoke timed out"
        );
        self.app.show(ui);
        let expected = format!("{ORIGINAL}{EDIT}");
        if self.read && self.step == 2 {
            assert_eq!(
                self.app.document.active().unwrap().regions[0].text,
                expected
            );
            assert!(
                self.app
                    .save_current_file()
                    .unwrap_err()
                    .contains("外部修改")
            );
            assert_eq!(
                std::fs::read_to_string(self.root.join("source.json")).unwrap(),
                "external change"
            );
        }
        if !self.read && self.step == 7 {
            assert_eq!(
                file_io::open(&self.root.join("source.json")).unwrap().0,
                expected
            );
            assert_eq!(
                std::fs::read_to_string(self.root.join("copy.txt")).unwrap(),
                expected
            );
            assert_eq!(
                file_io::open(&self.root.join("source.json"))
                    .unwrap()
                    .1
                    .encoding,
                tabloom::model::TextEncoding::Utf16Le
            );
        }
        if (!self.read && self.step == 8) || (self.read && self.step == 2) {
            let sender = self.image_sender.clone();
            ui.ctx()
                .request_screenshot(move |image| sender.send(image).unwrap());
        }
        if let Ok(image) = self.image_receiver.try_recv() {
            if let Some(parent) = self.screenshot.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let rgba: Vec<_> = image
                .pixels
                .iter()
                .flat_map(|pixel| pixel.to_array())
                .collect();
            image::save_buffer(
                &self.screenshot,
                &rgba,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!(
                "NATIVE_FILE_SMOKE_OK: file import, recovery copy, explicit save/export, restart conflict; screenshot={}",
                self.screenshot.display()
            );
            self.finished = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(30));
    }
}

fn main() -> eframe::Result {
    let root = PathBuf::from(std::env::args_os().nth(1).expect("scratch directory"));
    let read = std::env::args().nth(2).as_deref() == Some("read");
    let screenshot = root.join(if read { "restored.png" } else { "files.png" });
    let (image_sender, image_receiver) = mpsc::channel();
    eframe::run_native(
        "Tabloom file smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::persistent(&cc.egui_ctx, Ok(AppPaths::at(root.join("data"))));
            if !read {
                app.open_file_path(&root.join("source.json")).unwrap();
            }
            Ok(Box::new(NativeSmoke {
                app,
                root,
                read,
                step: 0,
                started: Instant::now(),
                screenshot,
                finished: false,
                image_sender,
                image_receiver,
            }))
        }),
    )
}
