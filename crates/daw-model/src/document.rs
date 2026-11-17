// SPDX-License-Identifier: GPL-3.0-or-later

//! The open project with its undo history.

use crate::project::Project;

/// How many steps of undo history are kept.
pub const HISTORY_LIMIT: usize = 1000;

/// One undoable step: the project as it was before the step.
#[derive(Clone, Debug)]
struct Step {
    label: String,
    before: Project,
    /// Edits made with [`Document::edit_merged`] and the same key join
    /// this step instead of starting a new one.
    merge: Option<u64>,
}

/// An edit gesture in progress (a drag, for example): every change made
/// until it ends becomes a single undo step.
#[derive(Clone, Debug)]
struct Gesture {
    label: String,
    before: Project,
    revision: u64,
}

/// The project being edited, with its undo and redo history.
///
/// Every change goes through [`Document::edit`], so everything the user
/// does can be undone. The history stores whole snapshots; because a
/// [`Project`] shares its unchanged tracks and patterns between copies,
/// a snapshot costs only what actually changed.
#[derive(Clone, Debug)]
pub struct Document {
    project: Project,
    undo: Vec<Step>,
    redo: Vec<Step>,
    gesture: Option<Gesture>,
    /// Increases with every change, undo and redo.
    revision: u64,
    /// The revision that was last saved, if any.
    saved_revision: Option<u64>,
    /// Settings kept outside the project (a plugin's, inside the plugin)
    /// changed since the last save.
    changed_outside: bool,
    /// How many times settings kept outside the project changed.
    outside_changes: u64,
}

impl Document {
    /// Opens a project with an empty history. It counts as saved.
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: Vec::new(),
            redo: Vec::new(),
            gesture: None,
            revision: 0,
            saved_revision: Some(0),
            changed_outside: false,
            outside_changes: 0,
        }
    }

    /// Opens a project that has not been saved anywhere yet.
    pub fn new_unsaved(project: Project) -> Self {
        Self {
            saved_revision: None,
            ..Self::new(project)
        }
    }

    /// The project as it is now.
    pub fn project(&self) -> &Project {
        &self.project
    }

    /// A number that changes whenever the project changes (including by
    /// undo and redo). The engine recompiles when it changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Changes the project. The change becomes one undo step labelled
    /// `label` (shown to people as "Undo <label>"), unless a gesture is in
    /// progress, in which case it joins the gesture. A change that leaves
    /// the project as it was records nothing.
    pub fn edit<R>(&mut self, label: &str, change: impl FnOnce(&mut Project) -> R) -> R {
        let before = (self.gesture.is_none()).then(|| self.project.clone());
        let result = change(&mut self.project);
        match before {
            Some(before) => {
                if before != self.project {
                    self.revision += 1;
                    self.push_undo(Step {
                        label: label.to_owned(),
                        before,
                        merge: None,
                    });
                    self.redo.clear();
                }
            }
            None => self.revision += 1,
        }
        result
    }

    /// Like [`Document::edit`], but successive edits with the same `key`
    /// become a single undo step, as long as nothing else was done in
    /// between. The step takes the label of the latest edit. Used for things
    /// that arrive piece by piece but are one action for the person, such as
    /// the notes of one recording.
    pub fn edit_merged<R>(
        &mut self,
        label: &str,
        key: u64,
        change: impl FnOnce(&mut Project) -> R,
    ) -> R {
        let joins_last = self.gesture.is_none()
            && self.redo.is_empty()
            && self.undo.last().is_some_and(|step| step.merge == Some(key));
        if !joins_last {
            let depth = self.undo.len();
            let revision = self.revision;
            let result = self.edit(label, change);
            if self.revision != revision
                && self.undo.len() >= depth
                && let Some(step) = self.undo.last_mut()
            {
                step.merge = Some(key);
            }
            return result;
        }
        let before = self.project.clone();
        let result = change(&mut self.project);
        if before != self.project {
            self.revision += 1;
            if let Some(step) = self.undo.last_mut() {
                label.clone_into(&mut step.label);
            }
        }
        result
    }

    /// Starts a gesture: every edit until [`Document::end_gesture`] becomes
    /// one undo step called `label`. Starting a gesture while one is open
    /// ends the open one first.
    pub fn begin_gesture(&mut self, label: &str) {
        self.end_gesture();
        self.gesture = Some(Gesture {
            label: label.to_owned(),
            before: self.project.clone(),
            revision: self.revision,
        });
    }

    /// Ends the gesture in progress, recording one undo step if anything
    /// changed.
    pub fn end_gesture(&mut self) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        if gesture.before != self.project {
            self.revision += 1;
            self.push_undo(Step {
                label: gesture.label,
                before: gesture.before,
                merge: None,
            });
            self.redo.clear();
        } else if self.revision != gesture.revision {
            // Changed and changed back: nothing to undo, but the engine
            // must still see the final state.
            self.revision += 1;
        }
    }

    /// Abandons the gesture in progress and puts the project back as it
    /// was when the gesture started (for Escape during a drag).
    pub fn cancel_gesture(&mut self) {
        if let Some(gesture) = self.gesture.take()
            && gesture.before != self.project
        {
            self.project = gesture.before;
            self.revision += 1;
        }
    }

    /// Whether a gesture is in progress.
    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    fn push_undo(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
    }

    /// Undoes the last step; returns its label.
    pub fn undo(&mut self) -> Option<String> {
        self.end_gesture();
        let step = self.undo.pop()?;
        let after = std::mem::replace(&mut self.project, step.before);
        self.revision += 1;
        let label = step.label.clone();
        self.redo.push(Step {
            label: step.label,
            before: after,
            merge: None,
        });
        Some(label)
    }

    /// Redoes the last undone step; returns its label.
    pub fn redo(&mut self) -> Option<String> {
        self.end_gesture();
        let step = self.redo.pop()?;
        let before = std::mem::replace(&mut self.project, step.before);
        self.revision += 1;
        let label = step.label.clone();
        self.push_undo(Step {
            label: step.label,
            before,
            merge: None,
        });
        Some(label)
    }

    /// The label of the step that undo would revert.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }

    /// The label of the step that redo would restore.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    /// Records that the current state has been saved.
    pub fn mark_saved(&mut self) {
        self.saved_revision = Some(self.revision);
        self.changed_outside = false;
    }

    /// Whether there are changes that have not been saved.
    pub fn is_dirty(&self) -> bool {
        self.changed_outside || self.saved_revision != Some(self.revision)
    }

    /// Settings the project keeps a copy of changed where they live (a
    /// plugin's, in its own editor): the song has unsaved changes, though
    /// the project itself has not changed (and there is nothing to undo).
    pub fn mark_changed_outside(&mut self) {
        self.changed_outside = true;
        self.outside_changes += 1;
    }

    /// Brings copies of outside settings in the project up to date (a
    /// plugin's settings, before saving): not an undo step, and not a
    /// change the engine needs to hear about.
    pub fn refresh(&mut self, change: impl FnOnce(&mut Project)) {
        change(&mut self.project);
    }

    /// A number that changes whenever there is something new to save: the
    /// project changed, or settings it keeps a copy of did.
    pub fn version(&self) -> u64 {
        self.revision + self.outside_changes
    }

    /// How many steps can be undone.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, reason = "test values")]
mod tests {
    use super::*;
    use crate::{DEFAULT_VELOCITY, Note, NoteId, Ticks};

    fn note(key: u8) -> Note {
        Note {
            id: NoteId(0),
            start: Ticks::ZERO,
            length: Ticks::QUARTER,
            key,
            velocity: DEFAULT_VELOCITY,
            pitch_offset: 0.0,
        }
    }

    fn document() -> (Document, crate::PatternId) {
        let project = Project::starter("P", "T");
        let pattern = project.first_pattern().unwrap().1;
        (Document::new(project), pattern)
    }

    fn keys(doc: &Document, pattern: crate::PatternId) -> Vec<u8> {
        doc.project()
            .pattern(pattern)
            .unwrap()
            .notes
            .iter()
            .map(|n| n.key)
            .collect()
    }

    #[test]
    fn edits_undo_and_redo() {
        let (mut doc, pattern) = document();
        doc.edit("Add note", |p| p.add_note(pattern, note(60)));
        doc.edit("Add note", |p| p.add_note(pattern, note(64)));
        assert_eq!(keys(&doc, pattern), [60, 64]);
        assert_eq!(doc.undo().as_deref(), Some("Add note"));
        assert_eq!(keys(&doc, pattern), [60]);
        assert_eq!(doc.redo_label(), Some("Add note"));
        doc.redo();
        assert_eq!(keys(&doc, pattern), [60, 64]);
        doc.undo();
        doc.undo();
        assert_eq!(keys(&doc, pattern), Vec::<u8>::new());
        assert!(doc.undo().is_none(), "nothing left to undo");
    }

    #[test]
    fn a_new_edit_clears_redo() {
        let (mut doc, pattern) = document();
        doc.edit("Add note", |p| p.add_note(pattern, note(60)));
        doc.undo();
        doc.edit("Add note", |p| p.add_note(pattern, note(67)));
        assert!(doc.redo().is_none());
        assert_eq!(keys(&doc, pattern), [67]);
    }

    #[test]
    fn a_gesture_is_one_step() {
        let (mut doc, pattern) = document();
        let id = doc
            .edit("Add note", |p| p.add_note(pattern, note(60)))
            .unwrap();
        doc.begin_gesture("Move note");
        for _ in 0..10 {
            doc.edit("Move note", |p| {
                p.update_notes(pattern, &[id], |n| n.key += 1);
            });
        }
        doc.end_gesture();
        assert_eq!(keys(&doc, pattern), [70]);
        assert_eq!(doc.undo_depth(), 2);
        assert_eq!(doc.undo().as_deref(), Some("Move note"));
        assert_eq!(
            keys(&doc, pattern),
            [60],
            "the whole drag is undone at once"
        );
    }

    #[test]
    fn cancelling_a_gesture_restores_the_project() {
        let (mut doc, pattern) = document();
        let id = doc
            .edit("Add note", |p| p.add_note(pattern, note(60)))
            .unwrap();
        let revision = doc.revision();
        doc.begin_gesture("Move note");
        doc.edit("Move note", |p| {
            p.update_notes(pattern, &[id], |n| n.key = 72);
        });
        doc.cancel_gesture();
        assert_eq!(keys(&doc, pattern), [60]);
        assert_eq!(doc.undo_depth(), 1);
        assert!(
            doc.revision() > revision,
            "the engine must see the restored state"
        );
    }

    #[test]
    fn edits_that_change_nothing_are_not_recorded() {
        let (mut doc, pattern) = document();
        let revision = doc.revision();
        doc.edit("Nothing", |p| p.remove_notes(pattern, &[NoteId(999)]));
        assert_eq!(doc.undo_depth(), 0);
        assert_eq!(doc.revision(), revision);
    }

    #[test]
    fn dirty_tracking_follows_undo() {
        let (mut doc, pattern) = document();
        assert!(!doc.is_dirty());
        doc.edit("Add note", |p| p.add_note(pattern, note(60)));
        assert!(doc.is_dirty());
        doc.mark_saved();
        assert!(!doc.is_dirty());
        doc.undo();
        assert!(
            doc.is_dirty(),
            "undoing past the save makes it differ from the file"
        );
        assert!(Document::new_unsaved(Project::empty()).is_dirty());
    }

    #[test]
    fn merged_edits_are_one_step_until_something_else_happens() {
        let (mut doc, pattern) = document();
        for key in [60, 62, 64] {
            doc.edit_merged("record notes", 7, |p| p.add_note(pattern, note(key)));
        }
        assert_eq!(doc.undo_depth(), 1);
        assert_eq!(doc.undo_label(), Some("record notes"));
        doc.edit("Add note", |p| p.add_note(pattern, note(70)));
        doc.edit_merged("record notes", 7, |p| p.add_note(pattern, note(72)));
        assert_eq!(doc.undo_depth(), 3, "an edit in between starts a new step");
        doc.undo();
        doc.undo();
        doc.undo();
        assert_eq!(keys(&doc, pattern), Vec::<u8>::new());
        doc.redo();
        assert_eq!(keys(&doc, pattern), [60, 62, 64]);
        doc.edit_merged("record notes", 7, |p| p.add_note(pattern, note(74)));
        assert_eq!(
            doc.undo_depth(),
            2,
            "a redone step is closed: later edits do not join it"
        );
    }

    #[test]
    fn history_is_bounded() {
        let (mut doc, pattern) = document();
        for key in 0..(HISTORY_LIMIT + 10) {
            doc.edit("Add note", |p| p.add_note(pattern, note((key % 128) as u8)));
        }
        assert_eq!(doc.undo_depth(), HISTORY_LIMIT);
    }
}
