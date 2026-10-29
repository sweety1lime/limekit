//! The themes screen's backend: the cursor sets Windows ships, each with its
//! standing on this Windows, and putting them on and taking them off through
//! the tweak journal.
//!
//! Themes go through the same engine as tweaks (`tweak_panel::TweakEngine`),
//! so there is one journal and one writer: this window process.

use base64::Engine as _;
use serde::Serialize;
use themes::{CursorSet, Part, Role, SwitchError};
use tweaks::{Availability, CurrentUser, Engine, EngineError, Outcome, State, Target, Value};

use crate::tweak_panel::{
    AvailabilityView, SwitchOutcome, TweakEngine, describe_windows, with_engine,
};

/// The cursors drawn on a theme's card, when the set has a still picture of
/// them.
const CARD_ROLES: [Role; 4] = [Role::Arrow, Role::Hand, Role::Help, Role::SizeAll];

/// A cursor file bigger than this is not drawn on a card.
const MAX_CARD_BYTES: u64 = 512 * 1024;

/// The cursor sets Windows ships, named in the current language.
fn windows_sets() -> Vec<CursorSet> {
    themes::windows_sets(
        &platform::cursors::system_schemes(),
        |path| themes::expand_env(path, |name| std::env::var(name).ok()),
        platform::shell::indirect_string,
    )
}

fn find(id: &str) -> Result<CursorSet, String> {
    windows_sets()
        .into_iter()
        .find(|set| set.id == id)
        .ok_or_else(|| format!("no cursor set {id:?}"))
}

/// A `.cur` file is a still picture; an animated `.ani` is not drawn.
fn is_picture(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".cur")
}

#[derive(Serialize)]
pub struct ThemesView {
    /// "Windows 11 24H2, 26100.4061, Pro"; empty when unknown.
    windows: String,
    themes: Vec<ThemeView>,
    /// Themes whose values something changed after LimeKit set them.
    drift: Vec<String>,
}

#[derive(Serialize)]
pub struct ThemeView {
    id: String,
    title: String,
    availability: AvailabilityView,
    /// LimeKit has this theme's cursors in effect, so taking it off gives back
    /// what was there before LimeKit.
    applied: bool,
    /// Roles the card can show a picture of, by registry name.
    pictures: Vec<&'static str>,
}

#[tauri::command]
pub async fn themes_view(state: tauri::State<'_, TweakEngine>) -> Result<ThemesView, String> {
    let version = platform::system::windows_version();
    let requirements = themes::requirements();
    let sets = windows_sets();
    with_engine(&state, |engine| {
        let active = themes::active(engine, Part::Cursors);
        let views = sets
            .iter()
            .map(|set| ThemeView {
                id: set.id.clone(),
                title: set.name.clone(),
                availability: version
                    .as_ref()
                    .map_or(Availability::Unverified, |version| {
                        requirements.availability(version)
                    })
                    .into(),
                applied: active.as_deref() == Some(set.id.as_str()),
                pictures: CARD_ROLES
                    .iter()
                    .filter(|role| set.path(**role).is_some_and(is_picture))
                    .map(|role| role.name())
                    .collect(),
            })
            .collect();
        let mut drift: Vec<String> = Vec::new();
        for item in engine.drift().map_err(|error| error.to_string())? {
            let theme = engine
                .entries()
                .iter()
                .find(|entry| entry.id == item.entry)
                .and_then(|entry| themes::parse_journal_id(&entry.tweak))
                .map(|(_, theme)| theme.to_owned());
            if let Some(theme) = theme
                && !drift.contains(&theme)
            {
                drift.push(theme);
            }
        }
        Ok(ThemesView {
            windows: version.as_ref().map(describe_windows).unwrap_or_default(),
            themes: views,
            drift,
        })
    })
}

/// One cursor of a set as a data URL the card can draw. Takes the set and the
/// role, not a path, so only files a set names can be read. A card works
/// without its pictures, so a file that is missing or too big gives none.
#[tauri::command]
pub async fn theme_cursor_image(id: String, role: String) -> Result<Option<String>, String> {
    let set = find(&id)?;
    let Some(path) = Role::from_name(&role)
        .and_then(|role| set.path(role))
        .filter(|path| is_picture(path))
    else {
        return Ok(None);
    };
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(None);
    };
    if metadata.len() > MAX_CARD_BYTES {
        return Ok(None);
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(None);
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(Some(format!("data:image/x-icon;base64,{encoded}")))
}

/// One line of "what will change".
#[derive(Serialize)]
pub struct ThemeLine {
    /// The role by its registry name; the screen has the words for it.
    role: String,
    current: Option<Value>,
    next: Option<Value>,
}

#[derive(Serialize)]
pub struct ThemePreview {
    /// What would change; what already matches is left out.
    lines: Vec<ThemeLine>,
    /// The theme in effect that this one takes off first.
    replaces: Option<String>,
}

#[tauri::command]
pub async fn theme_preview(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
) -> Result<ThemePreview, String> {
    let set = find(&id)?;
    let titles = windows_sets();
    with_engine(&state, |engine| {
        let active = themes::active(engine, Part::Cursors);
        let in_effect = |theme: &str| {
            let name = themes::journal_id(Part::Cursors, theme);
            engine
                .entries()
                .iter()
                .filter(move |entry| entry.tweak == name && entry.state == State::Applied)
        };
        let mut lines: Vec<(Target, Option<Value>)> = if on {
            themes::cursor_changes(&set, themes::CURSORS_KEY)
                .into_iter()
                .map(|change| (change.target, change.value))
                .collect()
        } else {
            // Off gives back what was there before LimeKit.
            in_effect(&id)
                .map(|entry| (entry.target.clone(), entry.before.value.clone()))
                .collect()
        };
        let replaced = active.filter(|other| on && *other != id);
        if let Some(other) = &replaced {
            // Roles the old theme set and this one does not go back to what
            // they were before it.
            for entry in in_effect(other) {
                if !lines.iter().any(|(target, _)| *target == entry.target) {
                    lines.push((entry.target.clone(), entry.before.value.clone()));
                }
            }
        }
        let lines = lines
            .into_iter()
            .map(|(target, next)| {
                Ok(ThemeLine {
                    role: match &target {
                        Target::Registry { name, .. } => name.clone(),
                        other => other.to_string(),
                    },
                    current: engine.current(&target).map_err(|error| error.to_string())?,
                    next,
                })
            })
            .collect::<Result<Vec<_>, String>>()?
            .into_iter()
            .filter(|line| line.current != line.next)
            .collect();
        Ok(ThemePreview {
            lines,
            replaces: replaced.map(|other| {
                titles
                    .iter()
                    .find(|set| set.id == other)
                    .map_or(other, |set| set.name.clone())
            }),
        })
    })
}

#[tauri::command]
pub async fn theme_switch(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
) -> Result<SwitchOutcome, String> {
    let set = find(&id)?;
    if let Some(version) = platform::system::windows_version()
        && !themes::requirements().availability(&version).applies()
    {
        return Ok(SwitchOutcome::NotAvailable);
    }
    let outcome = with_engine(&state, |engine| {
        switch(engine, &set, on, themes::CURSORS_KEY)
    })?;
    if matches!(
        outcome,
        SwitchOutcome::Done | SwitchOutcome::ChangedSince { .. }
    ) {
        platform::shell::reload_cursors()?;
    }
    Ok(outcome)
}

/// On puts the set's cursors on, taking off the theme that has them now. Off
/// undoes through the journal whatever LimeKit changed for the set.
fn switch(
    engine: &mut Engine<CurrentUser>,
    set: &CursorSet,
    on: bool,
    key: &str,
) -> Result<SwitchOutcome, String> {
    if !on {
        let left_alone =
            themes::take_off(engine, Part::Cursors, &set.id).map_err(|error| error.to_string())?;
        return Ok(if left_alone.is_empty() {
            SwitchOutcome::Done
        } else {
            SwitchOutcome::ChangedSince {
                targets: left_alone.iter().map(ToString::to_string).collect(),
            }
        });
    }
    let changes = themes::cursor_changes(set, key);
    match themes::put_on(engine, Part::Cursors, &set.id, &changes) {
        Ok(applied) if !applied.left_alone.is_empty() => Ok(SwitchOutcome::ChangedSince {
            targets: applied.left_alone.iter().map(ToString::to_string).collect(),
        }),
        Ok(applied)
            if applied
                .outcomes
                .iter()
                .all(|outcome| *outcome == Outcome::AlreadySet) =>
        {
            Ok(SwitchOutcome::AlreadySet)
        }
        Ok(_) => Ok(SwitchOutcome::Done),
        Err(SwitchError::Apply(error)) if error.access_denied() => {
            Ok(SwitchOutcome::AccessDenied {
                target: error.target.to_string(),
            })
        }
        Err(SwitchError::Apply(error)) => match *error.cause {
            EngineError::NotTaken { target, .. } => Ok(SwitchOutcome::NotTaken {
                target: target.to_string(),
            }),
            cause => Err(cause.to_string()),
        },
        Err(SwitchError::Engine(error)) => Err(error.to_string()),
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use platform::registry::{self, RawValue};
    use themes::Role;

    /// A set over a private key under HKCU and a journal in a temp folder,
    /// both gone after the test.
    struct Sandbox {
        root: String,
        set: CursorSet,
        _dir: tempfile::TempDir,
        engine: Engine<CurrentUser>,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root = format!(r"Software\LimeKitTest-{}-themes-{name}", std::process::id());
            let dir = tempfile::tempdir().unwrap();
            let engine = Engine::open(CurrentUser, &dir.path().join("journal.jsonl")).unwrap();
            let sandbox = Sandbox {
                root,
                set: CursorSet {
                    id: "windows-test".into(),
                    name: "Test".into(),
                    roles: vec![(Role::Arrow, r"C:\test\arrow.cur".into())],
                },
                _dir: dir,
                engine,
            };
            sandbox.write(r"C:\original\arrow.cur");
            sandbox
        }

        fn key(&self) -> String {
            format!(r"{}\Cursors", self.root)
        }

        fn write(&self, path: &str) {
            registry::write_value(&self.key(), "Arrow", &Value::Sz(path.into()).to_raw()).unwrap();
        }

        fn switch(&mut self, on: bool) -> SwitchOutcome {
            let key = self.key();
            switch(&mut self.engine, &self.set, on, &key).unwrap()
        }

        fn arrow(&self) -> Option<RawValue> {
            registry::read_value(&self.key(), "Arrow").unwrap()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.root);
        }
    }

    fn sz(path: &str) -> Option<RawValue> {
        Some(Value::Sz(path.into()).to_raw())
    }

    #[test]
    fn off_after_on_gives_back_the_cursor_that_was_there() {
        let mut sandbox = Sandbox::new("round-trip");

        assert!(matches!(sandbox.switch(true), SwitchOutcome::Done));
        assert_eq!(sandbox.arrow(), sz(r"C:\test\arrow.cur"));
        assert!(matches!(sandbox.switch(false), SwitchOutcome::Done));

        assert_eq!(sandbox.arrow(), sz(r"C:\original\arrow.cur"));
    }

    #[test]
    fn on_twice_changes_nothing_the_second_time() {
        let mut sandbox = Sandbox::new("twice");
        sandbox.switch(true);

        assert!(matches!(sandbox.switch(true), SwitchOutcome::AlreadySet));
    }

    #[test]
    fn off_leaves_alone_a_cursor_changed_after_the_theme() {
        let mut sandbox = Sandbox::new("changed");
        sandbox.switch(true);
        sandbox.write(r"C:\mine\arrow.cur");

        assert!(matches!(
            sandbox.switch(false),
            SwitchOutcome::ChangedSince { .. }
        ));
        assert_eq!(sandbox.arrow(), sz(r"C:\mine\arrow.cur"));
    }

    #[test]
    fn only_a_still_picture_is_drawn_on_a_card() {
        assert!(is_picture(r"C:\Windows\Cursors\aero_arrow.cur"));
        assert!(is_picture(r"C:\X\ARROW.CUR"));
        assert!(!is_picture(r"C:\Windows\Cursors\aero_busy.ani"));
    }
}
