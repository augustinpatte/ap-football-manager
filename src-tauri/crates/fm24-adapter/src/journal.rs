//! Everything the adapter decides that is not a plain copy is written down here: exclusions,
//! generated or estimated values, fallbacks. Nothing is dropped or invented silently.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct Note {
    pub severity: Severity,
    /// Stable machine code, e.g. `player.nation_missing`.
    pub code: &'static str,
    /// Entity id (`fm-p-123`) or empty.
    pub entity: String,
    pub message: String,
}

#[derive(Debug, Default, Clone)]
pub struct Journal {
    notes: Vec<Note>,
}

impl Journal {
    pub fn add(
        &mut self,
        severity: Severity,
        code: &'static str,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.notes.push(Note {
            severity,
            code,
            entity: entity.into(),
            message: message.into(),
        });
    }

    pub fn info(
        &mut self,
        code: &'static str,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.add(Severity::Info, code, entity, message);
    }

    pub fn warn(
        &mut self,
        code: &'static str,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.add(Severity::Warning, code, entity, message);
    }

    pub fn error(
        &mut self,
        code: &'static str,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.add(Severity::Error, code, entity, message);
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn errors(&self) -> impl Iterator<Item = &Note> {
        self.notes.iter().filter(|n| n.severity == Severity::Error)
    }

    /// Number of notes per code, in code order (stable report).
    pub fn counts(&self) -> BTreeMap<(&'static str, Severity), usize> {
        let mut map = BTreeMap::new();
        for note in &self.notes {
            *map.entry((note.code, note.severity)).or_insert(0) += 1;
        }
        map
    }

    pub fn count(&self, code: &str) -> usize {
        self.notes.iter().filter(|n| n.code == code).count()
    }
}
