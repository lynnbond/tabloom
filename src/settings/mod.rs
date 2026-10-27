//! Shared application defaults and OS-standard local data paths.
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

pub const SESSION_NAME: &str = "session.json";
pub const DEBOUNCE: Duration = Duration::from_millis(400);
pub const MAX_UNSAVED: Duration = Duration::from_secs(2);
pub const RETRY_DELAY: Duration = Duration::from_secs(3);
pub const BACKUP_INTERVAL: Duration = Duration::from_secs(60);
pub const BACKUPS_TO_KEEP: usize = 3;

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub root: PathBuf,
}

impl AppPaths {
    pub fn standard() -> Option<Self> {
        directories::ProjectDirs::from("org", "tabloom", "Tabloom")
            .map(|p| Self::at(p.data_local_dir().to_path_buf()))
    }
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn session(&self) -> PathBuf {
        self.root.join(SESSION_NAME)
    }
    pub fn backups(&self) -> PathBuf {
        self.root.join("backups")
    }
    pub fn recovery(&self) -> PathBuf {
        self.root.join("recovery")
    }
    pub fn workspaces(&self) -> PathBuf {
        self.root.join("workspaces")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Appearance {
    Graphite,
    Paper,
    Ocean,
    Sand,
}
impl Appearance {
    pub const ALL: [Self; 4] = [Self::Graphite, Self::Paper, Self::Ocean, Self::Sand];
    pub fn dark(self) -> bool {
        matches!(self, Self::Graphite | Self::Ocean)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preferences {
    pub dark: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<Appearance>,
    pub font_size: f32,
    pub window_size: [f32; 2],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            dark: true,
            appearance: None,
            font_size: 15.0,
            window_size: [1100.0, 760.0],
        }
    }
}

impl Preferences {
    pub fn appearance(&self) -> Appearance {
        self.appearance.unwrap_or(if self.dark {
            Appearance::Graphite
        } else {
            Appearance::Paper
        })
    }
    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.appearance = Some(appearance);
        self.dark = appearance.dark();
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.font_size.is_finite()
            || !(10.0..=32.0).contains(&self.font_size)
            || self
                .window_size
                .iter()
                .any(|v| !v.is_finite() || !(100.0..=20000.0).contains(v))
        {
            return Err("invalid preferences".into());
        }
        Ok(())
    }
}

/// The selected workspace lives outside the workspace's own snapshot so switching
/// directories survives restart without copying application config into user folders.
#[derive(Serialize, Deserialize)]
struct DirectoryChoice {
    version: u32,
    directory: PathBuf,
}
pub fn directory_choice_file() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "tabloom", "Tabloom")
        .map(|p| p.config_dir().join("workspace-location.json"))
}
pub fn read_directory_choice(file: &std::path::Path) -> std::io::Result<Option<PathBuf>> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    let choice: DirectoryChoice = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    if choice.version != 1 || !choice.directory.is_absolute() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid workspace directory configuration",
        ));
    }
    Ok(Some(choice.directory))
}
pub fn write_directory_choice(
    file: &std::path::Path,
    directory: &std::path::Path,
) -> std::io::Result<()> {
    use std::io::Write;
    let directory = directory.canonicalize()?;
    if !directory.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "workspace path is not a directory",
        ));
    }
    let parent = file.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "configuration path has no parent",
        )
    })?;
    std::fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(
        &serde_json::to_vec_pretty(&DirectoryChoice {
            version: 1,
            directory,
        })
        .map_err(std::io::Error::other)?,
    )?;
    staged.as_file().sync_all()?;
    staged.persist(file).map_err(|e| e.error)?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn configured_paths() -> crate::persistence::Result<AppPaths> {
    if let Some(config) = directory_choice_file()
        && let Some(directory) = read_directory_choice(&config)?
    {
        return Ok(AppPaths::at(directory));
    }
    AppPaths::standard().ok_or(crate::persistence::StorageError::NoDataDirectory)
}
