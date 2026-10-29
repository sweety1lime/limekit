//! The themes screen's backend: the cursor sets Windows ships, a set that
//! silences the system sounds, and the themes imported from packages, each
//! with its standing on this Windows; putting them on and taking them off
//! through the tweak journal; and the library the packages are kept in.
//!
//! Themes go through the same engine as tweaks (`tweak_panel::TweakEngine`),
//! so there is one journal and one writer: this window process.

use std::path::Path;

use base64::Engine as _;
use platform::sounds::SoundEvent;
use serde::Serialize;
use themes::{CursorSet, LibraryTheme, Part, Role, SoundSet, SwitchError, ThemeError};
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

/// The registry keys themes write under: the real ones, or private ones in
/// tests.
struct Keys<'a> {
    cursors: &'a str,
    sounds: &'a str,
}

const REAL: Keys<'static> = Keys {
    cursors: themes::CURSORS_KEY,
    sounds: themes::SOUNDS_KEY,
};

/// One part of a theme.
enum Content {
    Cursors(CursorSet),
    Sounds(SoundSet),
}

impl Content {
    fn part(&self) -> Part {
        match self {
            Content::Cursors(_) => Part::Cursors,
            Content::Sounds(_) => Part::Sounds,
        }
    }

    fn changes(&self, keys: &Keys) -> Vec<Change> {
        match self {
            Content::Cursors(set) => themes::cursor_changes(set, keys.cursors),
            Content::Sounds(set) => themes::sound_changes(set, keys.sounds),
        }
    }
}

/// Where a theme comes from, for the chip on its card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Source {
    Windows,
    LimeKit,
    Package,
}

/// A theme as the screen lists it: one or two parts that go on and off
/// together.
struct Theme {
    id: String,
    title: String,
    description: String,
    source: Source,
    parts: Vec<Content>,
    /// Events a package sets that this Windows does not have, as
    /// `program/event`.
    skipped: Vec<String>,
}

impl Theme {
    fn changes(&self, part: Part, keys: &Keys) -> Vec<Change> {
        self.parts
            .iter()
            .filter(|content| content.part() == part)
            .flat_map(|content| content.changes(keys))
            .collect()
    }
}

/// How much of a theme is in effect.
struct Standing {
    /// The parts LimeKit has put on for this theme.
    in_effect: Vec<Part>,
    /// Every part that has something to change is among them.
    whole: bool,
}

impl Standing {
    fn of(engine: &Engine<CurrentUser>, theme: &Theme, keys: &Keys) -> Standing {
        let active = |part: Part| themes::active(engine, part).as_deref() == Some(&theme.id);
        let parts = || theme.parts.iter().map(Content::part);
        Standing {
            in_effect: parts().filter(|part| active(*part)).collect(),
            // A part with nothing to change, say a package's sounds that this
            // Windows has no event for, does not stand in the way.
            whole: parts().all(|part| active(part) || theme.changes(part, keys).is_empty()),
        }
    }
}

/// The library of imported packages, in the per-user data directory.
fn library() -> Result<themes::Library, String> {
    let data = platform::appdata::user_dir().ok_or("no per-user data directory on this system")?;
    Ok(themes::Library::at(data.join("themes")))
}

/// Whether a theme has been imported before, so the `.limetheme` association
/// is due. A fresh install, and anyone who only plays wallpapers, has none.
pub fn library_exists() -> bool {
    library().is_ok_and(|library| library.exists())
}

/// The cursor sets Windows ships, named in the current language.
fn windows_sets() -> Vec<CursorSet> {
    themes::windows_sets(
        &platform::cursors::system_schemes(),
        |path| themes::expand_env(path, |name| std::env::var(name).ok()),
        platform::shell::indirect_string,
    )
}

/// The cursor set of a theme, wherever it comes from.
fn cursor_set(id: &str) -> Option<CursorSet> {
    windows_sets()
        .into_iter()
        .find(|set| set.id == id)
        .or_else(|| library().ok()?.get(id)?.cursor_set())
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

/// Who made a theme and under what license, in `lang`. Either may be unknown,
/// as with a cursor pack that came without.
fn credit(author: &str, license: &str, lang: &str) -> String {
    let en = lang == "en";
    let (author_known, license_known) = (author != themes::UNSTATED, license != themes::UNSTATED);
    match (author_known, license_known) {
        (false, false) if en => "Author and license not stated.".to_owned(),
        (false, false) => "Автор и лицензия не указаны.".to_owned(),
        (false, true) if en => format!("Author not stated. License: {license}."),
        (false, true) => format!("Автор не указан. Лицензия: {license}."),
        (true, false) if en => format!("By {author}. License not stated."),
        (true, false) => format!("Автор: {author}. Лицензия не указана."),
        (true, true) if en => format!("By {author}. License: {license}."),
        (true, true) => format!("Автор: {author}. Лицензия: {license}."),
    }
}

/// A theme from a package, with the sounds as this Windows has them.
fn from_package(theme: &LibraryTheme, events: &[SoundEvent], lang: &str) -> Theme {
    let mut parts = Vec::new();
    if let Some(set) = theme.cursor_set() {
        parts.push(Content::Cursors(set));
    }
    let mut skipped = Vec::new();
    if let Some((set, left_out)) = theme.sound_set(events) {
        parts.push(Content::Sounds(set));
        skipped = left_out;
    }
    let manifest = &theme.manifest;
    Theme {
        id: manifest.id.clone(),
        title: manifest.name.clone(),
        description: credit(&manifest.author, &manifest.license, lang),
        source: Source::Package,
        parts,
        skipped,
    }
}

/// Every theme, with the sounds as they are now: the set that silences them
/// names the events that still make one.
fn all(lang: &str, events: &[SoundEvent], packages: &[LibraryTheme]) -> Vec<Theme> {
    let mut all: Vec<Theme> = windows_sets()
        .into_iter()
        .map(|set| Theme {
            id: set.id.clone(),
            title: set.name.clone(),
            description: String::new(),
            source: Source::Windows,
            parts: vec![Content::Cursors(set)],
            skipped: Vec::new(),
        })
        .collect();
    let (title, description) = no_sounds_text(lang);
    all.push(Theme {
        id: themes::NO_SOUNDS.to_owned(),
        title: title.to_owned(),
        description: description.to_owned(),
        source: Source::LimeKit,
        parts: vec![Content::Sounds(themes::silence(events))],
        skipped: Vec::new(),
    });
    all.extend(
        packages
            .iter()
            .map(|theme| from_package(theme, events, lang)),
    );
    all
}

/// The themes in the library; an unreadable library has none.
fn packages() -> Vec<LibraryTheme> {
    library().map(|library| library.list()).unwrap_or_default()
}

fn find<'a>(themes: &'a [Theme], id: &str) -> Result<&'a Theme, String> {
    themes
        .iter()
        .find(|theme| theme.id == id)
        .ok_or_else(|| format!("no theme {id:?}"))
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
    description: String,
    source: Source,
    /// "cursors" and "sounds", whichever the theme has.
    parts: Vec<&'static str>,
    availability: AvailabilityView,
    /// LimeKit has some of this theme in effect, so taking it off gives back
    /// what was there before LimeKit.
    applied: bool,
    /// The parts in effect when that is not the whole theme, say a package's
    /// sounds after a cursor set took its cursors over. Empty when the theme is
    /// in effect whole or not at all.
    partly: Vec<&'static str>,
    /// Roles the card can show a picture of, by registry name.
    pictures: Vec<&'static str>,
    /// A theme from a package can be deleted from the library.
    removable: bool,
}

#[tauri::command]
pub async fn themes_view(
    state: tauri::State<'_, TweakEngine>,
    lang: String,
) -> Result<ThemesView, String> {
    let version = platform::system::windows_version();
    let requirements = themes::requirements();
    let sets = all(&lang, &platform::sounds::events(), &packages());
    with_engine(&state, |engine| {
        let views = sets
            .iter()
            .map(|theme| {
                let standing = Standing::of(engine, theme, &REAL);
                ThemeView {
                    id: theme.id.clone(),
                    title: theme.title.clone(),
                    description: theme.description.clone(),
                    source: theme.source,
                    parts: theme
                        .parts
                        .iter()
                        .map(|content| content.part().name())
                        .collect(),
                    availability: version
                        .as_ref()
                        .map_or(Availability::Unverified, |version| {
                            requirements.availability(version)
                        })
                        .into(),
                    applied: !standing.in_effect.is_empty(),
                    partly: if standing.whole {
                        Vec::new()
                    } else {
                        standing.in_effect.iter().map(|part| part.name()).collect()
                    },
                    pictures: theme
                        .parts
                        .iter()
                        .filter_map(|content| match content {
                            Content::Cursors(set) => Some(set),
                            Content::Sounds(_) => None,
                        })
                        .flat_map(|set| {
                            CARD_ROLES
                                .iter()
                                .filter(|role| set.path(**role).is_some_and(is_picture))
                                .map(|role| role.name())
                        })
                        .collect(),
                    removable: theme.source == Source::Package,
                }
            })
            .collect();
        let mut drift: Vec<String> = Vec::new();
        for item in engine.drift().map_err(|error| error.to_string())? {
            let found = engine
                .entries()
                .iter()
                .find(|entry| entry.id == item.entry)
                .and_then(|entry| themes::parse_journal_id(&entry.tweak))
                .map(|(_, theme)| theme.to_owned());
            if let Some(found) = found
                && !drift.contains(&found)
            {
                drift.push(found);
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
    let Some(set) = cursor_set(&id) else {
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
    /// "cursors" or "sounds".
    part: &'static str,
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
    /// The themes in effect that this one takes off first.
    replaces: Vec<String>,
    /// Events the theme names that this Windows does not have.
    skipped: Vec<String>,
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
    part: Part,
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
        part: part.name(),
        role,
        label,
        current,
        next,
    }
}

#[tauri::command]
pub async fn theme_preview(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
    lang: String,
) -> Result<ThemePreview, String> {
    let events = platform::sounds::events();
    let all = all(&lang, &events, &packages());
    let theme = find(&all, &id)?;
    with_engine(&state, |engine| {
        let mut lines = Vec::new();
        let mut replaces: Vec<String> = Vec::new();
        for content in &theme.parts {
            let part = content.part();
            let in_effect = |other: &str| {
                let name = themes::journal_id(part, other);
                engine
                    .entries()
                    .iter()
                    .filter(move |entry| entry.tweak == name && entry.state == State::Applied)
            };
            let mut shown: Vec<(Target, Option<Value>)> = if on {
                content
                    .changes(&REAL)
                    .into_iter()
                    .map(|change| (change.target, change.value))
                    .collect()
            } else {
                // Off gives back what was there before LimeKit.
                in_effect(&id)
                    .map(|entry| (entry.target.clone(), entry.before.value.clone()))
                    .collect()
            };
            let replaced = themes::active(engine, part).filter(|other| on && *other != id);
            if let Some(other) = &replaced {
                // What the old theme set and this one does not goes back to
                // what it was before the old theme.
                for entry in in_effect(other) {
                    if !shown.iter().any(|(target, _)| *target == entry.target) {
                        shown.push((entry.target.clone(), entry.before.value.clone()));
                    }
                }
                let title = all
                    .iter()
                    .find(|found| found.id == *other)
                    .map_or_else(|| other.clone(), |found| found.title.clone());
                if !replaces.contains(&title) {
                    replaces.push(title);
                }
            }
            for (target, next) in shown {
                let current = engine.current(&target).map_err(|error| error.to_string())?;
                lines.push(line_of(part, &target, &events, current, next));
            }
        }
        lines.retain(|line| line.current != line.next);
        Ok(ThemePreview {
            lines,
            replaces,
            skipped: theme.skipped.clone(),
        })
    })
}

#[tauri::command]
pub async fn theme_switch(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
) -> Result<SwitchOutcome, String> {
    let all = all("en", &platform::sounds::events(), &packages());
    let theme = find(&all, &id)?;
    if let Some(version) = platform::system::windows_version()
        && !themes::requirements().availability(&version).applies()
    {
        return Ok(SwitchOutcome::NotAvailable);
    }
    let outcome = with_engine(&state, |engine| switch_theme(engine, theme, on, &REAL))?;
    let has_cursors = theme
        .parts
        .iter()
        .any(|content| content.part() == Part::Cursors);
    if has_cursors
        && matches!(
            outcome,
            SwitchOutcome::Done | SwitchOutcome::ChangedSince { .. }
        )
    {
        platform::shell::reload_cursors()?;
    }
    Ok(outcome)
}

/// Puts every part of the theme on, or takes every part off. A part Windows
/// refuses takes back the parts this call had already put on, so a theme goes
/// on whole or not at all.
fn switch_theme(
    engine: &mut Engine<CurrentUser>,
    theme: &Theme,
    on: bool,
    keys: &Keys,
) -> Result<SwitchOutcome, String> {
    let mut changed_since: Vec<String> = Vec::new();
    let mut nothing_changed = true;
    let mut touched: Vec<Part> = Vec::new();
    for content in &theme.parts {
        let part = content.part();
        match switch(engine, part, &theme.id, &content.changes(keys), on)? {
            SwitchOutcome::Done => {
                nothing_changed = false;
                touched.push(part);
            }
            SwitchOutcome::AlreadySet => {}
            SwitchOutcome::ChangedSince { targets } => {
                nothing_changed = false;
                changed_since.extend(targets);
                touched.push(part);
            }
            refused => {
                for done in touched.iter().rev() {
                    // Best effort: what cannot be undone stays in the journal,
                    // where "Вернуть всё" finds it.
                    let _ = themes::take_off(engine, *done, &theme.id);
                }
                return Ok(refused);
            }
        }
    }
    Ok(if !changed_since.is_empty() {
        SwitchOutcome::ChangedSince {
            targets: changed_since,
        }
    } else if on && nothing_changed {
        SwitchOutcome::AlreadySet
    } else {
        SwitchOutcome::Done
    })
}

/// On puts a part on, taking off the theme that has it now. Off undoes through
/// the journal whatever LimeKit changed for it.
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

/// A theme an import added.
#[derive(Clone, Serialize)]
pub struct Imported {
    pub id: String,
    pub name: String,
}

/// What importing one path came to: the themes it added, and what a cursor pack
/// left out, for the window to say.
#[derive(Clone, Default, Serialize)]
pub struct ImportResult {
    pub themes: Vec<Imported>,
    /// Files a cursor pack names that it does not have.
    pub missing: Vec<String>,
    /// Schemes of a cursor pack that could not be added, each with the reason.
    pub skipped: Vec<String>,
}

/// A refusal in words a person can act on. The ones people meet most are said
/// in Russian, as the rest of the window is; the others keep the library's
/// own English.
fn describe(error: &ThemeError) -> String {
    match error {
        ThemeError::WallpaperPackage => {
            "Это пакет обоев, а не тема: добавьте его в библиотеку обоев.".to_owned()
        }
        ThemeError::MissingDescriptor => "В пакете нет theme.json: это не тема LimeKit.".to_owned(),
        ThemeError::NewerFormat(format) => {
            format!("Тема сделана в формате {format}, который эта версия LimeKit не читает.")
        }
        ThemeError::AlreadyInLibrary(id) => format!("Тема «{id}» уже в библиотеке."),
        ThemeError::NotAPack => {
            "Это не пакет курсоров: нужна папка, zip-архив или файл .inf.".to_owned()
        }
        ThemeError::NoInf => {
            "В пакете нет файла .inf, по которому видно, какой курсор для чего.".to_owned()
        }
        ThemeError::NoScheme => "В файле .inf нет схемы курсоров.".to_owned(),
        ThemeError::NoCursors => "Ни одного курсора из файла .inf в пакете нет.".to_owned(),
        ThemeError::TooManySchemes(most) => {
            format!("В пакете больше {most} схем курсоров: это слишком много за один раз.")
        }
        other => other.to_string(),
    }
}

/// Adds the package or the cursor pack at `path` to the library. An import also
/// registers the `.limetheme` association, so a double click finds the program.
pub fn import_path(path: &Path) -> Result<ImportResult, String> {
    // The data directory still looks empty while the LimeWall one has not been
    // moved over. Creating the themes folder now would make the half-made
    // directory look real, and the old data would never be brought over.
    if platform::appdata::migration_pending() {
        return Err(crate::MIGRATION_PENDING.to_owned());
    }
    let library = library()?;
    let result = if themes::has_theme_extension(path) {
        let theme = library.import(path).map_err(|error| describe(&error))?;
        ImportResult {
            themes: vec![imported(&theme)],
            ..ImportResult::default()
        }
    } else {
        let pack = library
            .import_pack(path)
            .map_err(|error| describe(&error))?;
        ImportResult {
            themes: pack.themes.iter().map(imported).collect(),
            missing: pack.missing,
            skipped: pack.skipped,
        }
    };
    if let Err(error) = crate::assoc::register_theme() {
        eprintln!("theme association not registered: {error}");
    }
    Ok(result)
}

fn imported(theme: &LibraryTheme) -> Imported {
    Imported {
        id: theme.id().to_owned(),
        name: theme.manifest.name.clone(),
    }
}

#[tauri::command]
pub async fn theme_import(path: String) -> Result<ImportResult, String> {
    tauri::async_runtime::spawn_blocking(move || import_path(Path::new(&path)))
        .await
        .map_err(|error| error.to_string())?
}

/// Deletes a theme from the library. A theme in effect is taken off first,
/// because the registry points at its files. Values something changed since
/// stay as they are and come back as the result.
#[tauri::command]
pub async fn theme_remove(
    state: tauri::State<'_, TweakEngine>,
    id: String,
) -> Result<Vec<String>, String> {
    let library = library()?;
    library
        .get(&id)
        .ok_or_else(|| format!("no theme {id:?} in the library"))?;
    let (left_alone, had_cursors) = with_engine(&state, |engine| {
        let had_cursors = themes::active(engine, Part::Cursors).as_deref() == Some(id.as_str());
        let mut left_alone = Vec::new();
        for part in [Part::Cursors, Part::Sounds] {
            left_alone.extend(
                themes::take_off(engine, part, &id)
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(ToString::to_string),
            );
        }
        Ok((left_alone, had_cursors))
    })?;
    library.remove(&id).map_err(|error| describe(&error))?;
    if had_cursors {
        platform::shell::reload_cursors()?;
    }
    Ok(left_alone)
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use platform::registry::{self, RawValue};

    /// Private keys under HKCU and a journal in a temp folder, all gone after
    /// the test.
    struct Sandbox {
        root: String,
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
                _dir: dir,
                engine,
            };
            sandbox.write_arrow(r"C:\original\arrow.cur");
            registry::write_value(
                &sandbox.close_key(),
                "",
                &Value::Sz(r"C:\Windows\media\close.wav".into()).to_raw(),
            )
            .unwrap();
            sandbox
        }

        fn cursors_key(&self) -> String {
            format!(r"{}\Cursors", self.root)
        }

        fn sounds_key(&self) -> String {
            format!(r"{}\Apps", self.root)
        }

        fn close_key(&self) -> String {
            format!(r"{}\.Default\Close\.Current", self.sounds_key())
        }

        fn write_arrow(&self, path: &str) {
            registry::write_value(
                &self.cursors_key(),
                "Arrow",
                &Value::Sz(path.into()).to_raw(),
            )
            .unwrap();
        }

        fn arrow(&self) -> Option<RawValue> {
            registry::read_value(&self.cursors_key(), "Arrow").unwrap()
        }

        fn close(&self) -> Option<RawValue> {
            registry::read_value(&self.close_key(), "").unwrap()
        }

        /// A theme with both parts over the private keys.
        fn theme(&self) -> Theme {
            let sounds = themes::silence(&[SoundEvent {
                app: ".Default".into(),
                event: "Close".into(),
                label: "Закрытие программы".into(),
                sound: Some(r"C:\Windows\media\close.wav".into()),
            }]);
            Theme {
                id: "both".into(),
                title: "Both".into(),
                description: String::new(),
                source: Source::Package,
                parts: vec![
                    Content::Cursors(CursorSet {
                        id: "both".into(),
                        name: "Both".into(),
                        roles: vec![(Role::Arrow, r"C:\test\arrow.cur".into())],
                    }),
                    Content::Sounds(sounds),
                ],
                skipped: Vec::new(),
            }
        }

        /// A theme with cursors only, which takes the cursors over from
        /// [`Sandbox::theme`] and leaves its sounds.
        fn cursors_only(&self) -> Theme {
            Theme {
                id: "cursors-only".into(),
                title: "Cursors only".into(),
                parts: vec![Content::Cursors(CursorSet {
                    id: "cursors-only".into(),
                    name: "Cursors only".into(),
                    roles: vec![(Role::Arrow, r"C:\test\other.cur".into())],
                })],
                ..self.theme()
            }
        }

        fn switch(&mut self, theme: &Theme, on: bool) -> SwitchOutcome {
            let cursors = self.cursors_key();
            let sounds = self.sounds_key();
            let keys = Keys {
                cursors: &cursors,
                sounds: &sounds,
            };
            switch_theme(&mut self.engine, theme, on, &keys).unwrap()
        }

        fn standing(&self, theme: &Theme) -> Standing {
            let cursors = self.cursors_key();
            let sounds = self.sounds_key();
            let keys = Keys {
                cursors: &cursors,
                sounds: &sounds,
            };
            Standing::of(&self.engine, theme, &keys)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.root);
        }
    }

    fn sz(text: &str) -> Option<RawValue> {
        Some(Value::Sz(text.into()).to_raw())
    }

    #[test]
    fn a_theme_with_both_parts_goes_on_and_off_as_one() {
        let mut sandbox = Sandbox::new("both");
        let theme = sandbox.theme();

        assert!(matches!(sandbox.switch(&theme, true), SwitchOutcome::Done));
        assert_eq!(sandbox.arrow(), sz(r"C:\test\arrow.cur"));
        assert_eq!(sandbox.close(), sz(""));
        assert!(matches!(sandbox.switch(&theme, false), SwitchOutcome::Done));

        assert_eq!(sandbox.arrow(), sz(r"C:\original\arrow.cur"));
        assert_eq!(sandbox.close(), sz(r"C:\Windows\media\close.wav"));
    }

    #[test]
    fn a_theme_is_in_effect_whole_or_not_at_all_when_nothing_takes_a_part() {
        let mut sandbox = Sandbox::new("standing");
        let theme = sandbox.theme();

        let before = sandbox.standing(&theme);
        assert!(before.in_effect.is_empty() && !before.whole);

        sandbox.switch(&theme, true);
        let on = sandbox.standing(&theme);
        assert_eq!(on.in_effect, [Part::Cursors, Part::Sounds]);
        assert!(on.whole);

        sandbox.switch(&theme, false);
        assert!(sandbox.standing(&theme).in_effect.is_empty());
    }

    #[test]
    fn a_theme_that_lost_its_cursors_to_another_keeps_its_sounds_and_can_be_put_back() {
        let mut sandbox = Sandbox::new("partly");
        let both = sandbox.theme();
        let cursors_only = sandbox.cursors_only();
        sandbox.switch(&both, true);

        sandbox.switch(&cursors_only, true);

        // The other theme has the cursors; the sounds are still the first one's.
        assert_eq!(sandbox.arrow(), sz(r"C:\test\other.cur"));
        assert_eq!(sandbox.close(), sz(""));
        let left = sandbox.standing(&both);
        assert_eq!(left.in_effect, [Part::Sounds]);
        assert!(!left.whole, "the screen has to say it is only partly on");
        let taker = sandbox.standing(&cursors_only);
        assert_eq!(taker.in_effect, [Part::Cursors]);
        assert!(taker.whole);

        // Put back takes off what is left of it, and not what is not its own.
        assert!(matches!(sandbox.switch(&both, false), SwitchOutcome::Done));
        assert_eq!(sandbox.close(), sz(r"C:\Windows\media\close.wav"));
        assert_eq!(sandbox.arrow(), sz(r"C:\test\other.cur"));
        assert!(sandbox.standing(&both).in_effect.is_empty());
        assert!(sandbox.standing(&cursors_only).whole);
    }

    #[test]
    fn on_twice_changes_nothing_the_second_time() {
        let mut sandbox = Sandbox::new("twice");
        let theme = sandbox.theme();
        sandbox.switch(&theme, true);

        assert!(matches!(
            sandbox.switch(&theme, true),
            SwitchOutcome::AlreadySet
        ));
    }

    #[test]
    fn a_change_after_the_theme_is_left_alone_and_the_other_part_still_comes_off() {
        let mut sandbox = Sandbox::new("changed");
        let theme = sandbox.theme();
        sandbox.switch(&theme, true);
        sandbox.write_arrow(r"C:\mine\arrow.cur");

        let outcome = sandbox.switch(&theme, false);

        assert!(
            matches!(outcome, SwitchOutcome::ChangedSince { ref targets } if targets.len() == 1)
        );
        assert_eq!(sandbox.arrow(), sz(r"C:\mine\arrow.cur"));
        assert_eq!(sandbox.close(), sz(r"C:\Windows\media\close.wav"));
    }

    #[test]
    fn a_theme_whose_sounds_are_already_silent_goes_on_through_its_cursors() {
        let mut sandbox = Sandbox::new("silent");
        registry::write_value(&sandbox.close_key(), "", &Value::Sz(String::new()).to_raw())
            .unwrap();
        let mut theme = sandbox.theme();
        // Nothing is left to silence, so the sound set is empty.
        theme.parts[1] = Content::Sounds(themes::silence(&[]));

        assert!(matches!(sandbox.switch(&theme, true), SwitchOutcome::Done));

        assert_eq!(sandbox.arrow(), sz(r"C:\test\arrow.cur"));
    }

    #[test]
    fn a_theme_that_has_nothing_to_change_is_already_set() {
        let mut sandbox = Sandbox::new("empty");
        let theme = Theme {
            parts: vec![Content::Sounds(themes::silence(&[]))],
            ..sandbox.theme()
        };

        assert!(matches!(
            sandbox.switch(&theme, true),
            SwitchOutcome::AlreadySet
        ));
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

        let line = line_of(Part::Sounds, &sound, &events, None, None);
        assert_eq!(
            (line.part, line.role.as_str(), line.label.as_deref()),
            ("sounds", "SystemAsterisk", Some("Звёздочка"))
        );
        let line = line_of(Part::Cursors, &cursor, &events, None, None);
        assert_eq!(
            (line.part, line.role.as_str(), line.label),
            ("cursors", "Arrow", None)
        );
    }

    #[test]
    fn what_is_not_known_of_a_theme_is_said_in_the_words_of_the_language() {
        assert_eq!(
            credit("A. Author", "CC-BY-4.0", "ru"),
            "Автор: A. Author. Лицензия: CC-BY-4.0."
        );
        assert_eq!(
            credit("A. Author", "CC-BY-4.0", "en"),
            "By A. Author. License: CC-BY-4.0."
        );
        assert_eq!(
            credit(themes::UNSTATED, themes::UNSTATED, "ru"),
            "Автор и лицензия не указаны."
        );
        assert_eq!(
            credit(themes::UNSTATED, "MIT", "en"),
            "Author not stated. License: MIT."
        );
        assert_eq!(
            credit("Me", themes::UNSTATED, "ru"),
            "Автор: Me. Лицензия не указана."
        );
    }

    #[test]
    fn the_refusals_for_a_cursor_pack_are_said_in_russian_too() {
        assert!(describe(&ThemeError::NotAPack).contains("папка, zip-архив или файл .inf"));
        assert!(describe(&ThemeError::NoInf).contains("нет файла .inf"));
        assert!(describe(&ThemeError::NoScheme).contains("нет схемы курсоров"));
        assert!(describe(&ThemeError::NoCursors).contains("Ни одного курсора"));
        assert!(describe(&ThemeError::TooManySchemes(40)).contains("больше 40"));
    }

    #[test]
    fn the_refusals_people_meet_are_said_in_russian() {
        assert!(describe(&ThemeError::WallpaperPackage).contains("пакет обоев"));
        assert!(describe(&ThemeError::MissingDescriptor).contains("theme.json"));
        assert!(describe(&ThemeError::AlreadyInLibrary("x".into())).contains("уже в библиотеке"));
        assert_eq!(
            describe(&ThemeError::TooBig),
            ThemeError::TooBig.to_string()
        );
    }
}
