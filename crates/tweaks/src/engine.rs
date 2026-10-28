//! Applying, checking and undoing changes through the journal.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use platform::parameters::ParameterError;
use platform::registry::{RawValue, RegistryError};

pub use crate::journal::{Before, EntryId, SessionId, Target};
use crate::journal::{Event, Journal, JournalError};
use crate::parameters::Parameters;
use crate::registry::Registry;
use crate::value::Value;

/// One change a tweak asks for: set a value, or remove it (`None`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub target: Target,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Written down, not confirmed: the process stopped mid-change. Resolved
    /// when the engine opens.
    Pending,
    Applied,
    Failed(String),
    Reverted,
}

/// A change as the journal knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub id: EntryId,
    pub session: SessionId,
    /// Seconds since the Unix epoch.
    pub at: u64,
    pub tweak: String,
    pub target: Target,
    pub before: Before,
    pub after: Option<Value>,
    pub state: State,
}

/// What [`Engine::apply`] did with one change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(EntryId),
    /// The value already was what the change asks for, so nothing was written
    /// and there is nothing to undo.
    AlreadySet,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevertOutcome {
    Reverted,
    /// Something else has changed the value since: undoing now would throw
    /// that away, so it was left alone. Revert with `force` to undo anyway.
    ChangedSince {
        current: Option<Value>,
    },
    /// The entry is not in effect: it failed, or was already undone.
    NotApplied,
}

/// An applied change whose value is no longer what it set: Windows or another
/// program has changed it since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drift {
    pub entry: EntryId,
    pub target: Target,
    pub expected: Option<Value>,
    pub current: Option<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Journal(#[from] JournalError),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Parameter(#[from] ParameterError),
    /// A system parameter is on or off; anything else cannot be written to it.
    #[error("{target} is 0 or 1, not {value:?}")]
    NotAFlag {
        target: Target,
        value: Option<Value>,
    },
    /// The registry accepted the write, but reading it back shows something
    /// else.
    #[error("{target}: read back {read_back:?} after writing it")]
    NotTaken {
        target: Target,
        read_back: Option<Value>,
    },
    #[error("no journal entry {0:?}")]
    UnknownEntry(EntryId),
}

/// A set of changes that did not go through.
#[derive(Debug, thiserror::Error)]
#[error("{target}: {cause}")]
pub struct ApplyError {
    pub target: Target,
    pub cause: Box<EngineError>,
    /// Whether the changes of the set that had already gone through were
    /// undone. False only if that undo failed too; the journal then still
    /// lists them as applied, so they can be undone later.
    pub rolled_back: bool,
}

impl ApplyError {
    /// Windows refused the change outright, as it does for keys it protects.
    pub fn access_denied(&self) -> bool {
        matches!(
            *self.cause,
            EngineError::Registry(RegistryError::AccessDenied(_))
        )
    }
}

pub struct Engine<R: Registry + Parameters> {
    registry: R,
    journal: Journal,
    entries: Vec<Entry>,
}

impl<R: Registry + Parameters> Engine<R> {
    /// Opens the journal at `journal_path` and settles any change a crash left
    /// unconfirmed, by looking at what the registry holds now.
    pub fn open(registry: R, journal_path: &Path) -> Result<Self, EngineError> {
        let (journal, events) = Journal::open(journal_path)?;
        let mut engine = Engine {
            registry,
            journal,
            entries: fold(events),
        };
        let pending: Vec<EntryId> = engine
            .entries
            .iter()
            .filter(|entry| entry.state == State::Pending)
            .map(|entry| entry.id)
            .collect();
        for id in pending {
            engine.settle(id)?;
        }
        Ok(engine)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn registry(&self) -> &R {
        &self.registry
    }

    /// What `target` holds now.
    pub fn current(&self, target: &Target) -> Result<Option<Value>, EngineError> {
        Ok(self.read(target)?.as_ref().map(Value::from_raw))
    }

    /// Applies `changes` for `tweak` as one session: each is written down,
    /// made, read back and compared. If one fails, the ones before it are
    /// undone, so a set takes effect whole or not at all.
    pub fn apply(
        &mut self,
        tweak: &str,
        changes: &[Change],
    ) -> Result<(SessionId, Vec<Outcome>), ApplyError> {
        let session = SessionId(
            self.entries
                .iter()
                .map(|entry| entry.session.0)
                .max()
                .unwrap_or(0)
                + 1,
        );
        let mut outcomes = Vec::new();
        for change in changes {
            match self.apply_one(session, tweak, change) {
                Ok(outcome) => outcomes.push(outcome),
                Err(cause) => {
                    let made: Vec<EntryId> = outcomes
                        .iter()
                        .filter_map(|outcome| match outcome {
                            Outcome::Applied(id) => Some(*id),
                            Outcome::AlreadySet => None,
                        })
                        .collect();
                    // Every one gets its undo, even after one of them fails.
                    let mut rolled_back = true;
                    for id in made.iter().rev() {
                        if self.revert(*id, true).is_err() {
                            rolled_back = false;
                        }
                    }
                    return Err(ApplyError {
                        target: change.target.clone(),
                        cause: Box::new(cause),
                        rolled_back,
                    });
                }
            }
        }
        Ok((session, outcomes))
    }

    /// Undoes one change. Unless `force`, a value something else has changed
    /// since is left alone and reported instead. A value that is already what
    /// it was before the change counts as undone, whoever put it back: setting
    /// a shell setting through the shell, as one tweak's change does, writes
    /// the registry value of another change of the same tweak as well.
    pub fn revert(&mut self, id: EntryId, force: bool) -> Result<RevertOutcome, EngineError> {
        let entry = self.entry(id)?.clone();
        if entry.state != State::Applied {
            return Ok(RevertOutcome::NotApplied);
        }
        let current = self.read(&entry.target)?;
        let back_already = same(current.as_ref(), raw(&entry.before.value).as_ref());
        if !force && !back_already && !same(current.as_ref(), raw(&entry.after).as_ref()) {
            return Ok(RevertOutcome::ChangedSince {
                current: current.as_ref().map(Value::from_raw),
            });
        }
        self.restore(&entry)?;
        let read_back = self.read(&entry.target)?;
        if !same(read_back.as_ref(), raw(&entry.before.value).as_ref()) {
            return Err(EngineError::NotTaken {
                target: entry.target.clone(),
                read_back: read_back.as_ref().map(Value::from_raw),
            });
        }
        self.record(id, State::Reverted)?;
        Ok(RevertOutcome::Reverted)
    }

    /// Undoes every change of `session`, newest first.
    pub fn revert_session(
        &mut self,
        session: SessionId,
        force: bool,
    ) -> Result<Vec<(EntryId, RevertOutcome)>, EngineError> {
        let ids: Vec<EntryId> = self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.session == session && entry.state == State::Applied)
            .map(|entry| entry.id)
            .collect();
        self.revert_each(ids, force)
    }

    /// Undoes every change still in effect, newest first.
    pub fn revert_all(
        &mut self,
        force: bool,
    ) -> Result<Vec<(EntryId, RevertOutcome)>, EngineError> {
        let ids: Vec<EntryId> = self
            .entries
            .iter()
            .rev()
            .filter(|entry| entry.state == State::Applied)
            .map(|entry| entry.id)
            .collect();
        self.revert_each(ids, force)
    }

    /// Applied changes whose value is no longer what they set. Of several
    /// changes to the same value only the newest counts.
    pub fn drift(&self) -> Result<Vec<Drift>, EngineError> {
        let mut seen = Vec::new();
        let mut drift = Vec::new();
        for entry in self.entries.iter().rev() {
            if entry.state != State::Applied || seen.contains(&&entry.target) {
                continue;
            }
            seen.push(&entry.target);
            let current = self.read(&entry.target)?;
            if !same(current.as_ref(), raw(&entry.after).as_ref()) {
                drift.push(Drift {
                    entry: entry.id,
                    target: entry.target.clone(),
                    expected: entry.after.clone(),
                    current: current.as_ref().map(Value::from_raw),
                });
            }
        }
        drift.reverse();
        Ok(drift)
    }

    fn revert_each(
        &mut self,
        ids: Vec<EntryId>,
        force: bool,
    ) -> Result<Vec<(EntryId, RevertOutcome)>, EngineError> {
        ids.into_iter()
            .map(|id| Ok((id, self.revert(id, force)?)))
            .collect()
    }

    fn apply_one(
        &mut self,
        session: SessionId,
        tweak: &str,
        change: &Change,
    ) -> Result<Outcome, EngineError> {
        let current = self.read(&change.target)?;
        let desired = raw(&change.value);
        if same(current.as_ref(), desired.as_ref()) {
            return Ok(Outcome::AlreadySet);
        }
        let created_key = match (&change.target, &desired) {
            (Target::Registry { path, .. }, Some(_)) => self.outermost_missing_key(path)?,
            _ => None,
        };
        let entry = Entry {
            id: EntryId(self.entries.last().map_or(0, |entry| entry.id.0) + 1),
            session,
            at: now(),
            tweak: tweak.to_owned(),
            target: change.target.clone(),
            before: Before {
                value: current.as_ref().map(Value::from_raw),
                created_key,
            },
            after: change.value.clone(),
            state: State::Pending,
        };
        self.journal.append(&Event::Intent {
            entry: entry.id,
            session,
            at: entry.at,
            tweak: entry.tweak.clone(),
            target: entry.target.clone(),
            before: entry.before.clone(),
            after: entry.after.clone(),
        })?;
        self.entries.push(entry.clone());

        if let Err(error) = self.write(&entry.target, desired.as_ref()) {
            // Nothing should have changed, but a key may have been created on
            // the way to the refused value.
            self.give_up(&entry, error.to_string())?;
            return Err(error);
        }
        let read_back = self.read(&entry.target)?;
        if !same(read_back.as_ref(), desired.as_ref()) {
            // Windows took the write and kept its own value, or put it back.
            let read_back = read_back.as_ref().map(Value::from_raw);
            self.give_up(&entry, format!("read back {read_back:?} after writing it"))?;
            return Err(EngineError::NotTaken {
                target: entry.target.clone(),
                read_back,
            });
        }
        self.record(entry.id, State::Applied)?;
        Ok(Outcome::Applied(entry.id))
    }

    /// Puts back what was there before a change that did not take and records
    /// it as failed, saying so when putting it back failed too.
    fn give_up(&mut self, entry: &Entry, reason: String) -> Result<(), EngineError> {
        let reason = match self.restore(entry) {
            Ok(()) => reason,
            Err(error) => format!("{reason}; putting the old value back failed too: {error}"),
        };
        self.record(entry.id, State::Failed(reason))
    }

    /// Resolves a change the journal has no outcome for: the process stopped
    /// between writing it down and confirming it.
    fn settle(&mut self, id: EntryId) -> Result<(), EngineError> {
        let entry = self.entry(id)?.clone();
        let current = self.read(&entry.target)?;
        if same(current.as_ref(), raw(&entry.after).as_ref()) {
            return self.record(id, State::Applied);
        }
        let reason = if same(current.as_ref(), raw(&entry.before.value).as_ref()) {
            // Only a key created on the way can be left over.
            self.restore(&entry)?;
            "interrupted before the change was made".to_owned()
        } else {
            format!(
                "interrupted; the value is now {:?} and was left alone",
                current.as_ref().map(Value::from_raw)
            )
        };
        self.record(id, State::Failed(reason))
    }

    /// Puts back what was there before `entry`, removing the keys it created
    /// once they hold nothing else.
    fn restore(&mut self, entry: &Entry) -> Result<(), EngineError> {
        self.write(&entry.target, raw(&entry.before.value).as_ref())?;
        let (Target::Registry { path, .. }, Some(created)) =
            (&entry.target, &entry.before.created_key)
        else {
            return Ok(());
        };
        let mut key = path.clone();
        loop {
            if !self.registry.remove_key_if_empty(&key)? || key.eq_ignore_ascii_case(created) {
                return Ok(());
            }
            match key.rfind('\\') {
                Some(cut) => key.truncate(cut),
                None => return Ok(()),
            }
        }
    }

    /// The first key along `path` that does not exist yet, if any.
    fn outermost_missing_key(&self, path: &str) -> Result<Option<String>, RegistryError> {
        let mut prefix = String::new();
        for part in path.split('\\') {
            if !prefix.is_empty() {
                prefix.push('\\');
            }
            prefix.push_str(part);
            if !self.registry.key_exists(&prefix)? {
                return Ok(Some(prefix));
            }
        }
        Ok(None)
    }

    fn read(&self, target: &Target) -> Result<Option<RawValue>, EngineError> {
        read(&self.registry, target)
    }

    fn write(&mut self, target: &Target, value: Option<&RawValue>) -> Result<(), EngineError> {
        match (target, value) {
            (Target::Registry { path, name }, Some(value)) => {
                self.registry.write(path, name, value)?
            }
            (Target::Registry { path, name }, None) => self.registry.delete(path, name)?,
            (Target::Parameter { name }, value) => {
                let on = match value.map(Value::from_raw) {
                    Some(Value::Dword(0)) => false,
                    Some(Value::Dword(1)) => true,
                    value => {
                        return Err(EngineError::NotAFlag {
                            target: target.clone(),
                            value,
                        });
                    }
                };
                self.registry.set(*name, on)?
            }
        }
        Ok(())
    }

    fn entry(&self, id: EntryId) -> Result<&Entry, EngineError> {
        self.entries
            .iter()
            .find(|entry| entry.id == id)
            .ok_or(EngineError::UnknownEntry(id))
    }

    /// Writes the outcome of entry `id` to the journal, then to memory.
    fn record(&mut self, id: EntryId, state: State) -> Result<(), EngineError> {
        let at = now();
        let event = match &state {
            State::Applied => Event::Applied { entry: id, at },
            State::Reverted => Event::Reverted { entry: id, at },
            State::Failed(reason) => Event::Failed {
                entry: id,
                at,
                reason: reason.clone(),
            },
            State::Pending => return Ok(()),
        };
        self.journal.append(&event)?;
        if let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) {
            entry.state = state;
        }
        Ok(())
    }
}

/// What `target` holds now. A system parameter reads as DWORD 1 when on and 0
/// when off, so it compares and goes into the journal like a registry value.
pub(crate) fn read(
    system: &(impl Registry + Parameters),
    target: &Target,
) -> Result<Option<RawValue>, EngineError> {
    Ok(match target {
        Target::Registry { path, name } => system.read(path, name)?,
        Target::Parameter { name } => Some(Value::Dword(system.get(*name)?.into()).to_raw()),
    })
}

/// Replays the journal's events into entries.
fn fold(events: Vec<Event>) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for event in events {
        let (id, state) = match event {
            Event::Intent {
                entry,
                session,
                at,
                tweak,
                target,
                before,
                after,
            } => {
                entries.push(Entry {
                    id: entry,
                    session,
                    at,
                    tweak,
                    target,
                    before,
                    after,
                    state: State::Pending,
                });
                continue;
            }
            Event::Applied { entry, .. } => (entry, State::Applied),
            Event::Failed { entry, reason, .. } => (entry, State::Failed(reason)),
            Event::Reverted { entry, .. } => (entry, State::Reverted),
        };
        if let Some(entry) = entries.iter_mut().find(|entry| entry.id == id) {
            entry.state = state;
        }
    }
    entries
}

fn raw(value: &Option<Value>) -> Option<RawValue> {
    value.as_ref().map(Value::to_raw)
}

fn same(left: Option<&RawValue>, right: Option<&RawValue>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.same_as(right),
        _ => false,
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fake::FakeRegistry;

    const KEY: &str = r"Software\Test\Explorer";

    fn target(name: &str) -> Target {
        Target::Registry {
            path: KEY.into(),
            name: name.into(),
        }
    }

    fn set(name: &str, value: u32) -> Change {
        Change {
            target: target(name),
            value: Some(Value::Dword(value)),
        }
    }

    fn dword(value: u32) -> RawValue {
        Value::Dword(value).to_raw()
    }

    struct Setup {
        _dir: tempfile::TempDir,
        path: std::path::PathBuf,
    }

    impl Setup {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("journal.jsonl");
            Setup { _dir: dir, path }
        }

        fn engine(&self, registry: FakeRegistry) -> Engine<FakeRegistry> {
            Engine::open(registry, &self.path).unwrap()
        }
    }

    fn existing() -> FakeRegistry {
        FakeRegistry::default().with(KEY, "Shown", dword(0))
    }

    mod apply {
        use super::*;

        #[test]
        fn writes_the_new_value() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();

            assert_eq!(engine.registry().get(KEY, "Shown"), Some(&dword(1)));
        }

        #[test]
        fn records_what_was_there_before() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();

            assert_eq!(engine.entries()[0].before.value, Some(Value::Dword(0)));
        }

        #[test]
        fn records_that_the_value_did_not_exist() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("New", 1)]).unwrap();

            assert_eq!(engine.entries()[0].before.value, None);
        }

        #[test]
        fn skips_a_value_that_is_already_set() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            let (_, outcomes) = engine.apply("t", &[set("Shown", 0)]).unwrap();

            assert_eq!(outcomes, vec![Outcome::AlreadySet]);
        }

        #[test]
        fn survives_reopening_the_journal() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            let registry = std::mem::take(&mut engine.registry);
            drop(engine);

            let reopened = setup.engine(registry);
            assert_eq!(reopened.entries()[0].state, State::Applied);
        }

        #[test]
        fn reports_access_denied() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing().deny(KEY));
            let error = engine.apply("t", &[set("Shown", 1)]).unwrap_err();

            assert!(error.access_denied());
        }

        #[test]
        fn undoes_the_set_when_a_later_change_is_refused() {
            let setup = Setup::new();
            let registry = existing().deny(r"Software\Test\Locked");
            let mut engine = setup.engine(registry);
            let locked = Change {
                target: Target::Registry {
                    path: r"Software\Test\Locked".into(),
                    name: "V".into(),
                },
                value: Some(Value::Dword(1)),
            };
            let error = engine.apply("t", &[set("Shown", 1), locked]).unwrap_err();

            assert_eq!(
                (error.rolled_back, engine.registry().get(KEY, "Shown")),
                (true, Some(&dword(0)))
            );
        }

        #[test]
        fn notices_a_write_windows_did_not_keep() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing().ignore(KEY));
            let error = engine.apply("t", &[set("Shown", 1)]).unwrap_err();

            assert!(matches!(*error.cause, EngineError::NotTaken { .. }));
        }

        #[test]
        fn marks_a_refused_change_failed() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing().deny(KEY));
            let _ = engine.apply("t", &[set("Shown", 1)]);

            assert!(matches!(engine.entries()[0].state, State::Failed(_)));
        }
    }

    mod revert {
        use super::*;

        #[test]
        fn puts_back_the_old_value() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert_eq!(engine.registry().get(KEY, "Shown"), Some(&dword(0)));
        }

        #[test]
        fn removes_a_value_that_did_not_exist() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("New", 1)]).unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert_eq!(engine.registry().get(KEY, "New"), None);
        }

        #[test]
        fn removes_the_keys_the_change_created() {
            let setup = Setup::new();
            let mut engine =
                setup.engine(FakeRegistry::default().with("Software", "Other", dword(0)));
            engine.apply("t", &[set("New", 1)]).unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert!(!engine.registry().has_key(r"Software\Test"));
        }

        #[test]
        fn keeps_a_created_key_something_else_moved_into() {
            let setup = Setup::new();
            let mut engine =
                setup.engine(FakeRegistry::default().with("Software", "Other", dword(0)));
            engine.apply("t", &[set("New", 1)]).unwrap();
            engine
                .registry
                .write(KEY, "SomeoneElse", &dword(5))
                .unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert!(engine.registry().has_key(KEY));
        }

        #[test]
        fn leaves_alone_a_value_changed_since() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.registry.write(KEY, "Shown", &dword(7)).unwrap();

            assert_eq!(
                engine.revert(EntryId(1), false).unwrap(),
                RevertOutcome::ChangedSince {
                    current: Some(Value::Dword(7))
                }
            );
        }

        #[test]
        fn takes_a_value_that_is_back_already_as_undone() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.registry.write(KEY, "Shown", &dword(0)).unwrap();

            assert_eq!(
                engine.revert(EntryId(1), false).unwrap(),
                RevertOutcome::Reverted
            );
        }

        #[test]
        fn force_undoes_a_value_changed_since() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.registry.write(KEY, "Shown", &dword(7)).unwrap();
            engine.revert(EntryId(1), true).unwrap();

            assert_eq!(engine.registry().get(KEY, "Shown"), Some(&dword(0)));
        }

        #[test]
        fn does_nothing_twice() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert_eq!(
                engine.revert(EntryId(1), false).unwrap(),
                RevertOutcome::NotApplied
            );
        }

        #[test]
        fn a_session_comes_back_whole_newest_first() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            // The same value twice in one session: newest first restores the
            // oldest "before".
            let (session, _) = engine
                .apply("t", &[set("Shown", 1), set("Shown", 2)])
                .unwrap();
            engine.revert_session(session, false).unwrap();

            assert_eq!(engine.registry().get(KEY, "Shown"), Some(&dword(0)));
        }

        #[test]
        fn everything_comes_back() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("a", &[set("Shown", 1)]).unwrap();
            engine.apply("b", &[set("New", 1)]).unwrap();
            engine.revert_all(false).unwrap();

            assert_eq!(
                (
                    engine.registry().get(KEY, "Shown"),
                    engine.registry().get(KEY, "New")
                ),
                (Some(&dword(0)), None)
            );
        }
    }

    mod drift {
        use super::*;

        #[test]
        fn finds_a_value_windows_put_back() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("t", &[set("Shown", 1)]).unwrap();
            engine.registry.write(KEY, "Shown", &dword(0)).unwrap();

            assert_eq!(
                engine.drift().unwrap(),
                vec![Drift {
                    entry: EntryId(1),
                    target: target("Shown"),
                    expected: Some(Value::Dword(1)),
                    current: Some(Value::Dword(0)),
                }]
            );
        }

        #[test]
        fn counts_only_the_newest_change_to_a_value() {
            let setup = Setup::new();
            let mut engine = setup.engine(existing());
            engine.apply("a", &[set("Shown", 1)]).unwrap();
            engine.apply("b", &[set("Shown", 2)]).unwrap();

            assert_eq!(engine.drift().unwrap(), vec![]);
        }
    }

    mod parameters {
        use platform::parameters::Parameter;

        use super::*;
        use crate::parameters::Parameters;

        const MENU: Target = Target::Parameter {
            name: Parameter::MenuAnimation,
        };

        fn turn(on: u32) -> Change {
            Change {
                target: MENU,
                value: Some(Value::Dword(on)),
            }
        }

        #[test]
        fn turns_a_parameter_off_and_back_on() {
            let setup = Setup::new();
            let mut engine = setup.engine(FakeRegistry::default());
            engine.apply("t", &[turn(0)]).unwrap();
            let off = Parameters::get(engine.registry(), Parameter::MenuAnimation).unwrap();
            engine.revert(EntryId(1), false).unwrap();

            assert_eq!(
                (
                    off,
                    Parameters::get(engine.registry(), Parameter::MenuAnimation).unwrap()
                ),
                (false, true)
            );
        }

        #[test]
        fn records_the_parameter_as_it_was() {
            let setup = Setup::new();
            let mut engine = setup.engine(FakeRegistry::default());
            engine.apply("t", &[turn(0)]).unwrap();

            assert_eq!(engine.entries()[0].before.value, Some(Value::Dword(1)));
        }

        #[test]
        fn finds_a_parameter_turned_back_on() {
            let setup = Setup::new();
            let mut engine = setup.engine(FakeRegistry::default());
            engine.apply("t", &[turn(0)]).unwrap();
            engine.registry.set(Parameter::MenuAnimation, true).unwrap();

            assert_eq!(engine.drift().unwrap().len(), 1);
        }

        #[test]
        fn refuses_a_value_other_than_on_or_off() {
            let setup = Setup::new();
            let mut engine = setup.engine(FakeRegistry::default());
            let error = engine.apply("t", &[turn(2)]).unwrap_err();

            assert!(matches!(*error.cause, EngineError::NotAFlag { .. }));
        }
    }

    mod settle {
        use super::*;

        /// Writes an intent to the journal the way a crash would leave it.
        fn crash_after_intent(setup: &Setup, before: Option<Value>) {
            let (mut journal, _) = Journal::open(&setup.path).unwrap();
            journal
                .append(&Event::Intent {
                    entry: EntryId(1),
                    session: SessionId(1),
                    at: 0,
                    tweak: "t".into(),
                    target: target("Shown"),
                    before: Before {
                        value: before,
                        created_key: None,
                    },
                    after: Some(Value::Dword(1)),
                })
                .unwrap();
        }

        #[test]
        fn confirms_a_change_that_was_made() {
            let setup = Setup::new();
            crash_after_intent(&setup, Some(Value::Dword(0)));
            let engine = setup.engine(FakeRegistry::default().with(KEY, "Shown", dword(1)));

            assert_eq!(engine.entries()[0].state, State::Applied);
        }

        #[test]
        fn fails_a_change_that_was_not_made() {
            let setup = Setup::new();
            crash_after_intent(&setup, Some(Value::Dword(0)));
            let engine = setup.engine(existing());

            assert!(matches!(engine.entries()[0].state, State::Failed(_)));
        }
    }
}
