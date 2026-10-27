//! The tweaks screen's backend: the catalog with each tweak's standing on this
//! Windows, and switching tweaks through the journal.
//!
//! This window process owns the journal. Tweaks are switched from here, and
//! `LimeKit.exe --cleanup` undoes them from a process of this same program
//! after the window has closed, so two writers never meet.

use std::path::PathBuf;
use std::sync::Mutex;

use platform::system::WindowsVersion;
use serde::Serialize;
use tweaks::{
    Availability, Catalog, CurrentUser, Edition, Engine, EngineError, EntryId, Outcome, Provenance,
    RevertOutcome, State, TakesEffect, Target, Text, Tweak, TweakState, Value,
};

/// The engine, opened on first use.
#[derive(Default)]
pub struct TweakEngine(Mutex<Option<Engine<CurrentUser>>>);

pub fn journal_path() -> Option<PathBuf> {
    Some(
        platform::appdata::user_dir()?
            .join("journal")
            .join("journal.jsonl"),
    )
}

fn with_engine<T>(
    state: &TweakEngine,
    work: impl FnOnce(&mut Engine<CurrentUser>) -> Result<T, String>,
) -> Result<T, String> {
    let mut slot = state
        .0
        .lock()
        .map_err(|_| "the tweak engine stopped after an error".to_owned())?;
    let engine = match slot.as_mut() {
        Some(engine) => engine,
        None => {
            let path = journal_path().ok_or("no data directory for the journal")?;
            slot.insert(Engine::open(CurrentUser, &path).map_err(|error| error.to_string())?)
        }
    };
    work(engine)
}

fn catalog() -> Result<Catalog, String> {
    Catalog::builtin().map_err(|error| error.to_string())
}

fn find<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Tweak, String> {
    catalog
        .get(id)
        .ok_or_else(|| format!("no tweak {id:?} in the catalog"))
}

fn text(text: &Text, lang: &str) -> String {
    if lang == "en" { &text.en } else { &text.ru }.clone()
}

/// Changes of `tweak` LimeKit has in effect, newest first.
fn applied(engine: &Engine<CurrentUser>, tweak: &str) -> Vec<EntryId> {
    engine
        .entries()
        .iter()
        .rev()
        .filter(|entry| entry.tweak == tweak && entry.state == State::Applied)
        .map(|entry| entry.id)
        .collect()
}

#[derive(Serialize)]
pub struct TweaksView {
    /// "Windows 11 24H2, 26100.4061, Pro"; empty when unknown.
    windows: String,
    tweaks: Vec<TweakView>,
    /// Tweaks whose values something changed after LimeKit set them.
    drift: Vec<String>,
}

#[derive(Serialize)]
pub struct TweakView {
    id: String,
    title: String,
    description: String,
    provenance: Provenance,
    takes_effect: TakesEffect,
    availability: AvailabilityView,
    /// "on", "off" or "mixed".
    state: &'static str,
    /// LimeKit has this tweak's changes in effect, so off gives back what was
    /// there before.
    applied: bool,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AvailabilityView {
    Verified,
    Unverified,
    BuiltIn { settings_uri: Option<String> },
    TooOld { build: String },
    TooNew { build: String },
    WrongEdition { editions: Vec<Edition> },
}

impl From<Availability> for AvailabilityView {
    fn from(availability: Availability) -> Self {
        match availability {
            Availability::Verified => AvailabilityView::Verified,
            Availability::Unverified => AvailabilityView::Unverified,
            Availability::BuiltIn { settings_uri } => AvailabilityView::BuiltIn { settings_uri },
            Availability::TooOld { min } => AvailabilityView::TooOld {
                build: min.to_string(),
            },
            Availability::TooNew { below } => AvailabilityView::TooNew {
                build: below.to_string(),
            },
            Availability::WrongEdition { editions } => AvailabilityView::WrongEdition { editions },
        }
    }
}

fn describe_windows(version: &WindowsVersion) -> String {
    let name = if version.build >= 22000 {
        "Windows 11"
    } else {
        "Windows 10"
    };
    let edition = match Edition::of(&version.edition_id) {
        Some(Edition::Home) => "Home",
        Some(Edition::Pro) => "Pro",
        Some(Edition::Enterprise) => "Enterprise",
        Some(Edition::Education) => "Education",
        Some(Edition::Server) => "Server",
        None => version.edition_id.as_str(),
    };
    let release = if version.display_version.is_empty() {
        String::new()
    } else {
        format!(" {}", version.display_version)
    };
    format!(
        "{name}{release}, {}.{}, {edition}",
        version.build, version.ubr
    )
}

#[tauri::command]
pub async fn tweaks_view(
    state: tauri::State<'_, TweakEngine>,
    lang: String,
) -> Result<TweaksView, String> {
    let catalog = catalog()?;
    let version = platform::system::windows_version();
    with_engine(&state, |engine| {
        let mut tweaks = Vec::new();
        for tweak in &catalog.tweaks {
            let availability = version
                .as_ref()
                .map_or(Availability::Unverified, |version| {
                    tweak.availability(version)
                });
            let state = match tweak
                .state(&CurrentUser)
                .map_err(|error| error.to_string())?
            {
                TweakState::On => "on",
                TweakState::Off => "off",
                TweakState::Mixed => "mixed",
            };
            tweaks.push(TweakView {
                id: tweak.id.clone(),
                title: text(&tweak.title, &lang),
                description: text(&tweak.description, &lang),
                provenance: tweak.provenance,
                takes_effect: tweak.takes_effect,
                availability: availability.into(),
                state,
                applied: !applied(engine, &tweak.id).is_empty(),
            });
        }
        let mut drift: Vec<String> = Vec::new();
        for item in engine.drift().map_err(|error| error.to_string())? {
            let tweak = engine
                .entries()
                .iter()
                .find(|entry| entry.id == item.entry)
                .map(|entry| entry.tweak.clone());
            if let Some(tweak) = tweak
                && !drift.contains(&tweak)
            {
                drift.push(tweak);
            }
        }
        Ok(TweaksView {
            windows: version.as_ref().map(describe_windows).unwrap_or_default(),
            tweaks,
            drift,
        })
    })
}

/// One line of "what will change".
#[derive(Serialize)]
pub struct PreviewLine {
    target: String,
    /// A system parameter, whose values are on (1) and off (0).
    parameter: bool,
    current: Option<Value>,
    next: Option<Value>,
}

#[tauri::command]
pub async fn tweak_preview(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
) -> Result<Vec<PreviewLine>, String> {
    let catalog = catalog()?;
    let tweak = find(&catalog, &id)?;
    with_engine(&state, |engine| {
        let reverting = applied(engine, &id);
        let lines = if !on && !reverting.is_empty() {
            // Off gives back what was there before LimeKit.
            engine
                .entries()
                .iter()
                .filter(|entry| reverting.contains(&entry.id))
                .map(|entry| (entry.target.clone(), entry.before.value.clone()))
                .collect()
        } else {
            tweak
                .changes(on)
                .into_iter()
                .map(|change| (change.target, change.value))
                .collect::<Vec<_>>()
        };
        lines
            .into_iter()
            .map(|(target, next)| {
                Ok(PreviewLine {
                    parameter: matches!(target, Target::Parameter { .. }),
                    current: engine.current(&target).map_err(|error| error.to_string())?,
                    target: target.to_string(),
                    next,
                })
            })
            .collect()
    })
}

/// What switching a tweak did, for the screen to put into words.
#[derive(Serialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum SwitchOutcome {
    Done,
    /// Everything already was as asked.
    AlreadySet,
    /// These values were changed after LimeKit set them and were left alone.
    ChangedSince {
        targets: Vec<String>,
    },
    /// Windows refused to change this value.
    AccessDenied {
        target: String,
    },
    /// Windows accepted the change but kept its own value.
    NotTaken {
        target: String,
    },
    /// The tweak is not for this Windows.
    NotAvailable,
}

#[tauri::command]
pub async fn tweak_switch(
    state: tauri::State<'_, TweakEngine>,
    id: String,
    on: bool,
) -> Result<SwitchOutcome, String> {
    let catalog = catalog()?;
    let tweak = find(&catalog, &id)?;
    if let Some(version) = platform::system::windows_version()
        && !tweak.availability(&version).applies()
    {
        return Ok(SwitchOutcome::NotAvailable);
    }
    let outcome = with_engine(&state, |engine| switch(engine, tweak, on))?;
    if matches!(
        outcome,
        SwitchOutcome::Done | SwitchOutcome::ChangedSince { .. }
    ) {
        announce([tweak]);
    }
    Ok(outcome)
}

/// Tells open windows about what `tweaks` changed, once per area.
fn announce<'a>(tweaks: impl IntoIterator<Item = &'a Tweak>) {
    let mut done: Vec<&str> = Vec::new();
    for area in tweaks
        .into_iter()
        .filter_map(|tweak| tweak.notify.as_deref())
    {
        if !done.contains(&area) {
            platform::shell::announce_setting_change(area);
            done.push(area);
        }
    }
}

/// On writes the tweak's values. Off undoes through the journal whatever
/// LimeKit changed for it, so a value someone had set by hand comes back; with
/// nothing to undo it writes the tweak's off values, the Windows defaults.
fn switch(
    engine: &mut Engine<CurrentUser>,
    tweak: &Tweak,
    on: bool,
) -> Result<SwitchOutcome, String> {
    let reverting = applied(engine, &tweak.id);
    if !on && !reverting.is_empty() {
        let mut changed = Vec::new();
        for entry in reverting {
            if let RevertOutcome::ChangedSince { .. } = engine
                .revert(entry, false)
                .map_err(|error| error.to_string())?
                && let Some(found) = engine.entries().iter().find(|item| item.id == entry)
            {
                changed.push(found.target.to_string());
            }
        }
        return Ok(if changed.is_empty() {
            SwitchOutcome::Done
        } else {
            SwitchOutcome::ChangedSince { targets: changed }
        });
    }
    match engine.apply(&tweak.id, &tweak.changes(on)) {
        Ok((_, outcomes))
            if outcomes
                .iter()
                .all(|outcome| *outcome == Outcome::AlreadySet) =>
        {
            Ok(SwitchOutcome::AlreadySet)
        }
        Ok(_) => Ok(SwitchOutcome::Done),
        Err(error) if error.access_denied() => Ok(SwitchOutcome::AccessDenied {
            target: error.target.to_string(),
        }),
        Err(error) => match *error.cause {
            EngineError::NotTaken { target, .. } => Ok(SwitchOutcome::NotTaken {
                target: target.to_string(),
            }),
            cause => Err(cause.to_string()),
        },
    }
}

/// What [`tweaks_undo_all`] did.
#[derive(Serialize, Default)]
pub struct UndoAll {
    pub reverted: usize,
    /// Values changed after LimeKit set them, left alone.
    pub changed_since: Vec<String>,
    /// Something undone shows only once Explorer restarts.
    pub restart_explorer: bool,
}

fn undo_all(engine: &mut Engine<CurrentUser>) -> Result<UndoAll, String> {
    let mut summary = UndoAll::default();
    let mut undone: Vec<String> = Vec::new();
    for (id, outcome) in engine
        .revert_all(false)
        .map_err(|error| error.to_string())?
    {
        let Some(entry) = engine.entries().iter().find(|entry| entry.id == id) else {
            continue;
        };
        match outcome {
            RevertOutcome::Reverted => {
                summary.reverted += 1;
                if !undone.contains(&entry.tweak) {
                    undone.push(entry.tweak.clone());
                }
            }
            RevertOutcome::ChangedSince { .. } => {
                summary.changed_since.push(entry.target.to_string());
            }
            RevertOutcome::NotApplied => {}
        }
    }
    let catalog = catalog()?;
    let undone: Vec<&Tweak> = catalog
        .tweaks
        .iter()
        .filter(|tweak| undone.contains(&tweak.id))
        .collect();
    summary.restart_explorer = undone
        .iter()
        .any(|tweak| tweak.takes_effect == TakesEffect::ExplorerRestart);
    announce(undone);
    Ok(summary)
}

#[tauri::command]
pub async fn tweaks_undo_all(state: tauri::State<'_, TweakEngine>) -> Result<UndoAll, String> {
    with_engine(&state, undo_all)
}

/// Restarts Explorer for the tweaks it reads only when it starts. The Restart
/// Manager reopens its folder windows; the daemon puts the wallpapers back on
/// the new desktop.
#[tauri::command]
pub async fn restart_explorer() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(platform::shell::restart_explorer)
        .await
        .map_err(|error| error.to_string())?
}

/// Undoes every tweak for `LimeKit.exe --cleanup`. Without a journal there is
/// nothing to undo, and opening one would create it.
pub fn undo_all_for_cleanup() -> Result<UndoAll, String> {
    let Some(path) = journal_path().filter(|path| path.is_file()) else {
        return Ok(UndoAll::default());
    };
    let mut engine = Engine::open(CurrentUser, &path).map_err(|error| error.to_string())?;
    undo_all(&mut engine)
}

#[tauri::command]
pub async fn tweaks_export_undo(
    state: tauri::State<'_, TweakEngine>,
    path: String,
) -> Result<(), String> {
    with_engine(&state, |engine| {
        std::fs::write(&path, tweaks::undo_reg_file(engine.entries()))
            .map_err(|error| format!("{path}: {error}"))
    })
}

/// Opens the Windows Settings page that does what a tweak does. Takes the
/// tweak, not a link, so the page can only be one the catalog names; the
/// catalog checks those are `ms-settings:` links.
#[tauri::command]
pub async fn tweak_open_settings(app: tauri::AppHandle, id: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let catalog = catalog()?;
    let uri = find(&catalog, &id)?
        .requires
        .settings_uri
        .clone()
        .ok_or_else(|| format!("{id} has no settings page"))?;
    app.opener()
        .open_url(uri, None::<&str>)
        .map_err(|error| error.to_string())
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;
    use platform::registry::{self, RawValue};

    /// A tweak over a private key under HKCU and a journal in a temp folder,
    /// both gone after the test.
    struct Sandbox {
        root: String,
        catalog: Catalog,
        _dir: tempfile::TempDir,
        engine: Engine<CurrentUser>,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let root = format!(r"Software\LimeKitTest-{}-panel-{name}", std::process::id());
            let catalog = Catalog::parse(&format!(
                r#"
                [[tweak]]
                id = "test.sample"
                title.ru = "Пример"
                title.en = "Sample"
                description.ru = "Описание"
                description.en = "Description"
                provenance = "observed"

                [[tweak.change]]
                path = '{root}'
                name = "V"
                on = {{ type = "dword", data = 1 }}
                off = {{ type = "dword", data = 0 }}
                "#
            ))
            .unwrap();
            let dir = tempfile::tempdir().unwrap();
            let engine = Engine::open(CurrentUser, &dir.path().join("journal.jsonl")).unwrap();
            Sandbox {
                root,
                catalog,
                _dir: dir,
                engine,
            }
        }

        fn switch(&mut self, on: bool) -> SwitchOutcome {
            let tweak = &self.catalog.tweaks[0];
            switch(&mut self.engine, tweak, on).unwrap()
        }

        fn value(&self) -> Option<RawValue> {
            registry::read_value(&self.root, "V").unwrap()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.root);
        }
    }

    #[test]
    fn off_after_on_gives_back_a_value_set_by_hand() {
        let mut sandbox = Sandbox::new("by-hand");
        let by_hand = Value::Dword(7).to_raw();
        registry::write_value(&sandbox.root, "V", &by_hand).unwrap();
        sandbox.switch(true);
        sandbox.switch(false);

        assert_eq!(sandbox.value(), Some(by_hand));
    }

    #[test]
    fn off_with_nothing_to_undo_writes_the_default() {
        let mut sandbox = Sandbox::new("default");
        registry::write_value(&sandbox.root, "V", &Value::Dword(1).to_raw()).unwrap();
        sandbox.switch(false);

        assert_eq!(sandbox.value(), Some(Value::Dword(0).to_raw()));
    }

    #[test]
    fn on_twice_changes_nothing_the_second_time() {
        let mut sandbox = Sandbox::new("twice");
        sandbox.switch(true);

        assert!(matches!(sandbox.switch(true), SwitchOutcome::AlreadySet));
    }

    #[test]
    fn off_leaves_alone_a_value_changed_after_limekit() {
        let mut sandbox = Sandbox::new("changed");
        sandbox.switch(true);
        registry::write_value(&sandbox.root, "V", &Value::Dword(5).to_raw()).unwrap();

        assert!(matches!(
            sandbox.switch(false),
            SwitchOutcome::ChangedSince { .. }
        ));
    }

    #[test]
    fn windows_names_read_like_the_product() {
        let version = WindowsVersion {
            build: 26100,
            ubr: 4061,
            edition_id: "Professional".into(),
            display_version: "24H2".into(),
        };
        assert_eq!(
            describe_windows(&version),
            "Windows 11 24H2, 26100.4061, Pro"
        );
    }
}
