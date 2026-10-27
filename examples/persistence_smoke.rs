//! Isolated native persistence exercise. Always pass a scratch directory.
//! `write` exits normally; `crash` bypasses all Rust destructors after autosave;
//! `read` validates a previous process's complete snapshot and captures the UI.
use eframe::egui::{self, Event, Key, Modifiers, ViewportCommand};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{
    app::TabloomApp,
    persistence::Snapshot,
    settings::AppPaths,
    ui::{editor_region::editor_id, text},
};

struct Smoke {
    app: TabloomApp,
    mode: String,
    paths: AppPaths,
    step: usize,
    started: Instant,
    done: bool,
    screenshot_requested: bool,
    sender: mpsc::Sender<Arc<egui::ColorImage>>,
    receiver: mpsc::Receiver<Arc<egui::ColorImage>>,
}

fn key(input: &mut egui::RawInput, key: Key) {
    input
        .events
        .push(Event::ModifiersChanged(Modifiers::COMMAND));
    for pressed in [true, false] {
        input.events.push(Event::Key {
            key,
            pressed,
            repeat: false,
            modifiers: Modifiers::COMMAND,
            physical_key: None,
        });
    }
    input.events.push(Event::ModifiersChanged(Modifiers::NONE));
}

impl eframe::App for Smoke {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.step += 1;
        if self.mode == "read" || self.done {
            return;
        }
        match self.step {
            2 => input
                .events
                .push(Event::Text("{\r\n\t\"中文😀\": true\n}\r".into())),
            3 => {
                let workspace = self.app.document.active_mut().unwrap();
                workspace.regions[0].syntax_id = "json".into();
                workspace.regions[0].resize(220.0);
                workspace.add_region(text::REGION);
                workspace.regions[1].syntax_id = "sql".into();
                workspace.regions[1].resize(300.0);
                let document_id = workspace.id;
                self.app.mark_changed();
                ctx.memory_mut(|m| m.request_focus(editor_id(document_id)));
                input
                    .events
                    .push(Event::Text("SELECT 'local persistence';\n".into()));
            }
            4 | 7 => key(input, Key::N),
            5 => input
                .events
                .push(Event::Text("recently closed workspace\t中文".into())),
            6 => key(input, Key::W),
            8 => input
                .events
                .push(Event::Text("active workspace C\n恢复最后活动标签".into())),
            9 => {
                let w = &mut self.app.document.workspaces[0];
                w.move_region(w.regions[1].id, -1);
                self.app.mark_changed();
            }
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        assert!(
            self.started.elapsed() < Duration::from_secs(20),
            "native persistence smoke timed out"
        );
        self.app.show(ui);
        if self.done {
            return;
        }
        let autosave = self.app.autosave().expect("storage must open");
        assert!(
            autosave.error.is_none(),
            "save failed: {:?}",
            autosave.error
        );
        if self.mode != "read" && self.step > 10 && !autosave.dirty() {
            assert_eq!(self.app.document.workspaces.len(), 3);
            assert_eq!(self.app.document.workspaces[0].regions[0].syntax_id, "sql");
            assert_eq!(self.app.document.workspaces[0].regions[1].syntax_id, "json");
            let expected = self.paths.root.join("expected.json");
            fs::write(
                &expected,
                serde_json::to_vec_pretty(&self.app.snapshot()).unwrap(),
            )
            .unwrap();
            fs::File::open(&expected).unwrap().sync_all().unwrap();
            println!(
                "PERSISTENCE_AUTOSAVE_OK: {:?}, mode={}",
                self.started.elapsed(),
                self.mode
            );
            if self.mode == "crash" {
                assert!(self.paths.session().exists());
                // Abnormal exit skips the normal close hook; the manifest must already be durable.
                std::process::exit(23);
            }
            self.done = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        } else if self.mode == "read" && self.step > 5 && !self.screenshot_requested {
            self.screenshot_requested = true;
            let sender = self.sender.clone();
            ui.ctx()
                .request_screenshot(move |image| sender.send(image).unwrap());
        }
        if let Ok(image) = self.receiver.try_recv() {
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            let output = self.paths.root.join("restored.png");
            image::save_buffer(
                &output,
                &rgba,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            println!("PERSISTENCE_RESTORE_OK: {}", output.display());
            self.done = true;
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(30));
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}

fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let mode = args
        .next()
        .expect("usage: persistence_smoke write|crash|read SCRATCH_DIRECTORY");
    assert!(["write", "crash", "read"].contains(&mode.as_str()));
    let paths = AppPaths::at(PathBuf::from(
        args.next().expect("scratch directory is required"),
    ));
    if mode != "read" {
        assert!(
            !paths.session().exists(),
            "writer refuses to replace an existing database"
        );
    }
    let expected: Option<Snapshot> = if mode == "read" {
        Some(serde_json::from_slice(&fs::read(paths.root.join("expected.json")).unwrap()).unwrap())
    } else {
        None
    };
    let (sender, receiver) = mpsc::channel();
    eframe::run_native(
        "Tabloom persistence smoke",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        },
        Box::new(move |cc| {
            let app = TabloomApp::persistent(&cc.egui_ctx, Ok(paths.clone()));
            if let Some(expected) = expected {
                assert_eq!(
                    app.snapshot(),
                    expected,
                    "reopened process must restore all persisted fields"
                );
            }
            Ok(Box::new(Smoke {
                app,
                mode,
                paths,
                step: 0,
                started: Instant::now(),
                done: false,
                screenshot_requested: false,
                sender,
                receiver,
            }))
        }),
    )
}
