use eframe::egui;
use egui_kittest::{Harness, kittest::Queryable};
use tabloom::{
    app::TabloomApp,
    editor::{
        commands::{self, TextCommand},
        highlighting::Highlighter,
    },
    settings::{AppPaths, Appearance, Preferences},
    ui::{editor_region::editor_id, text, theme},
};

#[test]
fn legacy_preferences_choose_equivalent_style_and_new_choices_roundtrip() {
    for dark in [true, false] {
        let old = serde_json::json!({"dark":dark,"font_size":15.,"window_size":[1100.,760.]});
        let mut preferences: Preferences = serde_json::from_value(old).unwrap();
        assert_eq!(
            preferences.appearance(),
            if dark {
                Appearance::Graphite
            } else {
                Appearance::Paper
            }
        );
        for appearance in Appearance::ALL {
            preferences.set_appearance(appearance);
            assert_eq!(preferences.dark, appearance.dark());
            let restored: Preferences =
                serde_json::from_str(&serde_json::to_string(&preferences).unwrap()).unwrap();
            assert_eq!(restored, preferences);
        }
    }
}
#[test]
fn each_style_persists_without_changing_text_languages_or_file_state() {
    let dir = tempfile::tempdir().unwrap();
    let paths = AppPaths::at(dir.path().join("data"));
    for appearance in Appearance::ALL {
        let ctx = egui::Context::default();
        let mut app = TabloomApp::persistent(&ctx, Ok(paths.clone()));
        if app.document.active().unwrap().text().is_empty() {
            let w = app.document.active_mut().unwrap();
            w.regions[0].text = "笔记😀\r\nSELECT 1;\n".into();
            w.extract_lines("笔记😀\r\n".len()..w.text().len(), "区域")
                .unwrap();
            w.regions[1].syntax_id = "sql".into();
        }
        let before = app.document.clone();
        app.set_appearance(&ctx, appearance);
        app.flush().unwrap();
        drop(app);
        let app = TabloomApp::persistent(&egui::Context::default(), Ok(paths.clone()));
        assert_eq!(app.snapshot().preferences.appearance(), appearance);
        assert_eq!(app.document, before);
    }
}
#[test]
fn menu_switches_all_styles_and_preserves_selection_and_undo() {
    let mut h = Harness::builder()
        .with_size((1100., 760.))
        .with_os(egui::os::OperatingSystem::from_target_os())
        .allow_missing_glyphs()
        .build_ui_state(
            |ui, app: &mut TabloomApp| app.show(ui),
            TabloomApp::default(),
        );
    let value = "first 中文😀\r\nSELECT 1;\n";
    h.event(egui::Event::Text(value.into()));
    h.run();
    let w = h.state().document.active().unwrap();
    let id = editor_id(w.id);
    let mut buffer = w.text();
    commands::apply(&h.ctx, id, &mut buffer, &TextCommand::SelectBytes(0..5));
    h.run();
    for appearance in Appearance::ALL {
        h.get_by_label(text::VIEW).click();
        h.run();
        h.get_by_label(theme::label(appearance)).click();
        h.run();
        assert_eq!(h.state().snapshot().preferences.appearance(), appearance);
        assert_eq!(theme::current(&h.ctx), appearance);
        assert_eq!(h.ctx.global_style().visuals.dark_mode, appearance.dark());
        assert_eq!(h.state().document.active().unwrap().text(), value);
        assert_eq!(commands::selected_bytes(&h.ctx, id, value), 0..5);
    }
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::Z);
    h.run();
    assert_eq!(h.state().document.active().unwrap().text(), "");
    h.key_press_modifiers(
        egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
        egui::Key::Z,
    );
    h.run();
    assert_eq!(h.state().document.active().unwrap().text(), value);
}
#[test]
fn styled_highlighting_keeps_raw_text_and_has_distinct_palettes() {
    let h = Highlighter::default();
    let source = "{\r\n\t\"中文😀\":true,\n\"n\":123}\r";
    let mut colors = Vec::new();
    for appearance in Appearance::ALL {
        let p = theme::palette(appearance);
        let job = h
            .layout_job_styled(source, "json", 15., appearance.dark(), p.syntax)
            .unwrap();
        assert_eq!(job.text, source);
        assert!(
            job.sections
                .iter()
                .map(|s| s.format.color)
                .collect::<std::collections::HashSet<_>>()
                .len()
                > 1
        );
        let plain = h
            .layout_job_styled(source, "plain", 15., appearance.dark(), p.syntax)
            .unwrap();
        assert_eq!(plain.sections[0].format.color, p.text);
        colors.push(plain.sections[0].format.color);
    }
    assert_eq!(
        colors
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        4
    );
}
