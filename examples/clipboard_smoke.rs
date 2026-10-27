//! Runs a real native renderer with synthetic egui input (not OS keyboard input).
//! Run via scripts/verify-native-clipboard.swift to seed rich clipboard alternatives.
//! Verifies real system plain-text operations, undo and find, then saves a screenshot.
use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{app::TabloomApp, editor::clipboard};

const FIXTURE: &str = "{\r\n\t\"中文😀\": \"<b>literal</b>\",\n\t\"ok\": true\r\n}\r";

struct NativeSmoke {
    app: TabloomApp,
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
        match self.step {
            2 => key(raw, Modifiers::COMMAND, Key::V),
            3 | 10 => key(raw, Modifiers::COMMAND, Key::A),
            4 | 11 => key(raw, Modifiers::COMMAND, Key::C),
            6 => key(raw, Modifiers::COMMAND, Key::X),
            7 => key(raw, Modifiers::COMMAND, Key::Z),
            8 => key(raw, Modifiers::COMMAND, Key::N),
            9 => key(raw, Modifiers::COMMAND, Key::V),
            12 => key(raw, Modifiers::COMMAND, Key::F),
            13 => raw.events.push(Event::Text("中文😀".into())),
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
        let first = &self.app.document.workspaces[0];
        match self.step {
            2 | 7 => assert_eq!(first.regions[0].text, FIXTURE),
            5 => assert_eq!(clipboard::read(ui.ctx()).unwrap(), FIXTURE),
            6 => assert!(first.regions[0].text.is_empty()),
            9 => {
                assert_eq!(first.regions[0].text, FIXTURE);
                assert_eq!(self.app.document.workspaces[1].regions[0].text, FIXTURE);
            }
            14 => {
                assert_eq!(clipboard::read(ui.ctx()).unwrap(), FIXTURE);
                let sender = self.image_sender.clone();
                ui.ctx()
                    .request_screenshot(move |image| sender.send(image).unwrap());
            }
            _ => {}
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
                "CLIPBOARD_SMOKE_OK: raw copy/cut/paste, Unicode, CRLF, independent tabs, undo, native find panel; screenshot={}",
                self.screenshot.display()
            );
            self.finished = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(30));
    }
}

fn main() -> eframe::Result {
    let screenshot = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target/native-smoke.png"));
    let (image_sender, image_receiver) = mpsc::channel();
    eframe::run_native(
        "Tabloom clipboard smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            clipboard::install(&cc.egui_ctx);
            let app = TabloomApp::new(&cc.egui_ctx);
            Ok(Box::new(NativeSmoke {
                app,
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
