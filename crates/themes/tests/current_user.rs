//! Themes against the real HKEY_CURRENT_USER, inside a private key that is
//! deleted afterwards, so the tests never touch the real cursors.
#![cfg(windows)]

use platform::registry::{self, RawValue};
use themes::{CursorSet, Part, Role, active, cursor_changes, put_on, take_off};
use tweaks::{Change, CurrentUser, Engine, Outcome, State, Target, Value};

struct Sandbox {
    root: String,
    _dir: tempfile::TempDir,
    journal: std::path::PathBuf,
}

impl Sandbox {
    /// A private stand-in for `Control Panel\Cursors` with the values a real
    /// one has: a path, an empty string meaning the built-in cursor, and roles
    /// that are missing altogether.
    fn new(name: &str) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let sandbox = Sandbox {
            root: format!(r"Software\LimeKitTest-{}-themes-{name}", std::process::id()),
            journal: dir.path().join("journal.jsonl"),
            _dir: dir,
        };
        sandbox.write("Arrow", r"C:\original\arrow.cur");
        sandbox.write("Crosshair", "");
        sandbox
    }

    fn key(&self) -> String {
        format!(r"{}\Cursors", self.root)
    }

    fn engine(&self) -> Engine<CurrentUser> {
        Engine::open(CurrentUser, &self.journal).unwrap()
    }

    fn write(&self, name: &str, text: &str) {
        registry::write_value(&self.key(), name, &Value::Sz(text.into()).to_raw()).unwrap();
    }

    fn read(&self, name: &str) -> Option<RawValue> {
        registry::read_value(&self.key(), name).unwrap()
    }

    fn is(&self, name: &str, text: &str) -> bool {
        self.read(name)
            .is_some_and(|value| value.same_as(&Value::Sz(text.into()).to_raw()))
    }

    fn changes(&self, id: &str, roles: &[(Role, &str)]) -> Vec<Change> {
        let set = CursorSet {
            id: id.into(),
            name: id.into(),
            roles: roles
                .iter()
                .map(|(role, path)| (*role, (*path).to_owned()))
                .collect(),
        };
        cursor_changes(&set, &self.key())
    }

    fn target(&self, name: &str) -> Target {
        Target::Registry {
            path: self.key(),
            name: name.into(),
        }
    }

    /// Every value a test looks at, as stored.
    fn snapshot(&self) -> Vec<Option<RawValue>> {
        ["Arrow", "Crosshair", "Hand", "Pin"]
            .iter()
            .map(|name| self.read(name))
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = windows_registry::CURRENT_USER.remove_tree(&self.root);
    }
}

fn same(left: &[Option<RawValue>], right: &[Option<RawValue>]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|pair| match pair {
            (Some(left), Some(right)) => left.same_as(right),
            (None, None) => true,
            _ => false,
        })
}

#[test]
fn a_theme_writes_its_roles_and_taking_it_off_gives_every_value_back() {
    let sandbox = Sandbox::new("round-trip");
    let before = sandbox.snapshot();
    let mut engine = sandbox.engine();
    let changes = sandbox.changes(
        "a",
        &[
            (Role::Arrow, r"C:\a\arrow.cur"),
            (Role::Pin, r"C:\a\pin.cur"),
        ],
    );

    put_on(&mut engine, Part::Cursors, "a", &changes).unwrap();

    assert!(sandbox.is("Arrow", r"C:\a\arrow.cur"));
    assert!(sandbox.is("Pin", r"C:\a\pin.cur"));
    assert!(
        sandbox.is("Crosshair", ""),
        "a role the theme does not name is left alone"
    );
    assert_eq!(active(&engine, Part::Cursors).as_deref(), Some("a"));

    let left_alone = take_off(&mut engine, Part::Cursors, "a").unwrap();

    assert!(left_alone.is_empty());
    assert!(
        same(&before, &sandbox.snapshot()),
        "the empty string and the missing value both came back"
    );
    assert_eq!(active(&engine, Part::Cursors), None);
}

#[test]
fn a_second_theme_takes_the_first_off_before_it_goes_on() {
    let sandbox = Sandbox::new("replace");
    let before = sandbox.snapshot();
    let mut engine = sandbox.engine();
    let a = sandbox.changes(
        "a",
        &[
            (Role::Arrow, r"C:\a\arrow.cur"),
            (Role::Pin, r"C:\a\pin.cur"),
        ],
    );
    let b = sandbox.changes(
        "b",
        &[
            (Role::Arrow, r"C:\b\arrow.cur"),
            (Role::Hand, r"C:\b\hand.cur"),
        ],
    );
    put_on(&mut engine, Part::Cursors, "a", &a).unwrap();

    let applied = put_on(&mut engine, Part::Cursors, "b", &b).unwrap();

    assert!(applied.left_alone.is_empty());
    assert!(sandbox.is("Arrow", r"C:\b\arrow.cur"));
    assert!(sandbox.is("Hand", r"C:\b\hand.cur"));
    assert!(
        sandbox.read("Pin").is_none(),
        "the first theme's extra role is gone"
    );
    assert_eq!(active(&engine, Part::Cursors).as_deref(), Some("b"));
    assert!(
        engine
            .entries()
            .iter()
            .filter(|entry| entry.tweak == "theme.cursors.a")
            .all(|entry| entry.state == State::Reverted)
    );

    take_off(&mut engine, Part::Cursors, "b").unwrap();

    assert!(
        same(&before, &sandbox.snapshot()),
        "no pile of themes: straight back to before LimeKit"
    );
}

#[test]
fn a_value_changed_after_the_theme_is_left_alone() {
    let sandbox = Sandbox::new("changed-since");
    let mut engine = sandbox.engine();
    let changes = sandbox.changes(
        "a",
        &[
            (Role::Arrow, r"C:\a\arrow.cur"),
            (Role::Pin, r"C:\a\pin.cur"),
        ],
    );
    put_on(&mut engine, Part::Cursors, "a", &changes).unwrap();
    sandbox.write("Arrow", r"C:\mine.cur");

    let left_alone = take_off(&mut engine, Part::Cursors, "a").unwrap();

    assert_eq!(left_alone, [sandbox.target("Arrow")]);
    assert!(
        sandbox.is("Arrow", r"C:\mine.cur"),
        "the user's own choice stays"
    );
    assert!(
        sandbox.read("Pin").is_none(),
        "the untouched role went back"
    );
}

#[test]
fn a_switch_names_what_the_old_theme_left() {
    let sandbox = Sandbox::new("switch-left");
    let mut engine = sandbox.engine();
    let a = sandbox.changes("a", &[(Role::Arrow, r"C:\a\arrow.cur")]);
    let b = sandbox.changes("b", &[(Role::Arrow, r"C:\b\arrow.cur")]);
    put_on(&mut engine, Part::Cursors, "a", &a).unwrap();
    sandbox.write("Arrow", r"C:\mine.cur");

    let applied = put_on(&mut engine, Part::Cursors, "b", &b).unwrap();

    assert_eq!(applied.left_alone, [sandbox.target("Arrow")]);
    assert!(sandbox.is("Arrow", r"C:\b\arrow.cur"));
}

#[test]
fn putting_the_same_theme_on_twice_changes_nothing_the_second_time() {
    let sandbox = Sandbox::new("twice");
    let mut engine = sandbox.engine();
    let changes = sandbox.changes("a", &[(Role::Arrow, r"C:\a\arrow.cur")]);
    put_on(&mut engine, Part::Cursors, "a", &changes).unwrap();

    let again = put_on(&mut engine, Part::Cursors, "a", &changes).unwrap();

    assert_eq!(again.outcomes, [Outcome::AlreadySet]);
}

#[test]
fn themes_and_tweaks_share_the_journal_without_meeting() {
    let sandbox = Sandbox::new("shared");
    let mut engine = sandbox.engine();
    let theme = sandbox.changes("a", &[(Role::Arrow, r"C:\a\arrow.cur")]);
    let tweak = Change {
        target: Target::Registry {
            path: format!(r"{}\Tweak", sandbox.root),
            name: "Flag".into(),
        },
        value: Some(Value::Dword(1)),
    };
    engine.apply("explorer.file_extensions", &[tweak]).unwrap();
    put_on(&mut engine, Part::Cursors, "a", &theme).unwrap();

    take_off(&mut engine, Part::Cursors, "a").unwrap();

    assert!(
        registry::read_value(&format!(r"{}\Tweak", sandbox.root), "Flag")
            .unwrap()
            .is_some(),
        "taking a theme off leaves the tweak on"
    );
    assert_eq!(active(&engine, Part::Cursors), None);
}
