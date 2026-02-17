//! Config draft management with snapshot undo/redo.
//!
//! Provides a safety layer between the GUI and config persistence.
//! All edits are made to a draft clone; changes are only persisted
//! on explicit commit after validation and user confirmation.

use std::path::{Path, PathBuf};

use claw_config::{ConfigError, OpenClawConfig, ValidationResult, load_config, validate_config, write_config_file};
use tracing::debug;

/// Maximum number of undo snapshots retained.
const MAX_UNDO_DEPTH: usize = 50;

/// Manages a draft copy of [`OpenClawConfig`] with undo/redo support.
///
/// The GUI edits the `draft`; the `live` config reflects what's on disk.
/// Changes are only persisted when [`ConfigManager::commit`] is called.
#[derive(Debug)]
pub struct ConfigManager {
    /// The last-saved config (source of truth on disk).
    live: OpenClawConfig,
    /// Mutable draft that the GUI edits.
    draft: OpenClawConfig,
    /// Undo stack — snapshots of draft before each mutation group.
    undo_stack: Vec<OpenClawConfig>,
    /// Redo stack — snapshots popped during undo.
    redo_stack: Vec<OpenClawConfig>,
    /// Path to the config file on disk.
    config_path: PathBuf,
    /// Last validation result (re-run after each edit).
    last_validation: ValidationResult,
}

impl ConfigManager {
    /// Load a config from disk, initializing both `live` and `draft` to the loaded value.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let config = load_config(path)?;
        let validation = validate_config(&config);
        debug!("loaded config from {}, {} validation issues", path.display(), validation.issues.len());

        Ok(Self {
            draft: config.clone(),
            live: config,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            config_path: path.to_path_buf(),
            last_validation: validation,
        })
    }

    /// Read-only reference to the current draft.
    pub fn draft(&self) -> &OpenClawConfig {
        &self.draft
    }

    /// Mutable reference to the draft for editing.
    ///
    /// Callers should call [`begin_edit`](Self::begin_edit) before a batch
    /// of related edits to snapshot the current state for undo.
    pub fn draft_mut(&mut self) -> &mut OpenClawConfig {
        &mut self.draft
    }

    /// Read-only reference to the live (on-disk) config.
    pub fn live(&self) -> &OpenClawConfig {
        &self.live
    }

    /// Returns `true` if the draft differs from the live config.
    ///
    /// Comparison is done on the serialized JSON form so that field ordering
    /// and `Option::None` vs absent-key differences are normalized.
    pub fn is_dirty(&self) -> bool {
        let draft_val = serde_json::to_value(&self.draft);
        let live_val = serde_json::to_value(&self.live);
        match (draft_val, live_val) {
            (Ok(d), Ok(l)) => d != l,
            // If serialization fails, assume dirty to be safe.
            _ => true,
        }
    }

    /// Snapshot the current draft onto the undo stack before a batch of edits.
    ///
    /// This clears the redo stack (new edits invalidate the redo timeline)
    /// and caps the undo stack at [`MAX_UNDO_DEPTH`].
    pub fn begin_edit(&mut self) {
        self.push_undo();
    }

    /// Whether an undo operation is available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether a redo operation is available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Undo the last edit group, restoring the previous draft snapshot.
    ///
    /// Returns `true` if an undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo_stack.pop() {
            self.redo_stack.push(self.draft.clone());
            self.draft = prev;
            debug!("undo: {} remaining on undo stack", self.undo_stack.len());
            true
        } else {
            false
        }
    }

    /// Redo a previously undone edit group.
    ///
    /// Returns `true` if a redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(self.draft.clone());
            self.draft = next;
            debug!("redo: {} remaining on redo stack", self.redo_stack.len());
            true
        } else {
            false
        }
    }

    /// Run validation on the current draft and cache the result.
    pub fn validate(&mut self) -> &ValidationResult {
        self.last_validation = validate_config(&self.draft);
        &self.last_validation
    }

    /// Persist the draft to disk and promote it to the live config.
    ///
    /// Clears both undo and redo stacks since the new baseline is established.
    pub fn commit(&mut self) -> Result<(), ConfigError> {
        write_config_file(&self.config_path, &self.draft)?;
        self.live = self.draft.clone();
        self.undo_stack.clear();
        self.redo_stack.clear();
        debug!("committed config to {}", self.config_path.display());
        Ok(())
    }

    /// Discard all draft edits, resetting to the live config.
    pub fn discard(&mut self) {
        self.draft = self.live.clone();
        self.undo_stack.clear();
        self.redo_stack.clear();
        debug!("discarded draft changes");
    }

    /// Path to the config file on disk.
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    // -- Internal helpers --------------------------------------------------

    /// Push the current draft onto the undo stack.
    fn push_undo(&mut self) {
        self.redo_stack.clear();
        self.undo_stack.push(self.draft.clone());
        if self.undo_stack.len() > MAX_UNDO_DEPTH {
            self.undo_stack.remove(0);
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Helper: write a minimal config file and return a loaded `ConfigManager`.
    fn setup_manager(content: &str) -> (TempDir, ConfigManager) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("config.json5");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        let mgr = ConfigManager::load(&path).unwrap();
        (dir, mgr)
    }

    #[test]
    fn load_default_config_is_not_dirty() {
        let (_dir, mgr) = setup_manager("{}");
        assert!(!mgr.is_dirty(), "freshly loaded config should not be dirty");
        assert!(!mgr.can_undo());
        assert!(!mgr.can_redo());
    }

    #[test]
    fn edit_makes_dirty_and_undo_restores() {
        let (_dir, mut mgr) = setup_manager(r#"{ "gateway": { "port": 3000 } }"#);
        assert!(!mgr.is_dirty());

        // Make an edit
        mgr.begin_edit();
        mgr.draft_mut().gateway.as_mut().unwrap().port = Some(9999);
        assert!(mgr.is_dirty(), "draft should differ from live after edit");
        assert!(mgr.can_undo());
        assert!(!mgr.can_redo());

        // Undo should restore
        assert!(mgr.undo());
        assert!(!mgr.is_dirty(), "undo should restore to live state");
        assert!(mgr.can_redo());

        // Redo should re-apply
        assert!(mgr.redo());
        assert!(mgr.is_dirty(), "redo should re-apply the edit");
        assert_eq!(mgr.draft().gateway.as_ref().unwrap().port, Some(9999));
    }

    #[test]
    fn begin_edit_and_discard_resets() {
        let (_dir, mut mgr) = setup_manager(r#"{ "gateway": { "port": 3000 } }"#);

        mgr.begin_edit();
        mgr.draft_mut().gateway.as_mut().unwrap().port = Some(1234);
        assert!(mgr.is_dirty());

        mgr.discard();
        assert!(!mgr.is_dirty(), "discard should reset draft to live");
        assert!(!mgr.can_undo(), "discard should clear undo stack");
        assert!(!mgr.can_redo(), "discard should clear redo stack");
        assert_eq!(mgr.draft().gateway.as_ref().unwrap().port, Some(3000));
    }

    #[test]
    fn commit_persists_and_clears_stacks() {
        let (dir, mut mgr) = setup_manager(r#"{ "gateway": { "port": 3000 } }"#);

        mgr.begin_edit();
        mgr.draft_mut().gateway.as_mut().unwrap().port = Some(5000);
        assert!(mgr.is_dirty());

        mgr.commit().unwrap();
        assert!(!mgr.is_dirty(), "commit should make draft == live");
        assert!(!mgr.can_undo(), "commit should clear undo stack");

        // Verify persisted to disk
        let reloaded = load_config(&dir.path().join("config.json5")).unwrap();
        assert_eq!(reloaded.gateway.unwrap().port, Some(5000));
    }

    #[test]
    fn undo_stack_caps_at_max_depth() {
        let (_dir, mut mgr) = setup_manager("{}");

        for i in 0..(MAX_UNDO_DEPTH + 10) {
            mgr.begin_edit();
            // Each edit is a no-op on the config, but we're testing stack depth.
            let _ = i;
        }

        assert_eq!(mgr.undo_stack.len(), MAX_UNDO_DEPTH);
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let (_dir, mut mgr) = setup_manager(r#"{ "gateway": { "port": 3000 } }"#);

        mgr.begin_edit();
        mgr.draft_mut().gateway.as_mut().unwrap().port = Some(4000);

        mgr.undo();
        assert!(mgr.can_redo());

        // New edit should clear redo
        mgr.begin_edit();
        mgr.draft_mut().gateway.as_mut().unwrap().port = Some(5000);
        assert!(!mgr.can_redo(), "new edit should invalidate redo stack");
    }

    #[test]
    fn validate_reports_issues() {
        let (_dir, mut mgr) = setup_manager(r#"{ "gateway": { "port": 0 } }"#);
        let result = mgr.validate();
        assert!(result.has_errors(), "port 0 should produce a validation error");
    }
}
