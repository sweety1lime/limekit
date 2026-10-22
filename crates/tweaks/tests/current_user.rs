//! The engine against the real HKEY_CURRENT_USER, inside a private key that is
//! deleted afterwards, so the tests never touch real settings.
#![cfg(windows)]

use platform::registry::{self, RawValue};
use tweaks::{Change, CurrentUser, Engine, EntryId, RevertOutcome, Target, Value};

struct Sandbox {
    root: String,
    _dir: tempfile::TempDir,
    journal: std::path::PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.jsonl");
        Sandbox {
            root: format!(r"Software\LimeKitTest-{}-tweaks-{name}", std::process::id()),
            _dir: dir,
            journal,
        }
    }

    fn engine(&self) -> Engine<CurrentUser> {
        Engine::open(CurrentUser, &self.journal).unwrap()
    }

    fn change(&self, sub: &str, name: &str, value: Value) -> Change {
        Change {
            target: Target::Registry {
                path: format!(r"{}\{sub}", self.root),
                name: name.into(),
            },
            value: Some(value),
        }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = windows_registry::CURRENT_USER.remove_tree(&self.root);
    }
}

#[test]
fn undoing_a_change_to_a_new_key_leaves_no_trace() {
    let sandbox = Sandbox::new("new-key");
    let mut engine = sandbox.engine();
    engine
        .apply("t", &[sandbox.change(r"Deep\Leaf", "V", Value::Dword(1))])
        .unwrap();
    assert!(registry::key_exists(&format!(r"{}\Deep\Leaf", sandbox.root)).unwrap());

    engine.revert(EntryId(1), false).unwrap();

    assert!(!registry::key_exists(&sandbox.root).unwrap());
}

#[test]
fn undoing_a_change_to_an_existing_value_restores_its_bytes() {
    let sandbox = Sandbox::new("existing");
    let path = format!(r"{}\Key", sandbox.root);
    // A string without its NUL, as some programs write them.
    let original = RawValue {
        kind: 1,
        bytes: vec![b'o', 0, b'k', 0],
    };
    registry::write_value(&path, "S", &original).unwrap();
    let mut engine = sandbox.engine();
    engine
        .apply(
            "t",
            &[sandbox.change("Key", "S", Value::Sz("changed".into()))],
        )
        .unwrap();

    assert_eq!(
        engine.revert(EntryId(1), false).unwrap(),
        RevertOutcome::Reverted
    );
    assert!(
        registry::read_value(&path, "S")
            .unwrap()
            .is_some_and(|value| value.same_as(&original))
    );
}

#[test]
fn the_journal_carries_changes_across_restarts() {
    let sandbox = Sandbox::new("restart");
    sandbox
        .engine()
        .apply("t", &[sandbox.change("Key", "V", Value::Dword(1))])
        .unwrap();

    let mut reopened = sandbox.engine();
    reopened.revert_all(false).unwrap();

    assert!(!registry::key_exists(&sandbox.root).unwrap());
}
