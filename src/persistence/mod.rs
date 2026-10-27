pub mod autosave;
pub mod store;

use crate::{
    model::Document,
    settings::{AppPaths, Preferences},
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, time::Duration};

pub use store::Store;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("该工作区正被另一个 Tabloom 实例使用：{0}")]
    AlreadyRunning(PathBuf),
    #[error("unsupported storage format version {0}; this build supports version 1")]
    UnsupportedVersion(i64),
    #[error("invalid persisted state: {0}")]
    InvalidData(String),
    #[error("workspace integrity check failed: {0}")]
    Corrupt(String),
    #[error("no valid backup; original files retained in {archive}: {reason}")]
    NoBackup { archive: PathBuf, reason: String },
    #[error("standard user data directory is unavailable")]
    NoDataDirectory,
}

impl StorageError {
    fn is_corruption(&self) -> bool {
        matches!(
            self,
            Self::Corrupt(_) | Self::InvalidData(_) | Self::Json(_)
        )
    }
}

pub type Result<T> = std::result::Result<T, StorageError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub document: Document,
    pub preferences: Preferences,
}

impl Snapshot {
    pub fn validate(&self) -> Result<()> {
        self.document
            .validate()
            .map_err(StorageError::InvalidData)?;
        self.preferences
            .validate()
            .map_err(StorageError::InvalidData)
    }
}

pub struct Persistence {
    pub store: Store,
    pub autosave: autosave::Autosave,
    pub backup_error: Option<String>,
    pub maintenance_error: Option<String>,
}

impl Persistence {
    pub fn open(paths: AppPaths) -> Result<(Self, Option<Snapshot>)> {
        let store = Store::open(paths)?;
        let snapshot = store.load()?;
        Ok((
            Self {
                store,
                autosave: autosave::Autosave::default(),
                backup_error: None,
                maintenance_error: None,
            },
            snapshot,
        ))
    }

    /// Separate recovery storage keeps the editor usable without overwriting a
    /// locked, unreadable or damaged primary workspace. Stable paths restore on retry.
    pub fn open_recovery(primary: Option<&AppPaths>) -> Result<(Self, Option<Snapshot>)> {
        let mut candidates = Vec::new();
        if let Some(primary) = primary {
            let mut sibling = primary.root.as_os_str().to_os_string();
            sibling.push(".recovery");
            candidates.push(AppPaths::at(sibling.into()));
        }
        let identity = primary.map_or_else(
            || "standard".into(),
            |p| p.root.to_string_lossy().into_owned(),
        );
        let key = crate::file_io::hash(identity.as_bytes())
            .iter()
            .take(8)
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if let Some(base) = directories::BaseDirs::new() {
            candidates.push(AppPaths::at(
                base.cache_dir()
                    .join("Tabloom")
                    .join("recovery-workspaces")
                    .join(&key),
            ));
        }
        candidates.push(AppPaths::at(
            std::env::temp_dir().join("tabloom-recovery").join(&key),
        ));
        candidates.push(AppPaths::at(
            std::env::temp_dir()
                .join("tabloom-recovery")
                .join(uuid::Uuid::new_v4().to_string()),
        ));
        let mut errors = Vec::new();
        for paths in candidates {
            match Self::open(paths) {
                Ok(value) => return Ok(value),
                Err(error) => errors.push(error.to_string()),
            }
        }
        Err(StorageError::InvalidData(format!(
            "no writable recovery directory: {}",
            errors.join("; ")
        )))
    }

    pub fn save(
        &mut self,
        document: &Document,
        preferences: &Preferences,
        now: Duration,
    ) -> Result<()> {
        let revision = self.autosave.revision();
        match self.store.save(document, preferences) {
            Ok(()) => {
                self.autosave.saved(revision);
                self.maintenance_error = self.store.maintain().err().map(|e| e.to_string());
                self.refresh_backup(false);
                Ok(())
            }
            Err(error) => {
                self.autosave.failed(now, error.to_string());
                Err(error)
            }
        }
    }

    fn refresh_backup(&mut self, force: bool) {
        match self.store.backup(force) {
            Ok(Some(_)) => self.backup_error = None,
            Ok(None) => {} // A throttled check is not proof a previous backup error recovered.
            Err(error) => self.backup_error = Some(error.to_string()),
        }
    }

    pub fn finish(
        &mut self,
        document: &Document,
        preferences: &Preferences,
        now: Duration,
    ) -> Result<()> {
        // Flush before exit and remove unreferenced immutable workspace revisions.
        self.save(document, preferences, now)?;
        if let Err(error) = self.store.maintain() {
            // The manifest already committed; maintenance failure is separate from unsaved text.
            self.maintenance_error = Some(error.to_string());
            return Err(error);
        }
        self.maintenance_error = None;
        self.refresh_backup(true);
        Ok(())
    }
}
