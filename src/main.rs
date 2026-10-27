#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() -> eframe::Result {
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Tabloom")
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([480.0, 360.0]);
    // An explicit empty icon tells eframe to preserve the macOS bundle icon;
    // leaving it unset would install eframe's default icon in the Dock at runtime.
    #[cfg(target_os = "macos")]
    {
        viewport = viewport.with_icon(eframe::egui::IconData::default());
    }
    #[cfg(not(target_os = "macos"))]
    {
        viewport = viewport.with_icon(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon/Tabloom.png"))
                .expect("bundled Tabloom icon must be a valid PNG"),
        );
    }
    let options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        clipboard_shortcuts: false,
        ..Default::default()
    };
    eframe::run_native(
        "Tabloom",
        options,
        Box::new(|cc| {
            tabloom::editor::clipboard::install(&cc.egui_ctx);
            let app = tabloom::app::TabloomApp::persistent(
                &cc.egui_ctx,
                tabloom::settings::configured_paths(),
            );
            #[cfg(target_os = "macos")]
            let app = {
                let mut app = app;
                app.install_native_menu(&cc.egui_ctx);
                app
            };
            Ok(Box::new(app))
        }),
    )
}
