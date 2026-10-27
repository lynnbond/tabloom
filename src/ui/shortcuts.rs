use eframe::egui::{Key, Modifiers, os::OperatingSystem};

pub fn replace_modifiers(mac: bool) -> Modifiers {
    // Cmd+H is reserved by macOS to hide the application.
    if mac {
        Modifiers::COMMAND | Modifiers::SHIFT
    } else {
        Modifiers::COMMAND
    }
}

pub fn command_label(os: OperatingSystem, key: &str) -> String {
    format!(
        "{}{key}",
        if os == OperatingSystem::Mac {
            "⌘"
        } else {
            "Ctrl+"
        }
    )
}

pub fn replacement_label(os: OperatingSystem) -> &'static str {
    if os == OperatingSystem::Mac {
        "⌘⇧H"
    } else {
        "Ctrl+H"
    }
}

pub fn font_delta(key: Key, modifiers: Modifiers) -> Option<f32> {
    if !modifiers.command || modifiers.alt {
        return None;
    }
    match key {
        Key::Plus | Key::Equals => Some(1.0),
        Key::Minus if !modifiers.shift => Some(-1.0),
        _ => None,
    }
}
