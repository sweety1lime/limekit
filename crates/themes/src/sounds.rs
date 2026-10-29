//! Sound sets: the sound each event plays.

use platform::sounds::SoundEvent;
use tweaks::{Change, Target, Value};

/// The key that holds every program's events, below HKEY_CURRENT_USER.
pub const SOUNDS_KEY: &str = r"AppEvents\Schemes\Apps";

/// The id of the set that silences the events.
pub const NO_SOUNDS: &str = "no-sounds";

/// Events named like this are not played on their own. They are the library of
/// alarm and ringtone melodies that Alarms & Clock and call apps let people
/// pick from, so silencing one would silence the alarm that picks it.
const MELODY_LIBRARY: &str = "Notification.Looping.";

/// A sound for each of some events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSet {
    pub id: String,
    pub events: Vec<SoundChoice>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundChoice {
    pub app: String,
    pub event: String,
    /// What the Sound dialog calls the event, for the confirmation.
    pub label: String,
    /// The `.wav` file, or empty for silence.
    pub sound: String,
}

/// The set that silences every event that has a sound now, the melody library
/// apart. Events that are silent already, or have no value at all, are left
/// out: there is nothing to change and nothing to undo.
pub fn silence(events: &[SoundEvent]) -> SoundSet {
    SoundSet {
        id: NO_SOUNDS.to_owned(),
        events: events
            .iter()
            .filter(|event| {
                event
                    .sound
                    .as_deref()
                    .is_some_and(|sound| !sound.is_empty())
            })
            .filter(|event| !event.event.starts_with(MELODY_LIBRARY))
            .map(|event| SoundChoice {
                app: event.app.clone(),
                event: event.event.clone(),
                label: event.label.clone(),
                sound: String::new(),
            })
            .collect(),
    }
}

/// The registry changes that give the current user the sounds of `set`, under
/// `key`: `SOUNDS_KEY`, or a private key in tests. A sound is the default value
/// of the event's `.Current` key.
pub fn sound_changes(set: &SoundSet, key: &str) -> Vec<Change> {
    set.events
        .iter()
        .map(|choice| Change {
            target: Target::Registry {
                path: format!(r"{key}\{}\{}\.Current", choice.app, choice.event),
                name: String::new(),
            },
            value: Some(Value::Sz(choice.sound.clone())),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(app: &str, name: &str, sound: Option<&str>) -> SoundEvent {
        SoundEvent {
            app: app.into(),
            event: name.into(),
            label: format!("label of {name}"),
            sound: sound.map(str::to_owned),
        }
    }

    #[test]
    fn silence_covers_the_events_that_make_a_sound() {
        let events = [
            event(
                ".Default",
                "SystemAsterisk",
                Some(r"C:\Windows\media\a.wav"),
            ),
            event("Explorer", "FaxSent", Some(r"%SystemRoot%\media\tada.wav")),
            event(
                ".Default",
                "WindowsLogon",
                Some(r"C:\Windows\media\logon.wav"),
            ),
        ];

        let set = silence(&events);

        assert_eq!(set.id, NO_SOUNDS);
        let named: Vec<(&str, &str)> = set
            .events
            .iter()
            .map(|choice| (choice.app.as_str(), choice.event.as_str()))
            .collect();
        assert_eq!(
            named,
            [
                (".Default", "SystemAsterisk"),
                ("Explorer", "FaxSent"),
                (".Default", "WindowsLogon")
            ]
        );
        assert!(set.events.iter().all(|choice| choice.sound.is_empty()));
        assert_eq!(set.events[0].label, "label of SystemAsterisk");
    }

    #[test]
    fn silence_leaves_out_what_is_silent_and_what_has_no_value() {
        let events = [
            event(".Default", "Close", Some("")),
            event(".Default", "SystemExit", None),
        ];

        assert!(silence(&events).events.is_empty());
    }

    #[test]
    fn silence_leaves_the_melody_library_alone() {
        let events = [
            event(
                ".Default",
                "Notification.Looping.Alarm2",
                Some(r"C:\Windows\media\Alarm02.wav"),
            ),
            event(
                ".Default",
                "Notification.Looping.Call",
                Some(r"C:\Windows\media\Ring01.wav"),
            ),
            event(
                ".Default",
                "Notification.Default",
                Some(r"C:\Windows\media\n.wav"),
            ),
        ];

        let set = silence(&events);

        assert_eq!(set.events.len(), 1);
        assert_eq!(set.events[0].event, "Notification.Default");
    }

    #[test]
    fn a_sound_is_the_default_value_of_the_event_current_key() {
        let set = silence(&[event(".Default", "Close", Some(r"C:\a.wav"))]);

        let changes = sound_changes(&set, r"Test\Apps");

        assert_eq!(
            changes,
            [Change {
                target: Target::Registry {
                    path: r"Test\Apps\.Default\Close\.Current".into(),
                    name: String::new()
                },
                value: Some(Value::Sz(String::new())),
            }]
        );
    }
}
