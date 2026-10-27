//! Reproducible release-mode smoke measurements, not a statistically rigorous benchmark.
use eframe::egui::{self, Key, Modifiers};
use egui_kittest::Harness;
use serde_json::json;
use std::time::Instant;
use tabloom::{
    app::TabloomApp,
    editor::{
        highlighting::Highlighter,
        search::{SearchOptions, find_matches},
    },
    persistence::Store,
    settings::AppPaths,
};

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
fn main() {
    let line = "developer scratch text 0123456789 abcdefghijklmnopqrstuvwxyz\tdata\r\n";
    let text = line.repeat((1024 * 1024) / line.len() + 1);
    let highlighter = Highlighter::default();
    let t = Instant::now();
    let job = highlighter.layout_job(&text, "plain", 15., true).unwrap();
    let plain = ms(t);
    assert_eq!(job.text, text);
    let t = Instant::now();
    let job = highlighter.layout_job(&text, "rust", 15., true).unwrap();
    let rust = ms(t);
    assert_eq!(job.text, text);
    let mut app = TabloomApp::default();
    for _ in 0..2 {
        app.document.create_workspace("bench", "region");
    }
    for w in &mut app.document.workspaces {
        w.regions[0].text = text.clone();
        w.add_region("region");
        w.regions[1].text = "SELECT '中文😀';\n".repeat(50);
        w.regions[1].syntax_id = "sql".into();
        w.add_region("region");
        w.regions[2].text = "notes 中文😀\t\r\n".repeat(100);
        w.active_region = w.regions[0].id;
    }
    let first = app.document.workspaces[0].id;
    app.document.select(first);
    let dir = tempfile::tempdir().unwrap();
    let mut db = Store::open(AppPaths::at(dir.path().to_path_buf())).unwrap();
    let t = Instant::now();
    db.save(&app.document, &app.snapshot().preferences).unwrap();
    let save = ms(t);
    let t = Instant::now();
    let loaded = db.load().unwrap().unwrap();
    let load = ms(t);
    assert_eq!(loaded.document, app.document);
    let t = Instant::now();
    let matches = find_matches(&text, "scratch", SearchOptions::default()).unwrap();
    let search = ms(t);
    assert_eq!(matches.len(), text.lines().count());
    let t = Instant::now();
    let mut h = Harness::builder()
        .with_size((1100., 900.))
        .allow_missing_glyphs()
        .with_os(egui::os::OperatingSystem::from_target_os())
        .build_ui_state(|ui, a: &mut TabloomApp| a.show(ui), app);
    let initial = ms(t);
    let mut edits = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        h.event(egui::Event::Text("x".into()));
        h.run();
        edits.push(ms(t));
    }
    assert!(
        h.state().document.active().unwrap().regions[0]
            .text
            .ends_with("xxx")
    );
    let t = Instant::now();
    h.key_press_modifiers(
        if cfg!(target_os = "macos") {
            Modifiers::CTRL
        } else {
            Modifiers::COMMAND
        },
        Key::Tab,
    );
    h.run();
    let switch = ms(t);
    assert_ne!(h.state().document.active_workspace, Some(first));
    println!("{}",serde_json::to_string_pretty(&json!({"bytes_per_large_region":text.len(),"workspaces":3,"regions_per_workspace":3,"plain_layout_job_ms":plain,"rust_layout_job_ms":rust,"storage_save_ms":save,"storage_load_ms":load,"literal_search_ms":search,"headless_initial_ui_ms":initial,"headless_edit_runs_ms":edits,"headless_switch_run_ms":switch,"note":"UI timings include kittest settling passes, no native paint or system fonts; run --release"})).unwrap());
}
