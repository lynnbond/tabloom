pub mod document;
// GUI-independent document state. Vec order is the canonical tab/region order.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub const MIN_REGION_HEIGHT: f32 = 120.0;
pub const MAX_REGION_HEIGHT: f32 = 2000.0;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextEncoding {
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileState {
    pub encoding: TextEncoding,
    pub disk_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorRegion {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub title: String,
    pub text: String,
    pub syntax_id: String,
    /// Logical pixels, independent of display DPI. The outer view scrolls on overflow.
    pub height: f32,
    pub file_path: Option<PathBuf>,
    #[serde(default)]
    pub file_state: Option<FileState>,
    pub modified: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl EditorRegion {
    pub fn new(workspace_id: Uuid, title: impl Into<String>) -> Self {
        let time = now_ms();
        Self {
            id: Uuid::new_v4(),
            workspace_id,
            title: title.into(),
            text: String::new(),
            syntax_id: "plain".into(),
            height: 240.0,
            file_path: None,
            file_state: None,
            modified: false,
            created_at: time,
            updated_at: time,
        }
    }

    pub fn touch(&mut self) {
        self.modified = true;
        self.updated_at = now_ms();
    }

    pub fn resize(&mut self, height: f32) {
        if height.is_finite() {
            self.height = height.clamp(MIN_REGION_HEIGHT, MAX_REGION_HEIGHT);
            self.touch();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub title: String,
    pub regions: Vec<EditorRegion>,
    pub active_region: Uuid,
    pub status: WorkspaceStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub closed_at: Option<i64>,
    pub(crate) next_region_number: usize,
}

impl Workspace {
    pub fn new(title: impl Into<String>, region_prefix: &str) -> Self {
        let id = Uuid::new_v4();
        let region = EditorRegion::new(id, format!("{region_prefix} 1"));
        let time = now_ms();
        Self {
            id,
            title: title.into(),
            active_region: region.id,
            regions: vec![region],
            status: WorkspaceStatus::Open,
            created_at: time,
            updated_at: time,
            closed_at: None,
            next_region_number: 2,
        }
    }

    pub fn add_region(&mut self, prefix: &str) -> Uuid {
        let region = EditorRegion::new(self.id, format!("{prefix} {}", self.next_region_number));
        self.next_region_number += 1;
        self.active_region = region.id;
        self.regions.push(region);
        self.touch();
        self.active_region
    }

    pub fn remove_region(&mut self, id: Uuid, prefix: &str) -> bool {
        let Some(index) = self.regions.iter().position(|region| region.id == id) else {
            return false;
        };
        self.regions.remove(index);
        if self.regions.is_empty() {
            self.add_region(prefix);
        } else if self.active_region == id {
            self.active_region = self.regions[index.min(self.regions.len() - 1)].id;
        }
        self.touch();
        true
    }

    pub fn move_region(&mut self, id: Uuid, offset: isize) -> bool {
        let Some(index) = self.regions.iter().position(|region| region.id == id) else {
            return false;
        };
        let Some(target) = index.checked_add_signed(offset) else {
            return false;
        };
        if target >= self.regions.len() || target == index {
            return false;
        }
        self.regions.swap(index, target);
        self.touch();
        true
    }

    pub fn touch(&mut self) {
        self.updated_at = now_ms();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: Option<Uuid>,
    pub(crate) next_workspace_number: usize,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            workspaces: Vec::new(),
            active_workspace: None,
            next_workspace_number: 1,
        }
    }
}

impl Document {
    /// Reject malformed persisted state before any write can replace valid data.
    pub fn validate(&self) -> Result<(), String> {
        use std::collections::HashSet;
        let mut workspace_ids = HashSet::new();
        let mut region_ids = HashSet::new();
        if self.next_workspace_number == 0 || self.next_workspace_number >= i64::MAX as usize {
            return Err("invalid workspace counter".into());
        }
        for w in &self.workspaces {
            if !workspace_ids.insert(w.id)
                || w.regions.is_empty()
                || w.next_region_number == 0
                || w.next_region_number >= i64::MAX as usize
            {
                return Err("duplicate workspace or empty region list".into());
            }
            if w.created_at < 0 || w.updated_at < 0 || w.closed_at.is_some_and(|time| time < 0) {
                return Err("invalid workspace timestamp".into());
            }
            if (w.status == WorkspaceStatus::Closed) != w.closed_at.is_some() {
                return Err("workspace close state mismatch".into());
            }
            if !w.regions.iter().any(|r| r.id == w.active_region) {
                return Err("active region does not exist".into());
            }
            for r in &w.regions {
                if r.file_state.is_some() && r.file_path.is_none() {
                    return Err("file baseline has no associated path".into());
                }
                if r.created_at < 0 || r.updated_at < 0 {
                    return Err("invalid region timestamp".into());
                }
                if !region_ids.insert(r.id) || r.workspace_id != w.id {
                    return Err("region identity or ownership mismatch".into());
                }
                if !r.height.is_finite()
                    || !(MIN_REGION_HEIGHT..=MAX_REGION_HEIGHT).contains(&r.height)
                {
                    return Err("invalid region height".into());
                }
            }
        }
        match self.active_workspace {
            Some(id)
                if self
                    .workspaces
                    .iter()
                    .any(|w| w.id == id && w.status == WorkspaceStatus::Open) =>
            {
                Ok(())
            }
            None if self
                .workspaces
                .iter()
                .all(|w| w.status == WorkspaceStatus::Closed) =>
            {
                Ok(())
            }
            _ => Err("invalid active workspace".into()),
        }
    }

    pub fn delete_closed(&mut self, id: Uuid) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|w| w.id == id && w.status == WorkspaceStatus::Closed)
        else {
            return false;
        };
        self.workspaces.remove(index);
        true
    }

    pub fn create_workspace(&mut self, prefix: &str, region_prefix: &str) -> Uuid {
        let workspace = Workspace::new(
            format!("{prefix} {}", self.next_workspace_number),
            region_prefix,
        );
        self.next_workspace_number += 1;
        let id = workspace.id;
        self.workspaces.push(workspace);
        self.active_workspace = Some(id);
        id
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.workspaces
            .iter()
            .find(|w| Some(w.id) == self.active_workspace && w.status == WorkspaceStatus::Open)
    }

    pub fn active_mut(&mut self) -> Option<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|w| Some(w.id) == self.active_workspace && w.status == WorkspaceStatus::Open)
    }

    pub fn select(&mut self, id: Uuid) -> bool {
        if self
            .workspaces
            .iter()
            .any(|w| w.id == id && w.status == WorkspaceStatus::Open)
        {
            self.active_workspace = Some(id);
            return true;
        }
        false
    }

    pub fn cycle(&mut self, offset: isize) {
        let open: Vec<_> = self
            .workspaces
            .iter()
            .filter(|w| w.status == WorkspaceStatus::Open)
            .map(|w| w.id)
            .collect();
        if open.is_empty() {
            return;
        }
        let index = open
            .iter()
            .position(|id| Some(*id) == self.active_workspace)
            .unwrap_or(0);
        self.active_workspace =
            Some(open[(index as isize + offset).rem_euclid(open.len() as isize) as usize]);
    }

    pub fn close(&mut self, id: Uuid) -> bool {
        let Some(index) = self
            .workspaces
            .iter()
            .position(|w| w.id == id && w.status == WorkspaceStatus::Open)
        else {
            return false;
        };
        let workspace = &mut self.workspaces[index];
        workspace.status = WorkspaceStatus::Closed;
        workspace.closed_at = Some(now_ms());
        workspace.touch();
        if self.active_workspace == Some(id) {
            self.active_workspace = self.workspaces[..index]
                .iter()
                .rev()
                .chain(self.workspaces[index + 1..].iter())
                .find(|w| w.status == WorkspaceStatus::Open)
                .map(|w| w.id);
        }
        true
    }

    pub fn reopen(&mut self, id: Uuid) -> bool {
        let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|w| w.id == id && w.status == WorkspaceStatus::Closed)
        else {
            return false;
        };
        workspace.status = WorkspaceStatus::Open;
        workspace.closed_at = None;
        workspace.touch();
        self.active_workspace = Some(id);
        true
    }
}
