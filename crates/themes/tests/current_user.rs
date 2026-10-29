//! Themes against the real HKEY_CURRENT_USER, inside a private key that is
//! deleted afterwards, so the tests never touch the real cursors.
#![cfg(windows)]

use platform::registry::{self, RawValue};
use platform::sounds::SoundEvent;
use themes::{
    CursorSet, Part, Role, active, cursor_changes, put_on, silence, sound_changes, take_off,
};
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

impl Sandbox {
    fn apps_key(&self) -> String {
        format!(r"{}\Apps", self.root)
    }

    fn event_path(&self, app: &str, event: &str) -> String {
        format!(r"{}\{app}\{event}\.Current", self.apps_key())
    }

    /// An event as Windows keeps it: a `.Current` key that holds a sound, or
    /// holds no value at all.
    fn seed_event(&self, app: &str, event: &str, sound: Option<Value>) -> SoundEvent {
        let path = self.event_path(app, event);
        let text = match &sound {
            Some(Value::Sz(text) | Value::ExpandSz(text)) => Some(text.clone()),
            _ => None,
        };
        match &sound {
            Some(value) => registry::write_value(&path, "", &value.to_raw()).unwrap(),
            None => {
                windows_registry::CURRENT_USER.create(&path).unwrap();
            }
        }
        SoundEvent {
            app: app.into(),
            event: event.into(),
            label: event.into(),
            sound: text,
        }
    }

    fn sound(&self, app: &str, event: &str) -> Option<RawValue> {
        registry::read_value(&self.event_path(app, event), "").unwrap()
    }
}

#[test]
fn silencing_the_sounds_and_taking_it_off_gives_each_back_with_its_type() {
    let sandbox = Sandbox::new("sounds");
    let events = [
        sandbox.seed_event(
            ".Default",
            "SystemAsterisk",
            Some(Value::Sz(r"C:\Windows\media\a.wav".into())),
        ),
        sandbox.seed_event(
            ".Default",
            "Close",
            Some(Value::ExpandSz(r"%SystemRoot%\media\b.wav".into())),
        ),
        sandbox.seed_event(".Default", "Open", Some(Value::Sz(String::new()))),
        sandbox.seed_event(".Default", "SystemExit", None),
        sandbox.seed_event(
            ".Default",
            "Notification.Looping.Alarm",
            Some(Value::Sz(r"C:\Windows\media\Alarm01.wav".into())),
        ),
    ];
    let names = [
        "SystemAsterisk",
        "Close",
        "Open",
        "SystemExit",
        "Notification.Looping.Alarm",
    ];
    let read_all = || -> Vec<Option<RawValue>> {
        names
            .iter()
            .map(|name| sandbox.sound(".Default", name))
            .collect()
    };
    let before = read_all();
    let mut engine = sandbox.engine();
    let changes = sound_changes(&silence(&events), &sandbox.apps_key());

    let applied = put_on(&mut engine, Part::Sounds, "no-sounds", &changes).unwrap();

    assert_eq!(
        applied.outcomes.len(),
        2,
        "only the two events with a sound are changed"
    );
    let now = read_all();
    let silent = Value::Sz(String::new()).to_raw();
    assert!(now[0].as_ref().is_some_and(|value| value.same_as(&silent)));
    assert!(
        now[1].as_ref().is_some_and(|value| value.same_as(&silent)),
        "the expanding string is plain silence now"
    );
    assert!(
        same(&before[2..], &now[2..]),
        "silent, valueless and library events are untouched"
    );
    assert_eq!(active(&engine, Part::Sounds).as_deref(), Some("no-sounds"));

    let left_alone = take_off(&mut engine, Part::Sounds, "no-sounds").unwrap();

    assert!(left_alone.is_empty());
    assert!(
        same(&before, &read_all()),
        "the expanding string is expanding again"
    );
    assert_eq!(active(&engine, Part::Sounds), None);
}

#[test]
fn cursors_and_sounds_come_off_independently() {
    let sandbox = Sandbox::new("parts");
    let events = [sandbox.seed_event(".Default", "Close", Some(Value::Sz(r"C:\a.wav".into())))];
    let mut engine = sandbox.engine();
    let cursors = sandbox.changes("a", &[(Role::Arrow, r"C:\a\arrow.cur")]);
    put_on(&mut engine, Part::Cursors, "a", &cursors).unwrap();
    put_on(
        &mut engine,
        Part::Sounds,
        "no-sounds",
        &sound_changes(&silence(&events), &sandbox.apps_key()),
    )
    .unwrap();

    take_off(&mut engine, Part::Sounds, "no-sounds").unwrap();

    assert_eq!(active(&engine, Part::Sounds), None);
    assert_eq!(active(&engine, Part::Cursors).as_deref(), Some("a"));
    assert!(
        sandbox.is("Arrow", r"C:\a\arrow.cur"),
        "the cursors stayed on"
    );
    assert!(
        sandbox
            .sound(".Default", "Close")
            .is_some_and(|value| value.same_as(&Value::Sz(r"C:\a.wav".into()).to_raw()))
    );
}
