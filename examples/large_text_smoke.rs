//! Runs a real native renderer with synthetic egui input (not OS keyboard input).
//! 3 workspaces with 1 MiB each; optional second argument `rust` exercises syntax fallback.
//! UI timings cover app.show, not GPU paint. Uses memory only and no system clipboard.
use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::app::TabloomApp;

const LINE: &str = "developer scratch text 0123456789 abcdefghijklmnopqrstuvwxyz\tdata\r\n";

struct NativeSmoke {
    app: TabloomApp,
    source: String,
    ui_ms: Vec<f64>,
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
        let tab = if cfg!(target_os = "macos") {
            Modifiers::CTRL
        } else {
            Modifiers::COMMAND
        };
        match self.step {
            2 => raw.events.push(Event::Text("x".into())),
            3 | 5 => key(raw, tab, Key::Tab),
            4 => raw.events.push(Event::Text("y".into())),
            6 => raw.events.push(Event::Text("z".into())),
            7 => key(raw, tab | Modifiers::SHIFT, Key::Tab),
            8 => key(raw, Modifiers::COMMAND, Key::Z),
            9 => key(raw, Modifiers::COMMAND | Modifiers::SHIFT, Key::Z),
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
        let started = Instant::now();
        self.app.show(ui);
        self.ui_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let workspaces = &self.app.document.workspaces;
        match self.step {
            2 => assert_eq!(workspaces[0].regions[0].text, format!("{}x", self.source)),
            4 => assert_eq!(workspaces[1].regions[0].text, format!("{}y", self.source)),
            6 => assert_eq!(workspaces[2].regions[0].text, format!("{}z", self.source)),
            8 => assert_eq!(workspaces[1].regions[0].text, self.source),
            9 => assert_eq!(workspaces[1].regions[0].text, format!("{}y", self.source)),
            10 => {
                assert_eq!(workspaces[0].regions[0].text, format!("{}x", self.source));
                assert_eq!(workspaces[2].regions[0].text, format!("{}z", self.source));
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
                "NATIVE_LARGE_TEXT_OK: 3 workspaces x 3 regions, 1 MiB each, edits/switch/undo/redo; screenshot={}",
                self.screenshot.display()
            );
            println!("NATIVE_UI_BUILD_MS={:?}", self.ui_ms);
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
        "Tabloom 1 MiB smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::new(&cc.egui_ctx);
            let source = LINE.repeat((1024 * 1024) / LINE.len() + 1);
            let first = app.document.active_workspace.unwrap();
            for _ in 0..2 {
                app.document.create_workspace("性能工作区", "区域");
            }
            for w in &mut app.document.workspaces {
                w.regions[0].text = source.clone();
                w.regions[0].syntax_id = if std::env::args().nth(2).as_deref() == Some("rust") {
                    "rust"
                } else {
                    "plain"
                }
                .into();
                w.add_region("区域");
                w.regions[1].text = "SELECT '中文😀';\r\n".repeat(20);
                w.regions[1].syntax_id = "sql".into();
                w.add_region("区域");
                w.regions[2].text = "notes\t中文😀".into();
                w.active_region = w.regions[0].id;
            }
            app.document.select(first);
            Ok(Box::new(NativeSmoke {
                app,
                source,
                ui_ms: Vec::new(),
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
