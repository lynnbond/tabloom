//! Runs a real native renderer with synthetic egui input (not OS keyboard input).
//! Verifies continuous document rendering, syntax spans, unchanged text and disk restore.
//! Uses a disposable standard application state; never touches the user workspace.
use eframe::egui::{self, Event, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{
    app::TabloomApp,
    editor::commands::{self, TextCommand},
    persistence::Store,
    settings::{AppPaths, Appearance},
    ui::editor_region::editor_id,
};

const SOURCE: &str = "# 开发笔记\n\n**Markdown 预览** · 双击编辑源码\n\n- 原始文本自动保存\n- 不改变换行与中文 😀\n\n{\"project\": \"Tabloom\", \"local\": true}\n\nSELECT id, name FROM projects WHERE active = true;\n\n接下来：保留整篇文档的普通编辑体验。\n";

struct NativeSmoke {
    app: TabloomApp,
    step: usize,
    started: Instant,
    screenshot: PathBuf,
    finished: bool,
    screenshot_requested: bool,
    image_sender: mpsc::Sender<Arc<egui::ColorImage>>,
    image_receiver: mpsc::Receiver<Arc<egui::ColorImage>>,
}

impl eframe::App for NativeSmoke {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.finished {
            return;
        }
        self.step += 1;
        match self.step {
            2 => raw.events.push(Event::Text(SOURCE.into())),
            3 | 4 => {
                let w = self.app.document.active_mut().unwrap();
                if self.step == 3 {
                    let md = w
                        .extract_lines(0..SOURCE.find('{').unwrap(), "区域")
                        .unwrap();
                    w.regions.iter_mut().find(|r| r.id == md).unwrap().syntax_id =
                        "markdown".into();
                }
                let start = SOURCE
                    .find(if self.step == 3 { "{" } else { "SELECT" })
                    .unwrap();
                let end = start + SOURCE[start..].find('\n').unwrap();
                let selected = w.extract_lines(start..end, "区域").unwrap();
                w.regions
                    .iter_mut()
                    .find(|r| r.id == selected)
                    .unwrap()
                    .syntax_id = if self.step == 3 { "json" } else { "sql" }.into();
                let id = editor_id(w.id);
                let mut value = w.text();
                commands::apply(ctx, id, &mut value, &TextCommand::SelectBytes(start..start));
                ctx.memory_mut(|m| m.request_focus(id));
                self.app.mark_changed();
            }
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
        assert_eq!(
            self.app.document.active().unwrap().text(),
            if self.step >= 2 { SOURCE } else { "" }
        );
        if self.step >= 5
            && !self.app.autosave().unwrap().dirty()
            && !self.finished
            && !self.screenshot_requested
        {
            self.screenshot_requested = true;
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
                "DOCUMENT_SMOKE_OK: Markdown preview, invisible syntax spans, unchanged source text, persisted; screenshot={}",
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
    let appearance = std::env::args().nth(2).map(|value| match value.as_str() {
        "A" => Appearance::Graphite,
        "B" => Appearance::Paper,
        "C" => Appearance::Ocean,
        "D" => Appearance::Sand,
        _ => panic!("theme must be A/B/C/D"),
    });
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().to_path_buf();
    let app_root = root.clone();
    let (image_sender, image_receiver) = mpsc::channel();
    eframe::run_native(
        "Tabloom document smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::persistent(&cc.egui_ctx, Ok(AppPaths::at(app_root)));
            if let Some(appearance) = appearance {
                app.document.create_workspace("临时工作区", "区域");
                app.set_appearance(&cc.egui_ctx, appearance);
            }
            Ok(Box::new(NativeSmoke {
                app,
                step: 0,
                started: Instant::now(),
                screenshot,
                finished: false,
                screenshot_requested: false,
                image_sender,
                image_receiver,
            }))
        }),
    )?;
    let db = Store::open(AppPaths::at(root)).unwrap();
    let loaded = db.load().unwrap().unwrap();
    if let Some(appearance) = appearance {
        assert_eq!(loaded.preferences.appearance(), appearance);
    }
    let w = loaded.document.active().unwrap();
    assert_eq!(w.text(), SOURCE);
    assert!(w.regions.iter().any(|r| r.syntax_id == "markdown"));
    assert!(w.regions.iter().any(|r| r.syntax_id == "json"));
    assert!(w.regions.iter().any(|r| r.syntax_id == "sql"));
    println!("DOCUMENT_RECOVERY_OK");
    Ok(())
}
