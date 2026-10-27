//! Immutable workspace files + an atomically replaced commit manifest.
use super::{Result, Snapshot, StorageError};
use crate::{
    model::{Document, Workspace},
    settings::{AppPaths, BACKUP_INTERVAL, BACKUPS_TO_KEEP, Preferences},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions, TryLockError},
    io::Write,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

pub struct Store {
    _lock: File,
    paths: AppPaths,
    pub recovery_archive: Option<PathBuf>,
    last_backup_attempt: Option<Instant>,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    version: i64,
    workspaces: Vec<String>,
    active_workspace: Option<Uuid>,
    next_workspace_number: usize,
    preferences: Preferences,
}
#[derive(Serialize, Deserialize)]
struct Backup {
    version: i64,
    snapshot: Snapshot,
}
fn check_version(value: &serde_json::Value) -> Result<()> {
    let version = value
        .get("version")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| StorageError::Corrupt("missing format version".into()))?;
    if version != 1 {
        return Err(StorageError::UnsupportedVersion(version));
    }
    Ok(())
}
fn read_manifest(path: &Path) -> Result<Manifest> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    check_version(&value)?;
    Ok(serde_json::from_value(value)?)
}
fn read_backup(path: &Path) -> Result<Snapshot> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
    check_version(&value)?;
    let b: Backup = serde_json::from_value(value)?;
    b.snapshot.validate()?;
    Ok(b.snapshot)
}
fn digest(bytes: &[u8]) -> String {
    crate::file_io::hash(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn workspace_name(w: &Workspace, bytes: &[u8]) -> String {
    format!("{}-{}.json", w.id, digest(bytes))
}
fn valid_name(name: &str) -> bool {
    name.len() == 106
        && Uuid::parse_str(&name[..36]).is_ok()
        && name.as_bytes()[36] == b'-'
        && name[37..101].bytes().all(|c| c.is_ascii_hexdigit())
        && name.ends_with(".json")
}
fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    File::open(path)?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().expect("storage path has parent");
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged
        .persist(path)
        .map_err(|e| StorageError::Io(e.error))?;
    sync_directory(parent)
}
fn candidates(paths: &AppPaths) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if paths.backups().exists() {
        for entry in fs::read_dir(paths.backups())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_file()
                && name.starts_with("backup-")
                && name.ends_with(".json")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    Ok(files)
}
impl Store {
    pub fn open(paths: AppPaths) -> Result<Self> {
        fs::create_dir_all(&paths.root)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(paths.root.join("instance.lock"))?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => {
                return Err(StorageError::AlreadyRunning(paths.root.clone()));
            }
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
        let mut store = Self {
            _lock: lock,
            paths,
            recovery_archive: None,
            last_backup_attempt: None,
        };
        let orphaned = store.paths.workspaces().exists()
            && fs::read_dir(store.paths.workspaces())?.next().is_some();
        let inspection = store.load();
        if let Err(error) = inspection {
            if !error.is_corruption() {
                return Err(error);
            }
            store.restore(&error.to_string())?;
        } else if inspection?.is_none() && (!candidates(&store.paths)?.is_empty() || orphaned) {
            store.restore("session manifest missing")?;
        }
        Ok(store)
    }
    pub fn paths(&self) -> &AppPaths {
        &self.paths
    }
    pub fn load(&self) -> Result<Option<Snapshot>> {
        if !self.paths.session().exists() {
            return Ok(None);
        }
        let m = read_manifest(&self.paths.session())?;
        let mut workspaces = Vec::new();
        for name in m.workspaces {
            if !name.is_ascii() || !valid_name(&name) {
                return Err(StorageError::Corrupt("invalid workspace filename".into()));
            }
            let bytes = fs::read(self.paths.workspaces().join(&name)).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    StorageError::Corrupt(format!("missing workspace {name}"))
                } else {
                    e.into()
                }
            })?;
            let w: Workspace = serde_json::from_slice(&bytes)?;
            if workspace_name(&w, &bytes) != name {
                return Err(StorageError::Corrupt(format!(
                    "workspace checksum mismatch: {name}"
                )));
            }
            workspaces.push(w);
        }
        let snapshot = Snapshot {
            document: Document {
                workspaces,
                active_workspace: m.active_workspace,
                next_workspace_number: m.next_workspace_number,
            },
            preferences: m.preferences,
        };
        snapshot.validate()?;
        Ok(Some(snapshot))
    }
    pub fn save(&mut self, document: &Document, preferences: &Preferences) -> Result<()> {
        document.validate().map_err(StorageError::InvalidData)?;
        preferences.validate().map_err(StorageError::InvalidData)?;
        fs::create_dir_all(self.paths.workspaces())?;
        let mut files = Vec::new();
        for workspace in &document.workspaces {
            let bytes = serde_json::to_vec(workspace)?;
            let name = workspace_name(workspace, &bytes);
            let path = self.paths.workspaces().join(&name);
            if !path.exists() {
                atomic_write(&path, &bytes)?;
            } else if fs::read(&path)? != bytes {
                return Err(StorageError::Corrupt(format!(
                    "existing workspace revision is damaged: {name}"
                )));
            }
            files.push(name);
        }
        let manifest = Manifest {
            version: 1,
            workspaces: files,
            active_workspace: document.active_workspace,
            next_workspace_number: document.next_workspace_number,
            preferences: preferences.clone(),
        };
        atomic_write(
            &self.paths.session(),
            &serde_json::to_vec_pretty(&manifest)?,
        )
    }
    pub fn maintain(&self) -> Result<()> {
        let m = read_manifest(&self.paths.session())?;
        let used: HashSet<_> = m.workspaces.into_iter().collect();
        for entry in fs::read_dir(self.paths.workspaces())? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type()?.is_file()
                && name.is_ascii()
                && valid_name(&name)
                && !used.contains(name.as_ref())
            {
                fs::remove_file(entry.path())?;
            }
        }
        sync_directory(&self.paths.workspaces())
    }
    pub fn backup(&mut self, force: bool) -> Result<Option<PathBuf>> {
        if !force
            && self
                .last_backup_attempt
                .is_some_and(|t| t.elapsed() < BACKUP_INTERVAL)
        {
            return Ok(None);
        }
        self.last_backup_attempt = Some(Instant::now());
        let Some(snapshot) = self.load()? else {
            return Ok(None);
        };
        fs::create_dir_all(self.paths.backups())?;
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = self
            .paths
            .backups()
            .join(format!("backup-{millis:020}-{}.json", Uuid::new_v4()));
        atomic_write(
            &path,
            &serde_json::to_vec(&Backup {
                version: 1,
                snapshot,
            })?,
        )?;
        read_backup(&path)?;
        for old in candidates(&self.paths)?.into_iter().skip(BACKUPS_TO_KEEP) {
            fs::remove_file(old)?;
        }
        sync_directory(&self.paths.backups())?;
        Ok(Some(path))
    }
    fn restore(&mut self, reason: &str) -> Result<()> {
        let archive = self
            .paths
            .recovery()
            .join(format!("archive-{}", Uuid::new_v4()));
        fs::create_dir_all(archive.join("workspaces"))?;
        if self.paths.session().exists() {
            fs::copy(self.paths.session(), archive.join("session.json"))?;
            File::open(archive.join("session.json"))?.sync_all()?;
        }
        if self.paths.workspaces().exists() {
            for entry in fs::read_dir(self.paths.workspaces())? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    let path = archive.join("workspaces").join(entry.file_name());
                    fs::copy(entry.path(), &path)?;
                    File::open(path)?.sync_all()?;
                }
            }
        }
        atomic_write(&archive.join("reason.txt"), reason.as_bytes())?;
        sync_directory(&archive.join("workspaces"))?;
        sync_directory(&archive)?;
        sync_directory(&self.paths.recovery())?;
        for candidate in candidates(&self.paths)? {
            let snapshot = match read_backup(&candidate) {
                Ok(snapshot) => snapshot,
                Err(e) if e.is_corruption() || matches!(e, StorageError::UnsupportedVersion(_)) => {
                    continue;
                }
                Err(e) => return Err(e),
            };
            // Damaged immutable revisions are already archived. Publish their exact
            // backup bytes first; the old manifest remains in place until final commit.
            fs::create_dir_all(self.paths.workspaces())?;
            for w in &snapshot.document.workspaces {
                let bytes = serde_json::to_vec(w)?;
                atomic_write(
                    &self.paths.workspaces().join(workspace_name(w, &bytes)),
                    &bytes,
                )?;
            }
            self.save(&snapshot.document, &snapshot.preferences)?;
            atomic_write(
                &archive.join("restored-from.txt"),
                candidate.to_string_lossy().as_bytes(),
            )?;
            self.recovery_archive = Some(archive);
            return Ok(());
        }
        Err(StorageError::NoBackup {
            archive,
            reason: reason.into(),
        })
    }
}
