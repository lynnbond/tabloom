//! A text-only native bridge. egui-winit's CRLF-normalizing shortcut translation
//! must be disabled with NativeOptions::clipboard_shortcuts = false.
use eframe::egui::{self, Event, Key, Modifiers};
use std::{ops::Range, sync::Mutex};

pub fn char_to_byte(text: &str, character: usize) -> usize {
    text.char_indices()
        .nth(character)
        .map_or(text.len(), |(i, _)| i)
}

pub fn selected_text(text: &str, characters: Range<usize>) -> Option<&str> {
    if characters.start > characters.end || characters.end > text.chars().count() {
        return None;
    }
    text.get(char_to_byte(text, characters.start)..char_to_byte(text, characters.end))
}

pub trait TextClipboard: Send + Sync + 'static {
    fn get_text(&self) -> Result<String, String>;
    fn set_text(&self, text: &str) -> Result<(), String>;
}

#[derive(Default)]
pub struct SystemClipboard(Mutex<Option<arboard::Clipboard>>);

impl SystemClipboard {
    fn access<T>(
        &self,
        f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>,
    ) -> Result<T, String> {
        let mut backend = self.0.lock().map_err(|e| e.to_string())?;
        if backend.is_none() {
            *backend = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        f(backend.as_mut().expect("initialized clipboard")).map_err(|e| e.to_string())
    }
}

impl TextClipboard for SystemClipboard {
    fn get_text(&self) -> Result<String, String> {
        self.access(|c| c.get_text())
    }
    fn set_text(&self, text: &str) -> Result<(), String> {
        self.access(|c| c.set_text(text))
    }
}

pub struct PlainTextClipboard {
    backend: Box<dyn TextClipboard>,
    pub error: Option<String>,
    skip_written_cut: Option<String>,
}

impl PlainTextClipboard {
    pub fn new(backend: impl TextClipboard) -> Self {
        Self {
            backend: Box::new(backend),
            error: None,
            skip_written_cut: None,
        }
    }
    fn read(&mut self) -> Result<String, String> {
        let result = self.backend.get_text();
        self.error = result.as_ref().err().cloned();
        result
    }
    fn write(&mut self, text: &str) -> Result<(), String> {
        let result = self.backend.set_text(text);
        self.error = result.as_ref().err().cloned();
        result
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ClipboardAction {
    Copy,
    Cut,
    Paste,
}

pub fn shortcut(key: Key, modifiers: Modifiers, mac: bool) -> Option<ClipboardAction> {
    let command = modifiers.command && !modifiers.alt;
    match key {
        Key::Copy => Some(ClipboardAction::Copy),
        Key::Cut => Some(ClipboardAction::Cut),
        Key::Paste => Some(ClipboardAction::Paste),
        Key::C if command && !modifiers.shift => Some(ClipboardAction::Copy),
        Key::X if command && !modifiers.shift => Some(ClipboardAction::Cut),
        Key::V if command => Some(ClipboardAction::Paste),
        Key::Insert if !mac && modifiers.ctrl && !modifiers.shift && !modifiers.alt => {
            Some(ClipboardAction::Copy)
        }
        Key::Insert if !mac && modifiers.shift && !modifiers.ctrl && !modifiers.alt => {
            Some(ClipboardAction::Paste)
        }
        Key::Delete if !mac && modifiers.shift && !modifiers.ctrl && !modifiers.alt => {
            Some(ClipboardAction::Cut)
        }
        _ => None,
    }
}

impl egui::Plugin for PlainTextClipboard {
    fn debug_name(&self) -> &'static str {
        "Tabloom plain text clipboard"
    }
    fn input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        let mut events = Vec::with_capacity(input.events.len());
        for event in std::mem::take(&mut input.events) {
            let action = match &event {
                Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } => shortcut(*key, *modifiers, ctx.os() == egui::os::OperatingSystem::Mac),
                _ => None,
            };
            match action {
                Some(ClipboardAction::Copy) => events.push(Event::Copy),
                Some(ClipboardAction::Cut) => events.push(Event::Cut),
                Some(ClipboardAction::Paste) => match self.read() {
                    Ok(text) => events.push(Event::Paste(text)),
                    Err(_) => ctx.request_repaint(), // The error is retained for the visible banner.
                },
                None => events.push(event),
            }
        }
        input.events = events;
    }
    fn output_hook(&mut self, ctx: &egui::Context, output: &mut egui::FullOutput) {
        output.platform_output.commands.retain(|command| {
            if let egui::OutputCommand::CopyText(text) = command {
                if self.skip_written_cut.as_deref() == Some(text) {
                    self.skip_written_cut = None;
                } else if self.write(text).is_err() {
                    ctx.request_repaint();
                }
                false
            } else {
                true
            }
        });
        self.skip_written_cut = None;
    }
}

pub fn install(ctx: &egui::Context) {
    ctx.add_plugin(PlainTextClipboard::new(SystemClipboard::default()));
}

pub fn read(ctx: &egui::Context) -> Result<String, String> {
    ctx.with_plugin::<PlainTextClipboard, _>(PlainTextClipboard::read)
        .unwrap_or_else(|| Err("native clipboard bridge is not installed".into()))
}

/// Copy first; a failed system write must never delete a cut selection.
pub fn guard_cut(ctx: &egui::Context, selected: &str) -> bool {
    if selected.is_empty() {
        return true;
    }
    ctx.with_plugin::<PlainTextClipboard, _>(|p| {
        if p.write(selected).is_err() {
            return false;
        }
        p.skip_written_cut = Some(selected.into());
        true
    })
    .unwrap_or(true)
}

pub fn error(ctx: &egui::Context) -> Option<String> {
    ctx.with_plugin::<PlainTextClipboard, _>(|p| p.error.clone())
        .flatten()
}
