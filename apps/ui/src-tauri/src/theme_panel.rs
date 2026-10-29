//! The themes screen's backend: the cursor sets Windows ships and a set that
//! silences the system sounds, each with its standing on this Windows, and
//! putting them on and taking them off through the tweak journal.
//!
//! Themes go through the same engine as tweaks (`tweak_panel::TweakEngine`),
//! so there is one journal and one writer: this window process.

use base64::Engine as _;
use platform::sounds::SoundEvent;
use serde::Serialize;
use themes::{CursorSet, Part, Role, SoundSet, SwitchError};
use tweaks::{
    Availability, Change, CurrentUser, Engine, EngineError, Outcome, State, Target, Value,
};

use crate::tweak_panel::{
    AvailabilityView, SwitchOutcome, TweakEngine, describe_windows, with_engine,
};

/// The cursors drawn on a theme's card, when the set has a still picture of
/// them.
const CARD_ROLES: [Role; 4] = [Role::Arrow, Role::Hand, Role::Help, Role::SizeAll];

/// A cursor file bigger than this is not drawn on a card.
const MAX_CARD_BYTES: u64 = 512 * 1024;

/// What a theme gives the user.
enum Content {
    Cursors(CursorSet),
    Sounds(SoundSet),
}

/// Where a theme comes from, for the chip on its card.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Windows,
    LimeKit,
}

/// A theme as the screen lists it.
struct Theme {
    id: String,
    title: String,
    description: String,
    source: Source,
    content: Content,
}

impl Theme {
    fn part(&self) -> Part {
        match self.content {
            Content::Cursors(_) => Part::Cursors,
            Content::Sounds(_) => Part::Sounds,
        }
    }

    /// The registry changes that put the theme on, under the real keys.
    fn changes(&self) -> Vec<Change> {
        match &self.content {
            Content::Cursors(set) => themes::cursor_changes(set, themes::CURSORS_KEY),
            Content::Sounds(set) => themes::sound_changes(set, themes::SOUNDS_KEY),
        }
    }
}

/// The cursor sets Windows ships, named in the current language.
fn windows_sets() -> Vec<CursorSet> {
    themes::windows_sets(
        &platform::cursors::system_schemes(),
        |path| themes::expand_env(path, |name| std::env::var(name).ok()),
        platform::shell::indirect_string,
    )
}

/// What the set that silences the sounds is called and says, in `lang`.
fn no_sounds_text(lang: &str) -> (&'static str, &'static str) {
    if lang == "en" {
        (
            "No sounds",
            "Silences the sounds of Windows events: errors, notifications, devices, signing in. The alarm and ringtone melodies stay.",
        )
    } else {
        (
            "Без звуков",
            "Гасит звуки событий Windows: ошибки, уведомления, подключение устройств, вход в систему. Мелодии будильников и звонков остаются.",
        )
    }
}

/// Every theme, with the sounds as they are now: the set that silences them
/// names the events that still make one.
fn all(lang: &str, events: &[SoundEvent]) -> Vec<Theme> {
    let mut all: Vec<Theme> = windows_sets()
        .into_iter()
        .map(|set| Theme {
            id: set.id.clone(),
            title: set.name.clone(),
            description: String::new(),
            source: Source::Windows,
            content: Content::Cursors(set),
        })
        .collect();
    let (title, description) = no_sounds_text(lang);
    all.push(Theme {
        id: themes::NO_SOUNDS.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        source: Source::LimeKit,
        content: Content::Sounds(themes::silence(events)),
    });
    all
}

fn part_named(name: &str) -> Result<Part, String> {
    Part::from_name(name).ok_or_else(|| format!("no theme part {name:?}"))
}

fn find(themes: Vec<Theme>, part: Part, id: &str) -> Result<Theme, String> {
    themes
        .into_iter()
        .find(|theme| theme.part() == part && theme.id == id)
        .ok_or_else(|| format!("no {} theme {id:?}", part.name()))
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
    drift: Vec<DriftView>,
}

#[derive(Serialize)]
pub struct ThemeView {
    /// "cursors" or "sounds".
    part: &'static str,
    id: String,
    title: String,
    description: String,
    source: Source,
    availability: AvailabilityView,
    /// LimeKit has this theme in effect, so taking it off gives back what was
    /// there before LimeKit.
    applied: bool,
    /// Roles the card can show a picture of, by registry name.
    pictures: Vec<&'static str>,
}

#[derive(Serialize)]
pub struct DriftView {
    part: &'static str,
    id: String,
}

#[tauri::command]
pub async fn themes_view(
    state: tauri::State<'_, TweakEngine>,
    lang: String,
) -> Result<ThemesView, String> {
    let version = platform::system::windows_version();
    let requirements = themes::requirements();
    let sets = all(&lang, &platform::sounds::events());
    with_engine(&state, |engine| {
        let views = sets
            .iter()
            .map(|theme| {
                let part = theme.part();
                ThemeView {
                    part: part.name(),
                    id: theme.id.clone(),
                    title: theme.title.clone(),
                    description: theme.description.clone(),
                    source: theme.source,
                    availability: version
                        .as_ref()
                        .map_or(Availability::Unverified, |version| {
                            requirements.availability(version)
                        })
                        .into(),
                    applied: themes::active(engine, part).as_deref() == Some(theme.id.as_str()),
                    pictures: match &theme.content {
                        Content::Cursors(set) => CARD_ROLES
                            .iter()
                            .filter(|role| set.path(**role).is_some_and(is_picture))
                            .map(|role| role.name())
                            .collect(),
                        Content::Sounds(_) => Vec::new(),
                    },
                }
            })
            .collect();
        let mut drift: Vec<(Part, String)> = Vec::new();
        for item in engine.drift().map_err(|error| error.to_string())? {
            let found = engine
                .entries()
                .iter()
                .find(|entry| entry.id == item.entry)
                .and_then(|entry| themes::parse_journal_id(&entry.tweak))
                .map(|(part, theme)| (part, theme.to_owned()));
            if let Some(found) = found
                && !drift.contains(&found)
            {
                drift.push(found);
            }
        }
        Ok(ThemesView {
            windows: version.as_ref().map(describe_windows).unwrap_or_default(),
            themes: views,
            drift: drift
                .into_iter()
                .map(|(part, id)| DriftView {
                    part: part.name(),
                    id,
                })
                .collect(),
        })
    })
}

/// One cursor of a set as a data URL the card can draw. Takes the set and the
/// role, not a path, so only files a set names can be read. A card works
/// without its pictures, so a file that is missing or too big gives none.
#[tauri::command]
pub async fn theme_cursor_image(id: String, role: String) -> Result<Option<String>, String> {
    let Some(set) = windows_sets().into_iter().find(|set| set.id == id) else {
        return Err(format!("no cursor set {id:?}"));
    };
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
    /// The cursor role, or the sound event, by its registry name.
    role: String,
    /// What Windows calls a sound event in this language; cursors are named by
    /// the screen.
    label: Option<String>,
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

/// The event a `...\<app>\<event>\.Current` target belongs to, as `(app,
/// event)`.
fn sound_event_of(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_suffix(r"\.Current")?;
    let (rest, event) = rest.rsplit_once('\\')?;
    let (_, app) = rest.rsplit_once('\\')?;
    Some((app, event))
}

/// A journal target as a preview line: a cursor role, or a sound event with
/// the name Windows gives it.
fn line_of(
    target: &Target,
    events: &[SoundEvent],
    current: Option<Value>,
    next: Option<Value>,
) -> ThemeLine {
    let (role, label) = match target {
        Target::Registry { path, name } => match sound_event_of(path) {
            Some((app, event)) => (
                event.to_owned(),
                events
                    .iter()
                    .find(|known| known.app == app && known.event == event)
                    .map(|known| known.label.clone()),
            ),
            None => (name.clone(), None),
        },
        other => (other.to_string(), None),
    };
    ThemeLine {
        role,
        label,
        current,
        next,
    }
}

#[tauri::command]
pub async fn theme_preview(
    state: tauri::State<'_, TweakEngine>,
    part: String,
    id: String,
    on: bool,
    lang: String,
) -> Result<ThemePreview, String> {
    let part = part_named(&part)?;
    let events = platform::sounds::events();
    let themes = all(&lang, &events);
    let titles: Vec<(Part, String, String)> = themes
        .iter()
        .map(|theme| (theme.part(), theme.id.clone(), theme.title.clone()))
        .collect();
    let theme = find(themes, part, &id)?;
    with_engine(&state, |engine| {
        let active = themes::active(engine, part);
        let in_effect = |other: &str| {
            let name = themes::journal_id(part, other);
            engine
                .entries()
                .iter()
                .filter(move |entry| entry.tweak == name && entry.state == State::Applied)
        };
        let mut lines: Vec<(Target, Option<Value>)> = if on {
            theme
                .changes()
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
            // What the old theme set and this one does not goes back to what
            // it was before the old theme.
            for entry in in_effect(other) {
                if !lines.iter().any(|(target, _)| *target == entry.target) {
                    lines.push((entry.target.clone(), entry.before.value.clone()));
                }
            }
        }
        let lines = lines
            .into_iter()
            .map(|(target, next)| {
                let current = engine.current(&target).map_err(|error| error.to_string())?;
                Ok(line_of(&target, &events, current, next))
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
                    .find(|(found, id, _)| *found == part && *id == other)
                    .map_or(other, |(_, _, title)| title.clone())
            }),
        })
    })
}

#[tauri::command]
pub async fn theme_switch(
    state: tauri::State<'_, TweakEngine>,
    part: String,
    id: String,
    on: bool,
) -> Result<SwitchOutcome, String> {
    let part = part_named(&part)?;
    let theme = find(all("en", &platform::sounds::events()), part, &id)?;
    if let Some(version) = platform::system::windows_version()
        && !themes::requirements().availability(&version).applies()
    {
        return Ok(SwitchOutcome::NotAvailable);
    }
    let outcome = with_engine(&state, |engine| {
        switch(engine, part, &theme.id, &theme.changes(), on)
    })?;
    if part == Part::Cursors
        && matches!(
            outcome,
            SwitchOutcome::Done | SwitchOutcome::ChangedSince { .. }
        )
    {
        platform::shell::reload_cursors()?;
    }
    Ok(outcome)
}

/// On puts the theme's part on, taking off the theme that has it now. Off
/// undoes through the journal whatever LimeKit changed for it.
fn switch(
    engine: &mut Engine<CurrentUser>,
    part: Part,
    id: &str,
    changes: &[Change],
    on: bool,
) -> Result<SwitchOutcome, String> {
    if !on {
        let left_alone = themes::take_off(engine, part, id).map_err(|error| error.to_string())?;
        return Ok(if left_alone.is_empty() {
            SwitchOutcome::Done
        } else {
            SwitchOutcome::ChangedSince {
                targets: left_alone.iter().map(ToString::to_string).collect(),
            }
        });
    }
    match themes::put_on(engine, part, id, changes) {
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

    /// A set over a private key under HKCU and a journal in a temp folder,
    /// both gone after the test.
    struct Sandbox {
        root: String,
        changes: Vec<Change>,
        _dir: tempfile::TempDir,
        engine: Engine<CurrentUser>,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root = format!(r"Software\LimeKitTest-{}-themes-{name}", std::process::id());
            let dir = tempfile::tempdir().unwrap();
            let engine = Engine::open(CurrentUser, &dir.path().join("journal.jsonl")).unwrap();
            let set = CursorSet {
                id: "windows-test".into(),
                name: "Test".into(),
                roles: vec![(Role::Arrow, r"C:\test\arrow.cur".into())],
            };
            let sandbox = Sandbox {
                changes: themes::cursor_changes(&set, &format!(r"{root}\Cursors")),
                root,
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
            switch(
                &mut self.engine,
                Part::Cursors,
                "windows-test",
                &self.changes,
                on,
            )
            .unwrap()
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
    fn a_set_that_has_nothing_to_change_is_already_set() {
        let mut sandbox = Sandbox::new("empty");

        let outcome = switch(&mut sandbox.engine, Part::Sounds, "no-sounds", &[], true).unwrap();

        assert!(matches!(outcome, SwitchOutcome::AlreadySet));
    }

    #[test]
    fn only_a_still_picture_is_drawn_on_a_card() {
        assert!(is_picture(r"C:\Windows\Cursors\aero_arrow.cur"));
        assert!(is_picture(r"C:\X\ARROW.CUR"));
        assert!(!is_picture(r"C:\Windows\Cursors\aero_busy.ani"));
    }

    #[test]
    fn a_sound_target_names_its_program_and_event() {
        assert_eq!(
            sound_event_of(r"AppEvents\Schemes\Apps\Explorer\FaxSent\.Current"),
            Some(("Explorer", "FaxSent"))
        );
        assert_eq!(sound_event_of(r"Control Panel\Cursors"), None);
    }

    #[test]
    fn a_line_says_the_event_in_the_words_of_windows() {
        let events = [SoundEvent {
            app: ".Default".into(),
            event: "SystemAsterisk".into(),
            label: "Звёздочка".into(),
            sound: Some(r"C:\Windows\media\a.wav".into()),
        }];
        let sound = Target::Registry {
            path: r"AppEvents\Schemes\Apps\.Default\SystemAsterisk\.Current".into(),
            name: String::new(),
        };
        let cursor = Target::Registry {
            path: r"Control Panel\Cursors".into(),
            name: "Arrow".into(),
        };

        let line = line_of(&sound, &events, None, None);
        assert_eq!(
            (line.role.as_str(), line.label.as_deref()),
            ("SystemAsterisk", Some("Звёздочка"))
        );
        let line = line_of(&cursor, &events, None, None);
        assert_eq!((line.role.as_str(), line.label), ("Arrow", None));
    }
}
