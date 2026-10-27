//! Deterministic scheduling; no sleeping, I/O or GUI dependencies.
use crate::settings::{DEBOUNCE, MAX_UNSAVED, RETRY_DELAY};
use std::time::Duration;

#[derive(Debug, Default)]
pub struct Autosave {
    revision: u64,
    saved_revision: u64,
    first_dirty: Option<Duration>,
    last_edit: Duration,
    retry_at: Option<Duration>,
    pub error: Option<String>,
}

impl Autosave {
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn dirty(&self) -> bool {
        self.revision != self.saved_revision || self.error.is_some()
    }
    pub fn changed(&mut self, now: Duration) {
        self.revision += 1;
        self.first_dirty.get_or_insert(now);
        self.last_edit = now;
    }
    pub fn deadline(&self) -> Option<Duration> {
        if !self.dirty() {
            return None;
        }
        Some(self.retry_at.unwrap_or_else(|| {
            (self.last_edit + DEBOUNCE)
                .min(self.first_dirty.unwrap_or(self.last_edit) + MAX_UNSAVED)
        }))
    }
    pub fn due(&self, now: Duration) -> bool {
        self.deadline().is_some_and(|at| now >= at)
    }
    pub fn saved(&mut self, revision: u64) {
        self.saved_revision = self.saved_revision.max(revision).min(self.revision);
        self.error = None;
        self.retry_at = None;
        if self.revision == self.saved_revision {
            self.first_dirty = None;
        }
    }
    pub fn failed(&mut self, now: Duration, error: String) {
        self.error = Some(error);
        self.first_dirty.get_or_insert(now);
        self.retry_at = Some(now + RETRY_DELAY);
    }
}
