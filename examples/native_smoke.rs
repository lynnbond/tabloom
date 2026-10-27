//! Runs a real native renderer with synthetic egui input (not OS keyboard input).
//! It verifies edits, tab switching and per-region undo, saves a screenshot and exits.
use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{
    app::TabloomApp,
    ui::{editor_region::editor_id, text},
};

const JSON: &str = "{\r\n\t\"标题\": \"Tabloom 中文 😀\",\r\n\t\"local\": true\r\n}";
const RUST: &str = "fn main() {\n\tprintln!(\"Hello, Tabloom!\");\n}\n";

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
            2 => raw.events.push(Event::Text(JSON.into())),
            3 => {
                let workspace = self.app.document.active_mut().unwrap();
                workspace.add_region(text::REGION);
                workspace.regions[1].syntax_id = "rust".into();
                workspace.regions[1].resize(220.0);
                ctx.memory_mut(|m| m.request_focus(editor_id(workspace.id)));
                raw.events.push(Event::Text(RUST.into()));
            }
            4 => key(raw, Modifiers::COMMAND, Key::N),
            5 => raw
                .events
                .push(Event::Text("独立工作区 B\nsecond workspace".into())),
            6 => key(
                raw,
                if cfg!(target_os = "macos") {
                    Modifiers::CTRL
                } else {
                    Modifiers::COMMAND
                },
                Key::Tab,
            ),
            7 => key(raw, Modifiers::COMMAND, Key::Z),
            8 => key(raw, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
            _ => {}
        }
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
            2 => assert_eq!(first.regions[0].text, JSON),
            3 => {
                assert_eq!(first.regions[0].text, JSON);
                assert_eq!(first.regions[1].text, RUST);
            }
            5 => {
                assert_eq!(self.app.document.workspaces.len(), 2);
                assert_eq!(
                    self.app.document.workspaces[1].regions[0].text,
                    "独立工作区 B\nsecond workspace"
                );
            }
            6 => assert_eq!(self.app.document.active_workspace, Some(first.id)),
            7 => {
                assert_eq!(first.regions[0].text, JSON);
                assert!(first.regions[1].text.is_empty());
            }
            8 => assert_eq!(first.regions[1].text, RUST),
            9 => {
                assert_eq!(first.regions[0].syntax_id, "json");
                assert_eq!(first.regions[1].syntax_id, "rust");
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
                "NATIVE_SMOKE_OK: edits, workspace isolation, independent syntax, undo/redo; screenshot={}",
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
        "Tabloom native smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::new(&cc.egui_ctx);
            let first = &mut app.document.active_mut().unwrap().regions[0];
            first.syntax_id = "json".into();
            first.resize(220.0);
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
