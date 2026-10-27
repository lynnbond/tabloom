//! Short-lived native memory fixture. Uses a disposable data directory, never user data.
use eframe::egui;
use std::time::{Duration, Instant};
use tabloom::{app::TabloomApp, settings::AppPaths};
struct Probe {
    app: TabloomApp,
    started: Instant,
    frames: usize,
}
impl eframe::App for Probe {
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.app.show(ui);
        self.frames += 1;
        if self.frames == 10 {
            println!("MEMORY_READY");
        }
        if self.started.elapsed() > Duration::from_secs(12) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ui.ctx().request_repaint_after(Duration::from_millis(100));
    }
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        self.app.on_exit(gl);
    }
}
fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--colors") {
        let themes = syntect::highlighting::ThemeSet::load_defaults();
        for name in ["base16-ocean.dark", "InspiredGitHub"] {
            println!("{name}: {:?}", themes.themes[name].settings.foreground);
        }
        return Ok(());
    }
    let scratch = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(scratch.path().into());
    let syntax = std::env::args().nth(1).unwrap_or_else(|| "plain".into());
    eframe::run_native(
        "Tabloom memory probe",
        eframe::NativeOptions {
            renderer: eframe::Renderer::Glow,
            clipboard_shortcuts: false,
            viewport: egui::ViewportBuilder::default().with_inner_size([1100., 760.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = TabloomApp::persistent(&cc.egui_ctx, Ok(paths));
            if syntax != "plain" {
                let w = app.document.active_mut().unwrap();
                w.regions[0].syntax_id = syntax;
                w.regions[0].text = "fn main() { println!(\"Tabloom 中文\"); }".into();
                app.mark_changed();
            }
            Ok(Box::new(Probe {
                app,
                started: Instant::now(),
                frames: 0,
            }))
        }),
    )
}
