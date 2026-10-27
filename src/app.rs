use crate::{
    editor::{
        clipboard::{self, PlainTextClipboard},
        commands::{self, TextCommand},
        highlighting::Highlighter,
        search::SearchState,
    },
    file_io,
    model::{Document, WorkspaceStatus},
    persistence::{Persistence, Snapshot, StorageError, autosave::Autosave},
    settings::{AppPaths, Appearance, Preferences},
    ui::{
        editor_region::{self, DocumentState, DocumentView},
        find_replace::{self, FindAction},
        shortcuts, text, theme,
    },
};
use eframe::egui::{self, Color32, Id, Key, Modifiers};
use std::time::Instant;
use uuid::Uuid;

#[derive(Clone)]
pub(crate) enum Action {
    #[cfg(target_os = "macos")]
    Quit,
    ToggleDocumentSource,
    OpenFile,
    SaveFile,
    ExportFile,
    ChooseDirectory,
    RevealDirectory,
    UseRecoveryDirectory,
    NewWorkspace,
    Select(Uuid),
    Close(Uuid),
    Reopen(Uuid),
    DeleteClosed(Uuid),
    AddRegion,
    Find(bool),
    Edit(TextCommand),
    Paste,
    Font(f32),
    Appearance(Appearance),
    ExtractSelection,
    FocusSpan(Uuid),
    DeleteSpan,
}

pub struct TabloomApp {
    pub document: Document,
    highlighter: Highlighter,
    cache: DocumentState,
    preferences: Preferences,
    persistence: Option<Persistence>,
    storage_paths: Option<AppPaths>,
    startup_error: Option<String>,
    recovery_notice: Option<String>,
    rescue_error: Option<String>,
    pending_permanent_delete: Option<Uuid>,
    started: Instant,
    geometry_grace: usize,
    pending_focus: Option<Uuid>,
    pending_delete: Option<(Uuid, Uuid)>,
    confirm_exit: bool,
    allow_exit: bool,
    search: SearchState,
    pending_text_command: Option<(Id, TextCommand)>,
    pending_find_action: Option<FindAction>,
    last_text_focus: Option<Id>,
    edit_error: Option<String>,
    file_notice: Option<(bool, String)>,
    applied_appearance: Option<Appearance>,
    #[cfg(target_os = "macos")]
    native_menu: Option<crate::ui::native_menu::NativeMenu>,
}

impl Default for TabloomApp {
    fn default() -> Self {
        let mut document = Document::default();
        document.create_workspace(text::WORKSPACE, text::REGION);
        let pending_focus = document.active().map(|w| w.active_region);
        Self {
            document,
            highlighter: Highlighter::default(),
            cache: DocumentState::default(),
            preferences: Preferences::default(),
            persistence: None,
            storage_paths: None,
            startup_error: None,
            recovery_notice: None,
            rescue_error: None,
            pending_permanent_delete: None,
            started: Instant::now(),
            geometry_grace: 2,
            pending_focus,
            pending_delete: None,
            confirm_exit: false,
            allow_exit: false,
            search: SearchState::default(),
            pending_text_command: None,
            pending_find_action: None,
            last_text_focus: None,
            edit_error: None,
            file_notice: None,
            applied_appearance: None,
            #[cfg(target_os = "macos")]
            native_menu: None,
        }
    }
}

impl TabloomApp {
    #[cfg(target_os = "macos")]
    pub fn install_native_menu(&mut self, ctx: &egui::Context) {
        if self.native_menu.is_some() {
            return;
        }
        match crate::ui::native_menu::NativeMenu::new(ctx) {
            Ok(menu) => self.native_menu = Some(menu),
            Err(error) => {
                self.file_notice = Some((true, format!("{}{error}", text::NATIVE_MENU_ERROR)))
            }
        }
    }

    /// Isolated in-memory constructor for tests/examples. Production uses persistent().
    pub fn new(ctx: &egui::Context) -> Self {
        theme::apply(ctx, Appearance::Graphite);
        Self::default()
    }

    pub fn persistent(ctx: &egui::Context, paths: crate::persistence::Result<AppPaths>) -> Self {
        let mut app = Self::new(ctx);
        app.storage_paths = paths.as_ref().ok().cloned();
        let opened = match paths.and_then(Persistence::open) {
            Ok(opened) => Ok(opened),
            Err(error) => {
                app.startup_error = Some(error.to_string());
                Persistence::open_recovery(app.storage_paths.as_ref())
            }
        };
        match opened {
            Ok((mut persistence, snapshot)) => {
                let initial = snapshot.is_none();
                if let Some(snapshot) = snapshot {
                    app.document = snapshot.document;
                    app.preferences = snapshot.preferences;
                }
                let no_open_workspace = app.document.active().is_none();
                if no_open_workspace {
                    app.document.create_workspace(text::WORKSPACE, text::REGION);
                }
                theme::apply(ctx, app.preferences.appearance());
                app.applied_appearance = Some(app.preferences.appearance());
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(
                    app.preferences.window_size.into(),
                ));
                app.focus_active();
                app.recovery_notice = persistence
                    .store
                    .recovery_archive
                    .as_ref()
                    .map(|path| format!("{}{}", text::RECOVERY_NOTICE, path.display()));
                if initial || no_open_workspace {
                    persistence.autosave.changed(app.started.elapsed());
                    // Errors are recorded in the scheduler and displayed without blocking editing.
                    if let Err(error) =
                        persistence.save(&app.document, &app.preferences, app.started.elapsed())
                    {
                        eprintln!("Tabloom initial save: {error}");
                    }
                } else {
                    persistence.backup_error =
                        persistence.store.backup(false).err().map(|e| e.to_string());
                }
                app.persistence = Some(persistence);
            }
            Err(error) => {
                let original = app.startup_error.take().unwrap_or_default();
                app.startup_error = Some(format!("{original}\n{error}"));
                app.focus_active();
            }
        }
        app
    }

    pub fn workspace_directory(&self) -> Option<&std::path::Path> {
        self.persistence
            .as_ref()
            .map(|p| p.store.paths().root.as_path())
    }

    /// Select an empty directory to carry the current snapshot, or open an existing
    /// one after saving the current snapshot. A failed target never replaces the editor.
    pub fn switch_workspace_directory(
        &mut self,
        ctx: &egui::Context,
        directory: &std::path::Path,
    ) -> Result<String, String> {
        std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        if self
            .workspace_directory()
            .and_then(|p| p.canonicalize().ok())
            .as_ref()
            == Some(&directory)
        {
            self.flush().map_err(|e| e.to_string())?;
            self.storage_paths = Some(AppPaths::at(directory.clone()));
            self.startup_error = None;
            self.focus_active();
            return Ok(format!("{}{}", text::DIRECTORY_SAVED, directory.display()));
        }
        let (mut target, snapshot) =
            Persistence::open(AppPaths::at(directory.clone())).map_err(|e| e.to_string())?;
        let replacing = snapshot.is_some();
        if replacing {
            if self.persistence.is_some() {
                self.flush()
                    .map_err(|e| format!("{} {e}", text::DIRECTORY_UNSAVED))?;
            } else if self
                .document
                .workspaces
                .iter()
                .flat_map(|w| &w.regions)
                .any(|r| !r.text.is_empty())
            {
                return Err(text::DIRECTORY_UNSAVED.into());
            }
        }
        let snapshot = snapshot.unwrap_or_else(|| self.snapshot());
        target
            .save(
                &snapshot.document,
                &snapshot.preferences,
                self.started.elapsed(),
            )
            .map_err(|e| e.to_string())?;
        let old_directory = self
            .workspace_directory()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.persistence = Some(target);
        self.storage_paths = Some(AppPaths::at(directory.clone()));
        self.startup_error = None;
        self.recovery_notice = None;
        self.rescue_error = None;
        if replacing {
            self.document = snapshot.document;
            self.preferences = snapshot.preferences;
            self.cache = DocumentState::default();
            self.search = SearchState::default();
            self.pending_text_command = None;
            self.last_text_focus = None;
            self.pending_delete = None;
            self.pending_permanent_delete = None;
            for w in &self.document.workspaces {
                egui::text_edit::TextEditState::default()
                    .store(ctx, editor_region::editor_id(w.id));
            }
            if self.document.active().is_none() {
                self.document
                    .create_workspace(text::WORKSPACE, text::REGION);
                self.mark_changed();
            }
            theme::apply(ctx, self.preferences.appearance());
            self.applied_appearance = Some(self.preferences.appearance());
            self.focus_active();
        }
        if !replacing {
            self.focus_active();
        }
        Ok(if replacing {
            format!("{}{old_directory}", text::DIRECTORY_OPENED)
        } else {
            format!("{}{}", text::DIRECTORY_SAVED, directory.display())
        })
    }

    fn choose_workspace_directory(&mut self, ctx: &egui::Context, use_current: bool) {
        // Native pickers are modal; commit pending typing before entering their loop.
        // A failed source save still permits selecting an empty rescue destination.
        if !use_current
            && self.persistence.is_some()
            && let Err(error) = self.flush()
        {
            self.file_notice = Some((true, format!("{} {error}", text::DIRECTORY_UNSAVED)));
        }
        let selected = if use_current {
            self.workspace_directory().map(std::path::Path::to_path_buf)
        } else {
            let mut picker = rfd::FileDialog::new().set_title(text::DIRECTORY_PICKER);
            if let Some(root) = self.workspace_directory() {
                picker = picker.set_directory(root);
            }
            picker.pick_folder()
        };
        let Some(directory) = selected else {
            return;
        };
        self.file_notice = Some(match self.switch_workspace_directory(ctx, &directory) {
            Ok(message) => {
                let remembered = crate::settings::directory_choice_file()
                    .ok_or_else(|| std::io::Error::other(text::NO_DIRECTORY))
                    .and_then(|file| crate::settings::write_directory_choice(&file, &directory));
                match remembered {
                    Ok(()) => (false, message),
                    Err(e) => (true, format!("{} {e}", text::DIRECTORY_REMEMBER_FAILED)),
                }
            }
            Err(error) => (true, error),
        });
    }

    fn reveal_workspace_directory(&mut self) {
        let result = self
            .workspace_directory()
            .ok_or_else(|| text::NO_DIRECTORY.to_owned())
            .and_then(|path| {
                let mut command = std::process::Command::new(if cfg!(target_os = "macos") {
                    "open"
                } else if cfg!(target_os = "windows") {
                    "explorer"
                } else {
                    "xdg-open"
                });
                command
                    .arg(path)
                    .spawn()
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            });
        if let Err(error) = result {
            self.file_notice = Some((true, error));
        }
    }

    pub fn set_appearance(&mut self, ctx: &egui::Context, appearance: Appearance) {
        self.preferences.set_appearance(appearance);
        theme::apply(ctx, appearance);
        self.applied_appearance = Some(appearance);
        if let Some(id) = self.last_text_focus.or_else(|| {
            self.document
                .active()
                .map(|w| editor_region::editor_id(w.id))
        }) && let Some(value) = self.input_text(id)
        {
            let selected = commands::selected_bytes(ctx, id, &value);
            self.pending_text_command = Some((id, TextCommand::SelectBytes(selected)));
        }
        self.mark_changed();
    }

    fn active_region(&self) -> Option<&crate::model::EditorRegion> {
        self.document
            .active()
            .and_then(|w| w.regions.iter().find(|r| r.id == w.active_region))
    }

    fn protect_file_path(&self, path: &std::path::Path) -> file_io::Result<std::path::PathBuf> {
        let path =
            file_io::guard_path(path, self.storage_paths.as_ref().map(|p| p.root.as_path()))?;
        file_io::guard_path(
            &path,
            self.persistence
                .as_ref()
                .map(|p| p.store.paths().root.as_path()),
        )
    }

    /// Open into a separate workspace, retaining a local recovery copy immediately.
    pub fn open_file_path(&mut self, path: &std::path::Path) -> Result<(), String> {
        let path = self.protect_file_path(path).map_err(|e| e.to_string())?;
        let (text, state) = file_io::open(&path).map_err(|e| e.to_string())?;
        self.document
            .create_workspace(text::WORKSPACE, text::REGION);
        let workspace = self.document.active_mut().expect("created workspace");
        let title = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        workspace.title = title.clone();
        let region = &mut workspace.regions[0];
        region.title = title;
        region.text = text;
        region.syntax_id = file_io::suggested_syntax(&path).into();
        region.file_path = Some(path);
        region.file_state = Some(state);
        region.modified = false;
        self.focus_active();
        self.mark_changed();
        if self.persistence.is_some() {
            self.flush()
                .map_err(|e| format!("{}：{e}", text::OPEN_RECOVERY_ERROR))?;
        }
        Ok(())
    }

    fn flush_before_file_write(&mut self) -> Result<(), String> {
        if self.persistence.is_some() {
            self.flush()
                .map_err(|e| format!("{}：{e}", text::BEFORE_FILE_WRITE_ERROR))?;
        }
        Ok(())
    }

    /// Ctrl/Cmd+S never opens a Save As dialog for an unnamed region.
    pub fn save_current_file(&mut self) -> Result<String, String> {
        self.flush_before_file_write()?;
        let Some(region) = self.active_region() else {
            return Ok(String::new());
        };
        let Some(path) = &region.file_path else {
            return Ok(if self.persistence.is_some() {
                text::TEMP_SAVED
            } else {
                text::MEMORY_STATUS
            }
            .into());
        };
        let path = self.protect_file_path(path).map_err(|e| e.to_string())?;
        let state = region
            .file_state
            .as_ref()
            .ok_or_else(|| file_io::FileError::MissingBaseline.to_string())?;
        let value = self
            .document
            .active()
            .unwrap()
            .regions
            .iter()
            .filter(|r| r.file_path.as_ref() == Some(&path))
            .map(|r| r.text.as_str())
            .collect::<String>();
        let state = file_io::save(&path, &value, state).map_err(|e| e.to_string())?;
        let workspace = self.document.active_mut().expect("active workspace");
        for region in &mut workspace.regions {
            if region.file_path.as_ref() == Some(&path) {
                region.file_state = Some(state.clone());
                region.modified = false;
            }
        }
        workspace.touch();
        self.mark_changed();
        if self.persistence.is_some() {
            self.flush()
                .map_err(|e| format!("{}：{e}", text::AFTER_FILE_WRITE_ERROR))?;
        }
        Ok(format!("{}{}", text::FILE_SAVED, path.display()))
    }

    /// Export is a UTF-8 copy; it never rebinds a temporary region to a file.
    pub fn export_current_to(&mut self, path: &std::path::Path) -> Result<String, String> {
        let path = self.protect_file_path(path).map_err(|e| e.to_string())?;
        if self
            .active_region()
            .and_then(|r| r.file_path.as_ref())
            .is_some_and(|p| *p == path)
        {
            return self.save_current_file();
        }
        // Export to another path remains an escape hatch when the recovery disk is full.
        let recovery_error = if self.persistence.is_some() {
            self.flush().err().map(|e| e.to_string())
        } else {
            None
        };
        let Some(region) = self.active_region() else {
            return Ok(String::new());
        };
        file_io::export(&path, &region.text).map_err(|e| e.to_string())?;
        Ok(match recovery_error {
            Some(error) => format!(
                "{}{}；{}{error}",
                text::FILE_EXPORTED,
                path.display(),
                text::EXPORT_RECOVERY_ERROR
            ),
            None => format!("{}{}", text::FILE_EXPORTED, path.display()),
        })
    }

    fn file_action(&mut self, action: Action) {
        let result = match action {
            Action::SaveFile => self.save_current_file(),
            Action::OpenFile => {
                // Preserve pending edits before a modal system dialog pauses painting.
                if let Err(e) = self.flush_before_file_write() {
                    self.file_notice = Some((true, e));
                    return;
                }
                let Some(path) = rfd::FileDialog::new()
                    .set_title(text::OPEN_FILE)
                    .pick_file()
                else {
                    return;
                };
                self.open_file_path(&path)
                    .map(|()| format!("{}{}", text::FILE_OPENED, path.display()))
            }
            Action::ExportFile => {
                if self.persistence.is_some()
                    && let Err(error) = self.flush()
                {
                    // The autosave banner retains this error; still allow rescue export.
                    eprintln!("Tabloom before export: {error}");
                }
                let name = self
                    .active_region()
                    .and_then(|r| r.file_path.as_ref())
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "untitled.txt".into());
                let Some(path) = rfd::FileDialog::new()
                    .set_title(text::EXPORT_FILE)
                    .set_file_name(name)
                    .save_file()
                else {
                    return;
                };
                self.export_current_to(&path)
            }
            _ => unreachable!(),
        };
        self.file_notice = Some(match result {
            Ok(message) => (false, message),
            Err(error) => (true, error),
        });
    }

    pub fn mark_changed(&mut self) {
        if let Some(persistence) = &mut self.persistence {
            persistence.autosave.changed(self.started.elapsed());
        }
    }

    pub fn autosave(&self) -> Option<&Autosave> {
        self.persistence.as_ref().map(|p| &p.autosave)
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            document: self.document.clone(),
            preferences: self.preferences.clone(),
        }
    }

    pub fn flush(&mut self) -> crate::persistence::Result<()> {
        let persistence = self
            .persistence
            .as_mut()
            .ok_or(StorageError::NoDataDirectory)?;
        persistence.save(&self.document, &self.preferences, self.started.elapsed())
    }

    fn finish(&mut self) -> crate::persistence::Result<()> {
        let persistence = self
            .persistence
            .as_mut()
            .ok_or(StorageError::NoDataDirectory)?;
        persistence.finish(&self.document, &self.preferences, self.started.elapsed())
    }

    fn tick(&mut self, ctx: &egui::Context) {
        if self.allow_exit {
            return;
        }
        let now = self.started.elapsed();
        if self
            .persistence
            .as_ref()
            .is_some_and(|p| p.autosave.due(now))
        {
            if let Err(error) = self.flush() {
                eprintln!("Tabloom autosave: {error}");
            } else if self.confirm_exit && self.finish().is_ok() {
                self.allow_exit = true;
                self.confirm_exit = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint();
        }
        if let Some(deadline) = self
            .persistence
            .as_ref()
            .and_then(|p| p.autosave.deadline())
        {
            ctx.request_repaint_after(deadline.saturating_sub(self.started.elapsed()));
        }
    }

    fn copy_rescue(&mut self, ctx: &egui::Context) {
        match serde_json::to_string_pretty(&self.snapshot()) {
            Ok(json) => {
                ctx.copy_text(json);
                self.rescue_error = None;
            }
            Err(error) => self.rescue_error = Some(error.to_string()),
        }
    }

    fn storage_banner(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let error = self
            .persistence
            .as_ref()
            .and_then(|p| p.autosave.error.clone());
        let backup_error = self
            .persistence
            .as_ref()
            .and_then(|p| p.backup_error.clone());
        let maintenance_error = self
            .persistence
            .as_ref()
            .and_then(|p| p.maintenance_error.clone());
        if self.persistence.is_some()
            && error.is_none()
            && backup_error.is_none()
            && maintenance_error.is_none()
            && self.recovery_notice.is_none()
            && self.rescue_error.is_none()
            && self.startup_error.is_none()
        {
            return;
        }
        let mut directory_action = None;
        egui::Panel::top("storage-notice").show(ui, |ui| {
            if let Some(error) = self.startup_error.clone() {
                ui.label(if self.persistence.is_some() {
                    text::RECOVERY_EDITING
                } else {
                    text::MEMORY_RECOVERY
                });
                if let Some(root) = self.workspace_directory() {
                    ui.monospace(root.display().to_string());
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(text::CHOOSE_DIRECTORY).clicked() {
                        directory_action = Some(Action::ChooseDirectory);
                    }
                    if self.persistence.is_some() {
                        if ui.button(text::REVEAL_DIRECTORY).clicked() {
                            directory_action = Some(Action::RevealDirectory);
                        }
                        if ui.button(text::USE_RECOVERY_DIRECTORY).clicked() {
                            directory_action = Some(Action::UseRecoveryDirectory);
                        }
                    }
                });
                ui.collapsing(text::STORAGE_DETAILS, |ui| {
                    ui.label(error);
                    if let Some(p) = &self.persistence {
                        ui.monospace(p.store.paths().root.display().to_string());
                    }
                    ui.label(text::RECOVERY_KEEP);
                });
                if ui.small_button(text::COPY_RESCUE).clicked() {
                    self.copy_rescue(&ctx);
                }
            } else if self.persistence.is_none() {
                ui.colored_label(Color32::YELLOW, text::MEMORY_WARNING);
            }
            if let Some(error) = error {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{}{}", text::SAVE_ERROR, error),
                );
                ui.horizontal(|ui| {
                    if ui.button(text::RETRY_SAVE).clicked()
                        && let Err(error) = self.flush()
                    {
                        eprintln!("Tabloom retry: {error}");
                    }
                    if ui.button(text::COPY_RESCUE).clicked() {
                        self.copy_rescue(&ctx);
                    }
                });
            }
            if let Some(error) = backup_error {
                ui.label(format!("{}{}", text::BACKUP_ERROR, error));
                if ui.button(text::RETRY_BACKUP).clicked()
                    && let Some(p) = &mut self.persistence
                {
                    p.backup_error = p.store.backup(true).err().map(|e| e.to_string());
                }
            }
            if let Some(error) = maintenance_error {
                ui.label(format!("{}{error}", text::MAINTENANCE_ERROR));
                if ui.button(text::RETRY_MAINTENANCE).clicked()
                    && let Some(p) = &mut self.persistence
                {
                    p.maintenance_error = p.store.maintain().err().map(|e| e.to_string());
                }
            }
            if let Some(notice) = self.recovery_notice.clone() {
                ui.label(notice);
                if ui.button(text::DISMISS).clicked() {
                    self.recovery_notice = None;
                }
            }
            if let Some(error) = &self.rescue_error {
                ui.label(format!("{}{}", text::RESCUE_ERROR, error));
            }
        });
        if let Some(action) = directory_action {
            self.apply(&ctx, action);
        }
    }

    fn focus_active(&mut self) {
        self.pending_focus = self.document.active().map(|w| w.active_region);
        self.last_text_focus = self.document.active().map(|w| {
            if self.cache.markdown.mixed(w) && !self.search.open {
                crate::ui::markdown::source_id(w.active_region)
            } else {
                editor_region::editor_id(w.id)
            }
        });
    }

    fn input_text(&self, id: Id) -> Option<String> {
        if self.search.open && id == find_replace::query_id() {
            return Some(self.search.query.clone());
        }
        if self.search.open && self.search.show_replace && id == find_replace::replacement_id() {
            return Some(self.search.replacement.clone());
        }
        let w = self.document.active()?;
        if editor_region::editor_id(w.id) == id {
            return Some(w.text());
        }
        w.regions
            .iter()
            .find(|r| crate::ui::markdown::source_id(r.id) == id)
            .map(|r| r.text.clone())
    }

    fn document_proxy(&self) -> Option<crate::model::EditorRegion> {
        let w = self.document.active()?;
        let mut r = w.regions[0].clone();
        r.id = w.id;
        r.title = w.title.clone();
        r.text = w.text();
        Some(r)
    }

    fn extract_selection(&mut self, ctx: &egui::Context) {
        let Some(w) = self.document.active() else {
            return;
        };
        let range = if let Some(region) = w
            .regions
            .iter()
            .find(|r| Some(crate::ui::markdown::source_id(r.id)) == self.last_text_focus)
        {
            let range = commands::selected_bytes(
                ctx,
                crate::ui::markdown::source_id(region.id),
                &region.text,
            );
            let offset = w.region_offset(region.id).unwrap();
            offset + range.start..offset + range.end
        } else {
            commands::selected_bytes(ctx, editor_region::editor_id(w.id), &w.text())
        };
        self.format_selection(ctx, range, None);
    }

    fn format_selection(
        &mut self,
        ctx: &egui::Context,
        range: std::ops::Range<usize>,
        syntax: Option<String>,
    ) {
        let Some(w) = self.document.active_mut() else {
            return;
        };
        let id = editor_region::editor_id(w.id);
        match w.extract_lines(range, text::REGION) {
            Ok(selected) => {
                if let Some(syntax) = syntax {
                    let region = w.regions.iter_mut().find(|r| r.id == selected).unwrap();
                    region.syntax_id = syntax;
                    region.touch();
                    w.touch();
                }
                let start = w.region_offset(selected).unwrap();
                let length = w
                    .regions
                    .iter()
                    .find(|r| r.id == selected)
                    .unwrap()
                    .text
                    .len();
                self.cache.record(w);
                if self.cache.markdown.mixed(w) && !self.search.open {
                    let mut value = w.text();
                    commands::apply(
                        ctx,
                        id,
                        &mut value,
                        &TextCommand::SelectBytes(start..start + length),
                    );
                } else {
                    self.pending_text_command =
                        Some((id, TextCommand::SelectBytes(start..start + length)));
                }
                self.focus_active();
                self.mark_changed();
            }
            Err(error) => self.file_notice = Some((true, error)),
        }
    }

    fn can_extract(&self, ctx: &egui::Context) -> bool {
        self.document.active().is_some_and(|w| {
            let id = editor_region::editor_id(w.id);
            if self.last_text_focus == Some(id) {
                return crate::model::document::whole_lines(
                    &w.text(),
                    commands::selected_bytes(ctx, id, &w.text()),
                )
                .is_some();
            }
            w.regions.iter().any(|r| {
                let id = crate::ui::markdown::source_id(r.id);
                self.last_text_focus == Some(id)
                    && crate::model::document::whole_lines(
                        &r.text,
                        commands::selected_bytes(ctx, id, &r.text),
                    )
                    .is_some()
            })
        })
    }

    fn queue_command(&mut self, command: TextCommand) {
        let target = self
            .last_text_focus
            .filter(|id| self.input_text(*id).is_some())
            .or_else(|| {
                self.document
                    .active()
                    .map(|w| editor_region::editor_id(w.id))
            });
        if let Some(id) = target {
            self.pending_text_command = Some((id, command));
        }
    }

    fn open_find(&mut self, ctx: &egui::Context, replace: bool) {
        if let Some(region) = self.document_proxy() {
            let id = editor_region::editor_id(region.id);
            if ctx.memory(|m| m.has_focus(id)) {
                let bytes = commands::selected_bytes(ctx, id, &region.text);
                let selected = &region.text[bytes];
                if !selected.is_empty() && !selected.contains(['\r', '\n']) {
                    self.search.query = selected.into();
                }
            }
        }
        self.search.open = true;
        self.search.show_replace = replace;
        self.search.request_query_focus = true;
    }

    fn protect_clipboard_edit(&self, ctx: &egui::Context) {
        let (cut, paste) = ctx.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
            )
        });
        if !cut && !paste {
            return;
        }
        if let Some(id) = ctx.memory(|m| m.focused())
            && let Some(text) = self.input_text(id)
        {
            let bytes = commands::selected_bytes(ctx, id, &text);
            if cut && !clipboard::guard_cut(ctx, &text[bytes]) {
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Cut)));
                return;
            }
            // Native paste/cut can arrive faster than egui's undo coalescing delay.
            // Preserve the pre-edit state so undo always restores the deleted text.
            let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
            let mut undoer = state.undoer();
            undoer.add_undo(&(commands::selection(ctx, id, &text), text.to_owned()));
            state.set_undoer(undoer);
            state.store(ctx, id);
        }
    }

    fn apply(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::ToggleDocumentSource => {
                if let Some(w) = self.document.active() {
                    self.cache.markdown.source_workspace =
                        if self.cache.markdown.source_workspace == Some(w.id) {
                            None
                        } else {
                            Some(w.id)
                        };
                    self.cache.markdown.finish_editing();
                    self.pending_text_command = None;
                    self.pending_focus = None;
                    self.last_text_focus = None;
                    ctx.memory_mut(|m| m.surrender_focus(m.focused().unwrap_or(Id::NULL)));
                }
                return;
            }
            #[cfg(target_os = "macos")]
            Action::Quit => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            Action::ChooseDirectory => {
                self.choose_workspace_directory(ctx, false);
                return;
            }
            Action::UseRecoveryDirectory => {
                self.choose_workspace_directory(ctx, true);
                return;
            }
            Action::RevealDirectory => {
                self.reveal_workspace_directory();
                return;
            }
            Action::Appearance(appearance) => {
                self.set_appearance(ctx, appearance);
                return;
            }
            Action::ExtractSelection => {
                self.extract_selection(ctx);
                return;
            }
            Action::FocusSpan(id) => {
                if let Some(w) = self.document.active() {
                    self.cache.markdown.source_workspace = Some(w.id);
                }
                if let Some(w) = self.document.active_mut()
                    && let Some(start) = w.region_offset(id)
                {
                    w.active_region = id;
                    self.pending_text_command = Some((
                        editor_region::editor_id(w.id),
                        TextCommand::SelectBytes(start..start),
                    ));
                }
                self.focus_active();
                return;
            }
            Action::DeleteSpan => {
                self.pending_delete = self.document.active().map(|w| (w.id, w.active_region));
                return;
            }
            Action::OpenFile | Action::SaveFile | Action::ExportFile => {
                self.file_action(action);
                return;
            }
            Action::Find(replace) => {
                self.open_find(ctx, replace);
                return;
            }
            Action::Edit(command) => {
                self.queue_command(command);
                return;
            }
            Action::Paste => {
                match clipboard::read(ctx) {
                    Ok(text) => {
                        self.edit_error = None;
                        self.queue_command(TextCommand::Paste(text));
                    }
                    Err(error) => self.edit_error = Some(error),
                }
                return;
            }
            Action::Font(delta) => {
                self.preferences.font_size = (self.preferences.font_size + delta).clamp(10.0, 32.0);
                self.mark_changed();
                return;
            }
            Action::NewWorkspace => {
                self.document
                    .create_workspace(text::WORKSPACE, text::REGION);
            }
            Action::Select(id) => {
                self.document.select(id);
            }
            Action::Close(id) => {
                self.document.close(id);
                if self.document.active().is_none() {
                    self.document
                        .create_workspace(text::WORKSPACE, text::REGION);
                }
            }
            Action::Reopen(id) => {
                self.document.reopen(id);
            }
            Action::DeleteClosed(id) => {
                self.pending_permanent_delete = Some(id);
                return;
            }
            Action::AddRegion => {
                if let Some(workspace) = self.document.active_mut() {
                    if let Some(last) = workspace.regions.last_mut()
                        && !last.text.is_empty()
                        && !last.text.ends_with('\n')
                    {
                        last.text.push('\n');
                        last.touch();
                    }
                    workspace.add_region(text::REGION);
                    let end = workspace.text().len();
                    self.pending_text_command = Some((
                        editor_region::editor_id(workspace.id),
                        TextCommand::SelectBytes(end..end),
                    ));
                }
            }
        }
        self.focus_active();
        self.mark_changed();
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.pending_delete.is_some()
            || self.pending_permanent_delete.is_some()
            || self.confirm_exit
        {
            return;
        }
        let mut action = None;
        let mac = ctx.os() == egui::os::OperatingSystem::Mac;
        ctx.input_mut(|input| {
            if input.consume_key(Modifiers::COMMAND, Key::O) {
                action = Some(Action::OpenFile);
            }
            if input.consume_key(Modifiers::COMMAND, Key::S) {
                action = Some(Action::SaveFile);
            }
            if input.consume_key(Modifiers::COMMAND, Key::F) {
                action = Some(Action::Find(false));
            }
            if input.consume_key(shortcuts::replace_modifiers(mac), Key::H) {
                action = Some(Action::Find(true));
            }
            let mut font_delta = 0.0;
            input.events.retain(|event| {
                if let egui::Event::Key {
                    key,
                    modifiers,
                    pressed: true,
                    ..
                } = event
                    && let Some(delta) = shortcuts::font_delta(*key, *modifiers)
                {
                    font_delta += delta;
                    false
                } else {
                    true
                }
            });
            if font_delta != 0.0 {
                self.preferences.font_size =
                    (self.preferences.font_size + font_delta).clamp(10.0, 32.0);
                self.mark_changed();
            }
            if self.search.open {
                if input.consume_key(Modifiers::NONE, Key::Escape) {
                    self.pending_find_action = Some(FindAction::Close);
                } else if input.consume_key(Modifiers::SHIFT, Key::F3) {
                    self.pending_find_action = Some(FindAction::Previous);
                } else if input.consume_key(Modifiers::NONE, Key::F3) {
                    self.pending_find_action = Some(FindAction::Next);
                }
                let query_focused = self.last_text_focus.is_some_and(|id| {
                    id == find_replace::query_id() || id == find_replace::replacement_id()
                });
                if query_focused {
                    if input.consume_key(Modifiers::SHIFT, Key::Enter) {
                        self.pending_find_action = Some(FindAction::Previous);
                    } else if input.consume_key(Modifiers::NONE, Key::Enter) {
                        self.pending_find_action = Some(FindAction::Next);
                    }
                }
            }
            if input.consume_key(Modifiers::COMMAND, Key::N) {
                action = Some(Action::NewWorkspace);
            }
            if input.consume_key(Modifiers::COMMAND, Key::W) {
                action = self.document.active_workspace.map(Action::Close);
            }
            // macOS reserves Cmd+Tab for switching applications. Ctrl+Tab is
            // the platform's conventional in-app tab navigation shortcut.
            let tab_modifier = if mac {
                Modifiers::CTRL
            } else {
                Modifiers::COMMAND
            };
            if input.consume_key(tab_modifier | Modifiers::SHIFT, Key::Tab) {
                self.document.cycle(-1);
                self.focus_active();
                self.mark_changed();
            } else if input.consume_key(tab_modifier, Key::Tab) {
                self.document.cycle(1);
                self.focus_active();
                self.mark_changed();
            }
        });
        if let Some(action) = action {
            self.apply(ctx, action);
        }
    }

    fn menus(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        egui::MenuBar::new().ui(ui, |ui| {
            let p = theme::palette(self.preferences.appearance());
            let (logo, _) = ui.allocate_exact_size(egui::vec2(12., 12.), egui::Sense::hover());
            ui.painter().rect_filled(logo.shrink(2.), 2, p.accent);
            ui.label(egui::RichText::new(text::APP_NAME).size(16.).strong());
            ui.add_space(18.);

            ui.menu_button(text::FILE, |ui| {
                ui.set_max_width(440.);
                if ui.button(text::CHOOSE_DIRECTORY).clicked() {
                    actions.push(Action::ChooseDirectory);
                    ui.close();
                }
                if ui.button(text::REVEAL_DIRECTORY).clicked() {
                    actions.push(Action::RevealDirectory);
                    ui.close();
                }
                if let Some(root) = self.workspace_directory() {
                    ui.label(
                        egui::RichText::new(root.display().to_string())
                            .small()
                            .weak(),
                    );
                }
                ui.label(egui::RichText::new(text::DIRECTORY_HELP).small().weak());
                ui.separator();
                for (label, shortcut, action) in [
                    (
                        text::OPEN_FILE,
                        shortcuts::command_label(ui.os(), "O"),
                        Action::OpenFile,
                    ),
                    (
                        text::SAVE_FILE,
                        shortcuts::command_label(ui.os(), "S"),
                        Action::SaveFile,
                    ),
                    (text::EXPORT_FILE, String::new(), Action::ExportFile),
                ] {
                    if ui
                        .add(egui::Button::new(label).shortcut_text(shortcut))
                        .clicked()
                    {
                        actions.push(action);
                        ui.close();
                    }
                }
                ui.separator();
                let modifier = if ui.os() == egui::os::OperatingSystem::Mac {
                    "⌘"
                } else {
                    "Ctrl+"
                };
                if ui
                    .button(format!("{}    {modifier}N", text::NEW_WORKSPACE))
                    .clicked()
                {
                    actions.push(Action::NewWorkspace);
                    ui.close();
                }
                if ui
                    .button(format!("{}    {modifier}W", text::CLOSE_WORKSPACE))
                    .clicked()
                {
                    if let Some(id) = self.document.active_workspace {
                        actions.push(Action::Close(id));
                    }
                    ui.close();
                }
                ui.separator();
                ui.menu_button(text::RECENTLY_CLOSED, |ui| {
                    let mut closed = self
                        .document
                        .workspaces
                        .iter()
                        .filter(|w| w.status == WorkspaceStatus::Closed)
                        .collect::<Vec<_>>();
                    closed.sort_by_key(|w| std::cmp::Reverse(w.closed_at));
                    if closed.is_empty() {
                        ui.label(text::NONE_CLOSED);
                    }
                    for workspace in closed {
                        ui.horizontal(|ui| {
                            if ui.button(&workspace.title).clicked() {
                                actions.push(Action::Reopen(workspace.id));
                                ui.close();
                            }
                            if ui.small_button(text::PERMANENT_DELETE).clicked() {
                                actions.push(Action::DeleteClosed(workspace.id));
                                ui.close();
                            }
                        });
                    }
                });
            });
            ui.menu_button(text::EDIT, |ui| {
                if ui
                    .add_enabled(
                        self.can_extract(ui.ctx()),
                        egui::Button::new(text::EXTRACT_SELECTION),
                    )
                    .on_hover_text(text::WHOLE_LINE_HINT)
                    .clicked()
                {
                    actions.push(Action::ExtractSelection);
                    ui.close();
                }
                ui.menu_button(text::SPAN_TOOLS, |ui| {
                    if ui.button(text::NEW_REGION).clicked() {
                        actions.push(Action::AddRegion);
                        ui.close();
                    }
                    if let Some(w) = self.document.active() {
                        for r in &w.regions {
                            if ui
                                .selectable_label(r.id == w.active_region, &r.title)
                                .clicked()
                            {
                                actions.push(Action::FocusSpan(r.id));
                                ui.close();
                            }
                        }
                    }
                    if ui.button(text::DELETE).clicked() {
                        actions.push(Action::DeleteSpan);
                        ui.close();
                    }
                });
                ui.separator();
                let os = ui.os();
                let entries = [
                    (
                        text::UNDO,
                        shortcuts::command_label(os, "Z"),
                        Action::Edit(TextCommand::Undo),
                    ),
                    (
                        text::REDO,
                        shortcuts::command_label(
                            os,
                            if os == egui::os::OperatingSystem::Mac {
                                "⇧Z"
                            } else {
                                "Y"
                            },
                        ),
                        Action::Edit(TextCommand::Redo),
                    ),
                    (
                        text::COPY,
                        shortcuts::command_label(os, "C"),
                        Action::Edit(TextCommand::Copy),
                    ),
                    (
                        text::CUT,
                        shortcuts::command_label(os, "X"),
                        Action::Edit(TextCommand::Cut),
                    ),
                    (
                        text::PASTE,
                        shortcuts::command_label(os, "V"),
                        Action::Paste,
                    ),
                    (
                        text::SELECT_ALL,
                        shortcuts::command_label(os, "A"),
                        Action::Edit(TextCommand::SelectAll),
                    ),
                    (
                        text::FIND,
                        shortcuts::command_label(os, "F"),
                        Action::Find(false),
                    ),
                    (
                        text::FIND_REPLACE,
                        shortcuts::replacement_label(os).into(),
                        Action::Find(true),
                    ),
                ];
                for (label, shortcut, action) in entries {
                    if ui
                        .add(egui::Button::new(label).shortcut_text(shortcut))
                        .clicked()
                    {
                        actions.push(action);
                        ui.close();
                    }
                }
            });
            ui.menu_button(text::VIEW, |ui| {
                if ui
                    .add(
                        egui::Button::new(text::FONT_INCREASE)
                            .shortcut_text(shortcuts::command_label(ui.os(), "+")),
                    )
                    .clicked()
                {
                    actions.push(Action::Font(1.0));
                }
                if ui
                    .add(
                        egui::Button::new(text::FONT_DECREASE)
                            .shortcut_text(shortcuts::command_label(ui.os(), "-")),
                    )
                    .clicked()
                {
                    actions.push(Action::Font(-1.0));
                }
                ui.separator();
                for appearance in Appearance::ALL {
                    if ui
                        .selectable_label(
                            self.preferences.appearance() == appearance,
                            theme::label(appearance),
                        )
                        .clicked()
                    {
                        actions.push(Action::Appearance(appearance));
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .add(
                        egui::Slider::new(&mut self.preferences.font_size, 10.0..=32.0)
                            .text(text::FONT_SIZE),
                    )
                    .changed()
                {
                    self.mark_changed();
                }
                ui.separator();
                ui.label(text::LOG_NOTE);
            });
        });
    }

    fn tabs(&self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        let appearance = self.preferences.appearance();
        let p = theme::palette(appearance);
        egui::ScrollArea::horizontal()
            .id_salt("tab-strip")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    for w in &self.document.workspaces {
                        if w.status != WorkspaceStatus::Open {
                            continue;
                        }
                        ui.push_id(w.id, |ui| {
                            let selected = self.document.active_workspace == Some(w.id);
                            let fill = if selected && appearance != Appearance::Graphite {
                                p.active
                            } else {
                                Color32::TRANSPARENT
                            };
                            let frame = egui::Frame::NONE
                                .fill(fill)
                                .corner_radius(if appearance == Appearance::Sand { 6 } else { 3 })
                                .inner_margin(egui::Margin::symmetric(10, 3));
                            let tab = frame.show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let title = egui::RichText::new(&w.title)
                                        .size(13.)
                                        .color(if selected { p.text } else { p.muted });
                                    let response = ui.add(egui::Button::new(title).frame(false));
                                    if response.clicked() {
                                        actions.push(Action::Select(w.id));
                                    }
                                    response.context_menu(|ui| {
                                        if ui.button(text::CLOSE_WORKSPACE).clicked() {
                                            actions.push(Action::Close(w.id));
                                            ui.close();
                                        }
                                    });
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("×").color(p.muted),
                                            )
                                            .frame(false)
                                            .min_size(egui::vec2(20., 22.)),
                                        )
                                        .on_hover_text(text::CLOSE_WORKSPACE)
                                        .clicked()
                                    {
                                        actions.push(Action::Close(w.id));
                                    }
                                });
                            });
                            if selected
                                && matches!(appearance, Appearance::Graphite | Appearance::Ocean)
                            {
                                let rect = tab.response.rect;
                                let y = if appearance == Appearance::Ocean {
                                    rect.top()
                                } else {
                                    rect.bottom()
                                };
                                ui.painter().line_segment(
                                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                                    egui::Stroke::new(2., p.accent),
                                );
                            }
                        });
                    }
                    let add = ui
                        .add(
                            egui::Button::new(egui::RichText::new("+").color(p.muted).size(17.))
                                .frame(false),
                        )
                        .on_hover_text(text::NEW_WORKSPACE);
                    add.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::Role::Button,
                            ui.is_enabled(),
                            text::NEW_WORKSPACE,
                        )
                    });
                    if add.clicked() {
                        actions.push(Action::NewWorkspace);
                    }
                });
            });
    }

    fn regions(&mut self, ui: &mut egui::Ui) {
        let command = self.pending_text_command.take();
        let Some(w) = self.document.active_mut() else {
            return;
        };
        if self.pending_focus.is_some()
            && egui::TextEdit::load_state(ui.ctx(), editor_region::editor_id(w.id)).is_none()
        {
            let r = w.regions.iter().find(|r| r.id == w.active_region).unwrap();
            let end = w.region_offset(r.id).unwrap() + r.text.len();
            let mut value = w.text();
            commands::apply(
                ui.ctx(),
                editor_region::editor_id(w.id),
                &mut value,
                &TextCommand::SelectBytes(end..end),
            );
        }
        if let Some(error) = self.cache.error(w.id) {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!("{}{error}", text::HIGHLIGHT_ERROR),
            );
        }
        if editor_region::limited(w) {
            ui.weak(text::LARGE_TEXT_NOTE);
        }
        let mixed = self.cache.markdown.mixed(w) && !self.search.open;
        let view = DocumentView {
            highlighter: &self.highlighter,
            state: &mut self.cache,
            font_size: self.preferences.font_size,
            request_focus: self.pending_focus.take().is_some(),
            command: command
                .as_ref()
                .filter(|(id, _)| *id == editor_region::editor_id(w.id))
                .map(|(_, c)| c),
            matched: if self.search.open {
                self.search.current.clone()
            } else {
                None
            },
        };
        let output = if mixed {
            crate::ui::markdown::show(ui, w, view, command.clone())
        } else {
            editor_region::show(ui, w, view)
        };
        if output.changed {
            self.mark_changed();
            ui.ctx().request_repaint();
        }
        if let Some((range, syntax)) = output.format {
            self.format_selection(ui.ctx(), range, Some(syntax));
        }
    }

    fn find_panel(&mut self, ui: &mut egui::Ui) {
        if !self.search.open {
            return;
        }
        let Some(mut region) = self.document_proxy() else {
            return;
        };
        self.search.refresh(region.id, &region.text);
        let field_command = if self.pending_text_command.as_ref().is_some_and(|(id, _)| {
            *id == find_replace::query_id() || *id == find_replace::replacement_id()
        }) {
            self.pending_text_command.take()
        } else {
            None
        };
        let output = egui::Panel::top("find-replace")
            .show(ui, |ui| {
                find_replace::show(ui, &mut self.search, &region, field_command)
            })
            .inner;
        if let Some(focused) = output.target_focus {
            self.last_text_focus = Some(focused);
        }
        let action = output.action.or(self.pending_find_action.take());
        if let Some(action) = action {
            let (changed, focus) =
                find_replace::apply_action(ui.ctx(), &mut self.search, &mut region, action);
            if changed && let Some(w) = self.document.active_mut() {
                self.cache.apply(w, &region.text, false, None);
                self.mark_changed();
            }
            if focus {
                self.focus_active();
            }
            ui.ctx().request_repaint();
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some((workspace_id, region_id)) = self.pending_delete {
            egui::Modal::new(Id::unique("delete-region")).show(ctx, |ui| {
                ui.heading(text::DELETE_TITLE);
                ui.label(text::DELETE_MESSAGE);
                ui.horizontal(|ui| {
                    if ui.button(text::CANCEL).clicked() {
                        self.pending_delete = None;
                    }
                    if ui.button(text::CONFIRM_DELETE).clicked() {
                        if let Some(workspace) = self
                            .document
                            .workspaces
                            .iter_mut()
                            .find(|w| w.id == workspace_id)
                        {
                            workspace.remove_region(region_id, text::REGION);
                            self.cache.remove(workspace_id);
                            egui::text_edit::TextEditState::default()
                                .store(ctx, editor_region::editor_id(workspace_id));
                        }
                        self.pending_delete = None;
                        self.focus_active();
                        self.mark_changed();
                    }
                });
            });
        }
        if let Some(id) = self.pending_permanent_delete {
            egui::Modal::new(Id::unique("permanent-delete")).show(ctx, |ui| {
                ui.heading(text::PERMANENT_DELETE_TITLE);
                ui.label(text::PERMANENT_DELETE_MESSAGE);
                ui.horizontal(|ui| {
                    if ui.button(text::CANCEL).clicked() {
                        self.pending_permanent_delete = None;
                    }
                    if ui.button(text::CONFIRM_PERMANENT_DELETE).clicked() {
                        self.cache.remove(id);
                        if self.document.delete_closed(id) {
                            self.mark_changed();
                        }
                        self.pending_permanent_delete = None;
                    }
                });
            });
        }
        if self.confirm_exit {
            egui::Modal::new(Id::unique("confirm-exit")).show(ctx, |ui| {
                let committed = self
                    .persistence
                    .as_ref()
                    .is_some_and(|p| !p.autosave.dirty());
                ui.heading(if committed {
                    text::MAINTENANCE_EXIT_TITLE
                } else {
                    text::EXIT_TITLE
                });
                ui.label(if committed {
                    text::MAINTENANCE_EXIT_MESSAGE
                } else {
                    text::EXIT_MESSAGE
                });
                if let Some(error) = self
                    .persistence
                    .as_ref()
                    .and_then(|p| p.autosave.error.as_ref())
                {
                    ui.label(error);
                }
                if ui.button(text::COPY_RESCUE).clicked() {
                    self.copy_rescue(ctx);
                }
                ui.horizontal(|ui| {
                    if ui.button(text::CANCEL).clicked() {
                        self.confirm_exit = false;
                    }
                    if self.persistence.is_some()
                        && ui.button(text::RETRY_AND_EXIT).clicked()
                        && self.finish().is_ok()
                    {
                        self.allow_exit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui
                        .button(if committed {
                            text::EXIT_COMMITTED
                        } else {
                            text::CONFIRM_EXIT
                        })
                        .clicked()
                    {
                        self.allow_exit = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        }
    }

    /// Shared by the native app and headless interaction tests.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let appearance = self.preferences.appearance();
        if self.applied_appearance != Some(appearance) {
            theme::apply(&ctx, appearance);
            self.applied_appearance = Some(appearance);
        }
        ui.set_style(ctx.global_style());
        let colors = theme::palette(appearance);
        let previous_revision = self.autosave().map(Autosave::revision);
        if let Some(id) = ctx.memory(|m| m.focused())
            && self.input_text(id).is_some()
        {
            self.last_text_focus = Some(id);
        }
        self.protect_clipboard_edit(&ctx);
        self.shortcuts(&ctx);
        let mut actions = Vec::new();
        #[cfg(target_os = "macos")]
        if let Some(menu) = &self.native_menu {
            actions.extend(menu.actions());
        }
        let native = {
            #[cfg(target_os = "macos")]
            {
                self.native_menu.is_some()
            }
            #[cfg(not(target_os = "macos"))]
            {
                false
            }
        };
        if !native {
            egui::Panel::top("menu")
                .frame(theme::panel(colors.bar, 14, 7))
                .show(ui, |ui| self.menus(ui, &mut actions));
        }
        self.storage_banner(ui);
        egui::Panel::top("tabs")
            .frame(theme::panel(colors.tabs, 12, 4))
            .show(ui, |ui| self.tabs(ui, &mut actions));
        for action in actions {
            self.apply(&ctx, action);
        }
        if let Some(error) = clipboard::error(&ctx).or_else(|| self.edit_error.clone()) {
            egui::Panel::top("clipboard-error").show(ui, |ui| {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    format!("{}{error}", text::CLIPBOARD_ERROR),
                );
                if ui.button(text::CLEAR_CLIPBOARD_ERROR).clicked() {
                    ctx.with_plugin::<PlainTextClipboard, _>(|p| p.error = None);
                    self.edit_error = None;
                }
            });
        }
        if let Some((failed, message)) = self.file_notice.clone() {
            egui::Panel::top("file-notice").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if failed {
                        ui.colored_label(ui.visuals().error_fg_color, message);
                    } else {
                        ui.label(message);
                    }
                    if ui.small_button(text::DISMISS).clicked() {
                        self.file_notice = None;
                    }
                });
            });
        }
        self.find_panel(ui);
        let mut status_action = None;
        egui::Panel::bottom("status")
            .frame(theme::panel(colors.footer, 14, 4))
            .show(ui, |ui| {
                ui.style_mut().override_font_id = Some(egui::FontId::proportional(11.));
                ui.horizontal(|ui| {
                    let left_width = (ui.available_width() - 160.).max(120.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, 26.),
                        egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true),
                        |ui| {
                            ui.visuals_mut().widgets.inactive.bg_stroke = egui::Stroke::NONE;
                            if let Some(w) = self.document.active() {
                                let value = w.text();
                                let r = w.regions.iter().find(|r| r.id == w.active_region).unwrap();
                                let selected = commands::selected_bytes(
                                    ui.ctx(),
                                    editor_region::editor_id(w.id),
                                    &value,
                                );
                                let mut offset = 0;
                                let mut selected_syntax = None;
                                let mut mixed = false;
                                for region in &w.regions {
                                    let end = offset + region.text.len();
                                    if selected.start < end && selected.end > offset {
                                        if let Some(syntax) = selected_syntax {
                                            mixed |= syntax != region.syntax_id;
                                        } else {
                                            selected_syntax = Some(region.syntax_id.as_str());
                                        }
                                    }
                                    offset = end;
                                }
                                let label = if mixed {
                                    text::MIXED_FORMAT
                                } else {
                                    self.highlighter
                                        .language_label(selected_syntax.unwrap_or(&r.syntax_id))
                                };
                                ui.label(format!("{}：{}", text::CURRENT_FORMAT, label));
                                if crate::ui::markdown::has_markdown(w) {
                                    let label =
                                        if self.cache.markdown.source_workspace == Some(w.id) {
                                            text::DOCUMENT_PREVIEW
                                        } else {
                                            text::DOCUMENT_SOURCE
                                        };
                                    if ui.small_button(label).clicked() {
                                        status_action = Some(Action::ToggleDocumentSource);
                                    }
                                }
                                ui.add_space(8.);
                                ui.weak(format!(
                                    "{} {} · {} {}",
                                    value.split('\n').count(),
                                    text::LINES,
                                    value.chars().count(),
                                    text::CHARACTERS
                                ));
                                let cursor = commands::selection(
                                    ui.ctx(),
                                    editor_region::editor_id(w.id),
                                    &value,
                                )
                                .primary
                                .index;
                                let byte = clipboard::char_to_byte(&value, cursor.into());
                                let before = &value[..byte];
                                ui.label(format!(
                                    "{} {} · {} {}",
                                    text::CURSOR_LINE,
                                    before.bytes().filter(|b| *b == b'\n').count() + 1,
                                    text::CURSOR_COLUMN,
                                    before.rsplit('\n').next().unwrap_or("").chars().count() + 1
                                ));
                                ui.menu_button(text::STORAGE_LOCATION, |ui| {
                                    ui.set_max_width(440.);
                                    if let Some(root) = self.workspace_directory() {
                                        ui.monospace(root.display().to_string());
                                    } else {
                                        ui.label(text::NO_DIRECTORY);
                                    }
                                    if ui.button(text::REVEAL_DIRECTORY).clicked() {
                                        status_action = Some(Action::RevealDirectory);
                                        ui.close();
                                    }
                                    if ui.button(text::CHOOSE_DIRECTORY).clicked() {
                                        status_action = Some(Action::ChooseDirectory);
                                        ui.close();
                                    }
                                });
                                if let Some(path) = &r.file_path {
                                    ui.weak(path.to_string_lossy());
                                }
                            }
                        },
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(match self.persistence.as_ref() {
                            None => text::MEMORY_STATUS,
                            Some(p) if p.autosave.error.is_some() => text::SAVE_FAILED,
                            Some(p) if p.autosave.dirty() => text::SAVE_PENDING,
                            Some(_) if self.startup_error.is_some() => text::RECOVERY_SAVED,
                            Some(_) => text::SAVED,
                        });
                        ui.colored_label(colors.accent, "·");
                    });
                });
            });
        if let Some(action) = status_action {
            self.apply(&ctx, action);
        }
        egui::CentralPanel::default()
            .frame(theme::panel(colors.document, 0, 0))
            .show(ui, |ui| self.regions(ui));
        self.dialogs(&ctx);
        if self.geometry_grace > 0 {
            self.geometry_grace -= 1;
        } else if self.persistence.is_some()
            && let Some(rect) = ctx.input(|i| i.viewport().inner_rect)
        {
            let size = [rect.width(), rect.height()];
            if size.iter().all(|s| s.is_finite() && *s >= 100.0)
                && self.preferences.window_size != size
            {
                self.preferences.window_size = size;
                self.mark_changed();
            }
        }
        self.tick(&ctx);
        if self.autosave().map(Autosave::revision) != previous_revision {
            ctx.request_repaint();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_exit {
            let has_content = self
                .document
                .workspaces
                .iter()
                .flat_map(|w| &w.regions)
                .any(|r| !r.text.is_empty());
            if self.persistence.is_some() {
                if self.finish().is_ok() {
                    self.allow_exit = true;
                } else {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    self.confirm_exit = true;
                }
            } else if has_content {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_exit = true;
            }
        }
        #[cfg(target_os = "macos")]
        if self.native_menu.is_some() {
            let can_extract = self.can_extract(&ctx);
            let editable = self
                .last_text_focus
                .is_some_and(|id| self.input_text(id).is_some());
            let modal = self.confirm_exit
                || self.pending_delete.is_some()
                || self.pending_permanent_delete.is_some();
            let menu = self.native_menu.as_mut().unwrap();
            if let Err(error) = menu.sync(
                &self.document,
                self.preferences.appearance(),
                can_extract,
                editable,
                modal,
            ) {
                self.file_notice = Some((true, format!("{}{error}", text::NATIVE_MENU_ERROR)));
                self.native_menu = None;
            }
        }
    }
}

impl eframe::App for TabloomApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick(ctx);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if !self.allow_exit
            && self.persistence.is_some()
            && let Err(error) = self.finish()
        {
            eprintln!("Tabloom final save: {error}");
        }
    }
}
