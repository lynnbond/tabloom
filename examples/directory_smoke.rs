//! Real native directory-switch fixture; all workspace/config files are disposable.
use eframe::egui::{self, ViewportCommand};
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
use tabloom::{
    app::TabloomApp,
    persistence::Store,
    settings::{self, AppPaths},
};
struct Probe {
    app: TabloomApp,
    target: PathBuf,
    config: PathBuf,
    step: usize,
    started: Instant,
    screenshot: PathBuf,
    requested: bool,
    tx: mpsc::Sender<Arc<egui::ColorImage>>,
    rx: mpsc::Receiver<Arc<egui::ColorImage>>,
}
impl eframe::App for Probe {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        assert!(self.started.elapsed() < Duration::from_secs(20));
        self.step += 1;
        self.app.show(ui);
        if self.step == 2 {
            self.app
                .switch_workspace_directory(ui.ctx(), &self.target)
                .unwrap();
            settings::write_directory_choice(&self.config, &self.target).unwrap();
            assert_eq!(
                settings::read_directory_choice(&self.config).unwrap(),
                Some(self.target.canonicalize().unwrap())
            );
        }
        if self.step >= 4 && !self.requested {
            self.requested = true;
            let tx = self.tx.clone();
            ui.ctx()
                .request_screenshot(move |image| tx.send(image).unwrap());
        }
        if let Ok(image) = self.rx.try_recv() {
            if let Some(p) = self.screenshot.parent() {
                std::fs::create_dir_all(p).unwrap();
            }
            let rgba: Vec<_> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
            image::save_buffer(
                &self.screenshot,
                &rgba,
                image.width() as u32,
                image.height() as u32,
                image::ColorType::Rgba8,
            )
            .unwrap();
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(30));
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}
fn main() -> eframe::Result {
    let temp = tempfile::tempdir().unwrap();
    let source = AppPaths::at(temp.path().join("source"));
    let target = temp.path().join("自选工作区 folder");
    let inspect = target.clone();
    let config = temp.path().join("config/location.json");
    let screenshot = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| "target/directory-smoke.png".into());
    let (tx, rx) = mpsc::channel();
    eframe::run_native(
        "Tabloom directory smoke",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100., 760.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::persistent(&cc.egui_ctx, Ok(source));
            app.document.active_mut().unwrap().regions[0].text =
                "选择自己的工作区目录\n中文 / Emoji 😀 / CRLF\r\n全部标签会保存到所选文件夹。"
                    .into();
            app.mark_changed();
            Ok(Box::new(Probe {
                app,
                target,
                config,
                step: 0,
                started: Instant::now(),
                screenshot,
                requested: false,
                tx,
                rx,
            }))
        }),
    )?;
    let s = Store::open(AppPaths::at(inspect))
        .unwrap()
        .load()
        .unwrap()
        .unwrap();
    assert!(
        s.document
            .active()
            .unwrap()
            .text()
            .contains("全部标签会保存")
    );
    println!(
        "DIRECTORY_NATIVE_PASS: move current snapshot, remember Unicode path, restart restore"
    );
    Ok(())
}
