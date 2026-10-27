//! macOS menu ownership and event bridge. All AppKit calls stay on the UI thread.
use crate::{
    app::Action,
    editor::commands::TextCommand,
    model::{Document, WorkspaceStatus},
    settings::Appearance,
    ui::{text, theme},
};
use eframe::egui;
use muda::{
    AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
    accelerator::{Accelerator, Code, Key, KeyAccelerator, Modifiers},
};
use std::{collections::HashMap, sync::mpsc};

pub(crate) struct NativeMenu {
    root: Menu,
    receiver: mpsc::Receiver<MenuEvent>,
    bindings: HashMap<MenuId, Action>,
    items: HashMap<MenuId, MenuItem>,
    closed: Submenu,
    delete_closed: Submenu,
    focus: Submenu,
    themes: Vec<(Appearance, CheckMenuItem)>,
    signature: Vec<(String, String)>,
}

impl NativeMenu {
    pub fn new(ctx: &egui::Context) -> muda::Result<Self> {
        let root = Menu::new();
        let application = Submenu::new(text::APP_NAME, true);
        application.append(&PredefinedMenuItem::about(
            Some(text::ABOUT_APP),
            Some(AboutMetadata {
                name: Some(text::APP_NAME.into()),
                version: Some(env!("CARGO_PKG_VERSION").into()),
                ..Default::default()
            }),
        ))?;
        application.append(&PredefinedMenuItem::separator())?;
        application.append(&PredefinedMenuItem::hide(Some(text::HIDE_APP)))?;
        application.append(&PredefinedMenuItem::hide_others(Some(text::HIDE_OTHERS)))?;
        application.append(&PredefinedMenuItem::show_all(Some(text::SHOW_ALL)))?;
        application.append(&PredefinedMenuItem::separator())?;
        let file = Submenu::new(text::FILE, true);
        let edit = Submenu::new(text::EDIT, true);
        let view = Submenu::new(text::VIEW, true);
        let window = Submenu::new(text::WINDOW, true);
        let closed = Submenu::new(text::RECENTLY_CLOSED, false);
        let delete_closed = Submenu::new(text::PERMANENT_DELETE, false);
        let focus = Submenu::new(text::SPAN_TOOLS, true);
        let (sender, receiver) = mpsc::channel();
        let mut menu = Self {
            root,
            receiver,
            bindings: HashMap::new(),
            items: HashMap::new(),
            closed,
            delete_closed,
            focus,
            themes: Vec::new(),
            signature: Vec::new(),
        };
        menu.add(
            &application,
            "quit",
            text::QUIT_APP,
            Some(Code::KeyQ),
            Action::Quit,
        )?;
        for (id, label, key, action) in [
            (
                "new",
                text::NEW_WORKSPACE,
                Some(Code::KeyN),
                Action::NewWorkspace,
            ),
            (
                "close",
                text::CLOSE_WORKSPACE,
                Some(Code::KeyW),
                Action::Close(uuid::Uuid::nil()),
            ),
            ("open", text::OPEN_FILE, Some(Code::KeyO), Action::OpenFile),
            ("save", text::SAVE_FILE, Some(Code::KeyS), Action::SaveFile),
            ("export", text::EXPORT_FILE, None, Action::ExportFile),
        ] {
            menu.add(&file, id, label, key, action)?;
        }
        file.append(&PredefinedMenuItem::separator())?;
        menu.add(
            &file,
            "choose",
            text::CHOOSE_DIRECTORY,
            None,
            Action::ChooseDirectory,
        )?;
        menu.add(
            &file,
            "reveal",
            text::REVEAL_DIRECTORY,
            None,
            Action::RevealDirectory,
        )?;
        file.append(&PredefinedMenuItem::separator())?;
        file.append(&menu.closed)?;
        file.append(&menu.delete_closed)?;
        for (id, label, key, action) in [
            (
                "undo",
                text::UNDO,
                Some(Code::KeyZ),
                Action::Edit(TextCommand::Undo),
            ),
            ("redo", text::REDO, None, Action::Edit(TextCommand::Redo)),
            (
                "cut",
                text::CUT,
                Some(Code::KeyX),
                Action::Edit(TextCommand::Cut),
            ),
            (
                "copy",
                text::COPY,
                Some(Code::KeyC),
                Action::Edit(TextCommand::Copy),
            ),
            ("paste", text::PASTE, Some(Code::KeyV), Action::Paste),
            (
                "all",
                text::SELECT_ALL,
                Some(Code::KeyA),
                Action::Edit(TextCommand::SelectAll),
            ),
            ("find", text::FIND, Some(Code::KeyF), Action::Find(false)),
            ("replace", text::FIND_REPLACE, None, Action::Find(true)),
            (
                "extract",
                text::EXTRACT_SELECTION,
                None,
                Action::ExtractSelection,
            ),
        ] {
            menu.add(&edit, id, label, key, action)?;
        }
        // AppKit distinguishes shifted character equivalents by uppercase glyphs.
        menu.items[&MenuId::from("redo")].set_key_accelerator(Some(KeyAccelerator::new(
            Modifiers::META | Modifiers::SHIFT,
            Key::Character("Z".into()),
        )))?;
        menu.items[&MenuId::from("replace")].set_key_accelerator(Some(KeyAccelerator::new(
            Modifiers::META | Modifiers::SHIFT,
            Key::Character("H".into()),
        )))?;
        edit.append(&menu.focus)?;
        menu.add(
            &view,
            "larger",
            text::FONT_INCREASE,
            Some(Code::Equal),
            Action::Font(1.),
        )?;
        menu.add(
            &view,
            "smaller",
            text::FONT_DECREASE,
            Some(Code::Minus),
            Action::Font(-1.),
        )?;
        view.append(&PredefinedMenuItem::separator())?;
        for (index, appearance) in Appearance::ALL.into_iter().enumerate() {
            let id = MenuId::from(format!("theme:{index}"));
            let item =
                CheckMenuItem::with_id(id.clone(), theme::label(appearance), true, false, None);
            view.append(&item)?;
            menu.bindings.insert(id, Action::Appearance(appearance));
            menu.themes.push((appearance, item));
        }
        window.append(&PredefinedMenuItem::minimize(Some(text::MINIMIZE)))?;
        window.append(&PredefinedMenuItem::fullscreen(Some(text::FULLSCREEN)))?;
        menu.root
            .append_items(&[&application, &file, &edit, &view, &window])?;
        // Custom quit goes through Tabloom's existing save/error/close gate.
        // Custom edit items keep copy/paste plain text and target the egui editor.
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if sender.send(event).is_ok() {
                ctx.request_repaint();
            }
        }));
        menu.root.init_for_nsapp();
        window.set_as_windows_menu_for_nsapp();
        Ok(menu)
    }

    fn add(
        &mut self,
        parent: &Submenu,
        id: &str,
        label: &str,
        key: Option<Code>,
        action: Action,
    ) -> muda::Result<()> {
        let id = MenuId::from(id);
        let item = MenuItem::with_id(
            id.clone(),
            label,
            true,
            key.map(|key| Accelerator::new(Modifiers::META, key)),
        );
        parent.append(&item)?;
        self.bindings.insert(id.clone(), action);
        self.items.insert(id, item);
        Ok(())
    }

    pub fn sync(
        &mut self,
        document: &Document,
        appearance: Appearance,
        can_extract: bool,
        editable: bool,
        modal: bool,
    ) -> muda::Result<()> {
        for (id, item) in &self.items {
            let enabled = if modal {
                id == &MenuId::from("quit")
            } else {
                match id.as_ref() {
                    "undo" | "redo" | "cut" | "copy" | "paste" | "all" => editable,
                    "extract" => can_extract,
                    "close" | "save" | "export" | "delete-span" => document.active().is_some(),
                    _ => true,
                }
            };
            if item.is_enabled() != enabled {
                item.set_enabled(enabled);
            }
        }
        for (appearance_id, item) in &self.themes {
            if item.is_checked() != (*appearance_id == appearance) {
                item.set_checked(*appearance_id == appearance);
            }
            if item.is_enabled() == modal {
                item.set_enabled(!modal);
            }
        }
        self.closed.set_enabled(
            !modal
                && document
                    .workspaces
                    .iter()
                    .any(|w| w.status == WorkspaceStatus::Closed),
        );
        self.delete_closed.set_enabled(
            !modal
                && document
                    .workspaces
                    .iter()
                    .any(|w| w.status == WorkspaceStatus::Closed),
        );
        self.focus
            .set_enabled(!modal && document.active().is_some());
        if let Some(id) = document.active_workspace {
            self.bindings
                .insert(MenuId::from("close"), Action::Close(id));
        }
        let mut signature = Vec::new();
        let mut closed: Vec<_> = document
            .workspaces
            .iter()
            .filter(|w| w.status == WorkspaceStatus::Closed)
            .collect();
        closed.sort_by_key(|w| std::cmp::Reverse(w.closed_at));
        for w in closed {
            signature.push((format!("closed:{}", w.id), w.title.clone()));
        }
        if let Some(w) = document.active() {
            signature.push((format!("active:{}", w.id), String::new()));
            for r in &w.regions {
                signature.push((format!("focus:{}", r.id), r.title.clone()));
            }
        }
        if self.signature != signature {
            for parent in [&self.closed, &self.delete_closed, &self.focus] {
                for index in (0..parent.items().len()).rev() {
                    parent.remove_at(index).expect("existing menu child");
                }
            }
            self.bindings
                .retain(|id, _| !id.as_ref().starts_with("dynamic:"));
            self.items
                .retain(|id, _| !id.as_ref().starts_with("dynamic:"));
            for (key, title) in &signature {
                if let Some(id) = key.strip_prefix("closed:") {
                    let id = uuid::Uuid::parse_str(id).expect("model UUID");
                    self.add(
                        &self.closed.clone(),
                        &format!("dynamic:reopen:{id}"),
                        title,
                        None,
                        Action::Reopen(id),
                    )?;
                    self.add(
                        &self.delete_closed.clone(),
                        &format!("dynamic:delete:{id}"),
                        title,
                        None,
                        Action::DeleteClosed(id),
                    )?;
                } else if let Some(id) = key.strip_prefix("focus:") {
                    let id = uuid::Uuid::parse_str(id).expect("model UUID");
                    self.add(
                        &self.focus.clone(),
                        &format!("dynamic:focus:{id}"),
                        title,
                        None,
                        Action::FocusSpan(id),
                    )?;
                }
            }
            self.add(
                &self.focus.clone(),
                "dynamic:add-span",
                text::NEW_REGION,
                None,
                Action::AddRegion,
            )?;
            self.add(
                &self.focus.clone(),
                "dynamic:delete-span",
                text::DELETE,
                None,
                Action::DeleteSpan,
            )?;
            self.signature = signature;
        }
        Ok(())
    }

    pub fn actions(&self) -> Vec<Action> {
        self.receiver
            .try_iter()
            .filter_map(|event| self.bindings.get(&event.id).cloned())
            .collect()
    }
}

impl Drop for NativeMenu {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
        self.root.remove_for_nsapp();
    }
}
