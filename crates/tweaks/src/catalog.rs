//! The tweak catalog: what each tweak changes, described as data and built into
//! the binary. A tweak is a switch. "On" writes its values; "off" writes the
//! Windows defaults it lists; an undo through the journal puts back whatever
//! was there before, which need not be either.

use std::collections::BTreeSet;

use platform::parameters::Parameter;
use platform::registry::RawValue;
use platform::system::WindowsVersion;
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::compat::{Availability, Requirements};
use crate::engine::{self, Change, EngineError, Target};
use crate::parameters::Parameters;
use crate::registry::Registry;
use crate::value::Value;

const BUILTIN: &str = include_str!("../catalog.toml");

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    #[serde(rename = "tweak", default)]
    pub tweaks: Vec<Tweak>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tweak {
    /// `category.name`, stable: the journal refers to tweaks by it.
    pub id: String,
    pub title: Text,
    pub description: Text,
    pub provenance: Provenance,
    /// Where the values come from; required for a documented tweak.
    pub source: Option<String>,
    #[serde(default)]
    pub takes_effect: TakesEffect,
    /// The `WM_SETTINGCHANGE` area Windows announces when its own settings
    /// make this change, such as `ImmersiveColorSet`. LimeKit announces the
    /// same after switching the tweak, so open windows pick it up.
    pub notify: Option<String>,
    #[serde(default)]
    pub requires: Requirements,
    #[serde(rename = "change")]
    pub changes: Vec<TweakChange>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub ru: String,
    pub en: String,
}

/// How far LimeKit can vouch for a tweak's values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    /// Microsoft documents the value: a policy or a documented setting.
    Documented,
    /// Undocumented, but what Windows' own settings write.
    Observed,
    /// A trick Windows does not support and may stop honoring any update.
    Unsupported,
}

/// When a change becomes visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakesEffect {
    #[default]
    Immediately,
    /// File Explorer windows opened after the change show it; open ones keep
    /// the old view.
    NewWindows,
    ExplorerRestart,
    SignOut,
}

/// One value a tweak sets: a registry value under HKEY_CURRENT_USER, given by
/// `path` and `name`, or a system parameter.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TweakChange {
    /// Key path under HKEY_CURRENT_USER.
    pub path: Option<String>,
    /// Value name; empty for the key's default value.
    #[serde(default)]
    pub name: String,
    /// A system parameter instead of a registry value; its on and off are
    /// DWORD 1 and 0.
    pub parameter: Option<Parameter>,
    pub on: Setting,
    pub off: Setting,
    /// What Windows goes by while the value is not there at all. A fresh
    /// install often has no value until someone flips the switch in Settings.
    pub missing: Option<Value>,
}

impl TweakChange {
    pub fn target(&self) -> Target {
        match (&self.parameter, &self.path) {
            (Some(parameter), _) => Target::Parameter { name: *parameter },
            (None, path) => Target::Registry {
                path: path.clone().unwrap_or_default(),
                name: self.name.clone(),
            },
        }
    }

    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let target = self.target();
        match (&self.path, &self.parameter) {
            (Some(path), None) => {
                let upper = path.to_ascii_uppercase();
                if path.is_empty()
                    || upper.starts_with("HKEY")
                    || upper.starts_with("HKCU")
                    || path.starts_with('\\')
                    || path.ends_with('\\')
                    || path.contains(r"\\")
                {
                    problems.push(format!(
                        "{path:?} should be a key path under HKEY_CURRENT_USER, without the hive"
                    ));
                }
            }
            (None, Some(_)) => {
                let flag = |setting: &Setting| matches!(setting, Setting::Set(Value::Dword(0 | 1)));
                if !flag(&self.on) || !flag(&self.off) {
                    problems.push(format!("{target}: on and off should be DWORD 1 or 0"));
                }
                if !self.name.is_empty() || self.missing.is_some() {
                    problems.push(format!(
                        "{target}: a system parameter takes neither a name nor missing"
                    ));
                }
            }
            _ => problems.push("a change needs either a path or a parameter".into()),
        }
        if self.on == self.off {
            problems.push(format!("{target}: on and off are the same"));
        }
        if self.missing.is_some() && (self.on == Setting::Delete || self.off == Setting::Delete) {
            problems.push(format!(
                "{target}: missing means nothing when on or off deletes the value"
            ));
        }
        problems
    }
}

/// A value to write, or `"delete"` in the catalog to remove it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setting {
    Delete,
    Set(Value),
}

impl Setting {
    pub fn value(&self) -> Option<Value> {
        match self {
            Setting::Delete => None,
            Setting::Set(value) => Some(value.clone()),
        }
    }

    /// Whether `current` is what this setting writes.
    pub fn matches(&self, current: Option<&RawValue>) -> bool {
        match (self, current) {
            (Setting::Delete, None) => true,
            (Setting::Set(value), Some(current)) => value.to_raw().same_as(current),
            _ => false,
        }
    }
}

impl<'de> Deserialize<'de> for Setting {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Word(String),
            Value(Value),
        }
        match Form::deserialize(deserializer)? {
            Form::Word(word) if word == "delete" => Ok(Setting::Delete),
            Form::Word(word) => Err(de::Error::custom(format!(
                "expected \"delete\" or a value, found {word:?}"
            ))),
            Form::Value(value) => Ok(Setting::Set(value)),
        }
    }
}

/// Where a tweak stands on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TweakState {
    On,
    Off,
    /// Some values on and some not, or values that are neither: set by hand,
    /// by another program, or by a Windows update.
    Mixed,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("tweak catalog: {0}")]
    Parse(String),
    #[error("tweak catalog: {}", .0.join("; "))]
    Invalid(Vec<String>),
}

impl Catalog {
    /// The catalog built into LimeKit.
    pub fn builtin() -> Result<Catalog, CatalogError> {
        Catalog::parse(BUILTIN)
    }

    /// Parses `text` and checks it with [`Catalog::problems`].
    pub fn parse(text: &str) -> Result<Catalog, CatalogError> {
        let catalog: Catalog =
            toml::from_str(text).map_err(|error| CatalogError::Parse(error.to_string()))?;
        let problems = catalog.problems();
        if problems.is_empty() {
            Ok(catalog)
        } else {
            Err(CatalogError::Invalid(problems))
        }
    }

    pub fn get(&self, id: &str) -> Option<&Tweak> {
        self.tweaks.iter().find(|tweak| tweak.id == id)
    }

    /// Everything wrong with the catalog, one line per problem.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut ids = BTreeSet::new();
        for tweak in &self.tweaks {
            if !ids.insert(tweak.id.as_str()) {
                problems.push(format!("{}: the id is used twice", tweak.id));
            }
            for problem in tweak.problems() {
                problems.push(format!("{}: {problem}", tweak.id));
            }
        }
        problems
    }
}

impl Tweak {
    /// The changes that turn the tweak on, or off.
    pub fn changes(&self, on: bool) -> Vec<Change> {
        self.changes
            .iter()
            .map(|change| Change {
                target: change.target(),
                value: if on { &change.on } else { &change.off }.value(),
            })
            .collect()
    }

    pub fn availability(&self, version: &WindowsVersion) -> Availability {
        self.requires.availability(version)
    }

    pub fn state(&self, system: &(impl Registry + Parameters)) -> Result<TweakState, EngineError> {
        let mut all_on = true;
        let mut all_off = true;
        for change in &self.changes {
            let current = engine::read(system, &change.target())?
                .or_else(|| change.missing.as_ref().map(Value::to_raw));
            all_on &= change.on.matches(current.as_ref());
            all_off &= change.off.matches(current.as_ref());
        }
        Ok(match (all_on, all_off) {
            (true, _) => TweakState::On,
            (false, true) => TweakState::Off,
            (false, false) => TweakState::Mixed,
        })
    }

    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let well_formed = self.id.split('.').count() == 2
            && self.id.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            });
        if !well_formed {
            problems.push("the id should look like category.name in lowercase".into());
        }
        for (what, text) in [("title", &self.title), ("description", &self.description)] {
            if text.ru.trim().is_empty() || text.en.trim().is_empty() {
                problems.push(format!("the {what} needs both Russian and English"));
            }
        }
        match &self.source {
            Some(source) if !source.starts_with("https://") => {
                problems.push("the source should be an https link".into());
            }
            None if self.provenance == Provenance::Documented => {
                problems.push("a documented tweak needs its source".into());
            }
            _ => {}
        }
        if self.changes.is_empty() {
            problems.push("there is nothing to change".into());
        }
        for change in &self.changes {
            problems.extend(change.problems());
        }
        problems.extend(self.requires_problems());
        problems
    }

    fn requires_problems(&self) -> Vec<String> {
        let requires = &self.requires;
        let mut problems = Vec::new();
        if let (Some(min), Some(below)) = (requires.min_build, requires.below_build)
            && min >= below
        {
            problems.push(format!("min_build {min} is not below below_build {below}"));
        }
        if requires.editions.as_ref().is_some_and(Vec::is_empty) {
            problems.push("an empty edition list rules out every edition".into());
        }
        if requires.native_since.is_some() && requires.settings_uri.is_none() {
            problems.push("native_since needs the settings_uri it points to".into());
        }
        if requires
            .settings_uri
            .as_ref()
            .is_some_and(|uri| !uri.starts_with("ms-settings:"))
        {
            problems.push("settings_uri should be an ms-settings: link".into());
        }
        for &build in &requires.verified {
            let outside = requires.min_build.is_some_and(|min| build < min.number)
                || requires
                    .below_build
                    .is_some_and(|below| build >= below.number);
            if outside {
                problems.push(format!(
                    "verified on {build}, outside the builds it applies to"
                ));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fake::FakeRegistry;

    const ONE: &str = r#"
        [[tweak]]
        id = "explorer.sample"
        title.ru = "Пример"
        title.en = "Sample"
        description.ru = "Описание"
        description.en = "Description"
        provenance = "observed"

        [[tweak.change]]
        path = 'Software\Test'
        name = "Shown"
        on = { type = "dword", data = 1 }
        off = "delete"
    "#;

    const PARAMETER: &str = r#"
        [[tweak]]
        id = "appearance.sample"
        title.ru = "Пример"
        title.en = "Sample"
        description.ru = "Описание"
        description.en = "Description"
        provenance = "documented"
        source = "https://learn.microsoft.com/"

        [[tweak.change]]
        parameter = "menu_animation"
        on = { type = "dword", data = 0 }
        off = { type = "dword", data = 1 }
    "#;

    fn sample() -> Catalog {
        Catalog::parse(ONE).unwrap()
    }

    #[test]
    fn reads_a_system_parameter() {
        let catalog = Catalog::parse(PARAMETER).unwrap();
        assert_eq!(
            catalog.tweaks[0].changes(true),
            vec![Change {
                target: Target::Parameter {
                    name: Parameter::MenuAnimation,
                },
                value: Some(Value::Dword(0)),
            }]
        );
    }

    #[test]
    fn the_builtin_catalog_is_valid() {
        if let Err(error) = Catalog::builtin() {
            panic!("{error}");
        }
    }

    // File Explorer keeps these four settings in memory as well, and its Folder
    // Options dialog writes that copy back when it closes, which undoes a
    // change made only in the registry. Announcing ShellState makes Explorer
    // read the registry again.
    #[test]
    fn the_explorer_settings_kept_in_memory_announce_the_shell_state() {
        let catalog = Catalog::builtin().unwrap();
        for id in [
            "explorer.file_extensions",
            "explorer.hidden_files",
            "explorer.checkboxes",
            "explorer.icons_only",
        ] {
            let tweak = catalog.get(id).unwrap();
            assert_eq!(tweak.notify.as_deref(), Some("ShellState"), "{id}");
        }
    }

    #[test]
    fn reads_a_tweak() {
        let catalog = sample();
        let change = &catalog.get("explorer.sample").unwrap().changes[0];
        assert_eq!(
            (&change.on, &change.off),
            (&Setting::Set(Value::Dword(1)), &Setting::Delete)
        );
    }

    #[test]
    fn turns_a_tweak_into_engine_changes() {
        let tweak = &sample().tweaks[0];
        assert_eq!(
            tweak.changes(false),
            vec![Change {
                target: Target::Registry {
                    path: r"Software\Test".into(),
                    name: "Shown".into(),
                },
                value: None,
            }]
        );
    }

    #[test]
    fn refuses_an_unknown_field() {
        let text = ONE.replace("provenance", "provenence");
        assert!(matches!(Catalog::parse(&text), Err(CatalogError::Parse(_))));
    }

    #[test]
    fn refuses_a_word_other_than_delete() {
        let text = ONE.replace(r#"off = "delete""#, r#"off = "remove""#);
        assert!(matches!(Catalog::parse(&text), Err(CatalogError::Parse(_))));
    }

    mod problems {
        use super::*;

        fn problems_of(text: &str) -> Vec<String> {
            match Catalog::parse(text) {
                Err(CatalogError::Invalid(problems)) => problems,
                other => panic!("expected problems, got {other:?}"),
            }
        }

        #[test]
        fn a_path_with_the_hive() {
            let text = ONE.replace(r"path = 'Software\Test'", r"path = 'HKCU\Software\Test'");
            assert_eq!(problems_of(&text).len(), 1);
        }

        #[test]
        fn a_documented_tweak_without_a_source() {
            let text = ONE.replace(r#""observed""#, r#""documented""#);
            assert_eq!(
                problems_of(&text),
                ["explorer.sample: a documented tweak needs its source"]
            );
        }

        #[test]
        fn a_missing_translation() {
            let text = ONE.replace(r#"title.en = "Sample""#, r#"title.en = """#);
            assert_eq!(
                problems_of(&text),
                ["explorer.sample: the title needs both Russian and English"]
            );
        }

        #[test]
        fn on_and_off_alike() {
            let text = ONE.replace(r#"off = "delete""#, r#"off = { type = "dword", data = 1 }"#);
            assert_eq!(problems_of(&text).len(), 1);
        }

        #[test]
        fn a_duplicate_id() {
            let text = format!("{ONE}\n{ONE}");
            assert_eq!(
                problems_of(&text),
                ["explorer.sample: the id is used twice"]
            );
        }

        #[test]
        fn a_change_with_both_a_path_and_a_parameter() {
            let text = ONE.replace(
                r#"name = "Shown""#,
                r#"name = "Shown"
                parameter = "menu_animation""#,
            );
            assert!(!problems_of(&text).is_empty());
        }

        #[test]
        fn a_parameter_set_to_something_but_on_or_off() {
            let text = PARAMETER.replace("data = 0", "data = 2");
            assert_eq!(
                problems_of(&text),
                ["appearance.sample: SPI_SETMENUANIMATION: on and off should be DWORD 1 or 0"]
            );
        }

        #[test]
        fn missing_next_to_delete() {
            let text = ONE.replace(
                r#"off = "delete""#,
                r#"off = "delete"
                missing = { type = "dword", data = 0 }"#,
            );
            assert_eq!(problems_of(&text).len(), 1);
        }

        #[test]
        fn an_upside_down_build_range() {
            let text = format!("{ONE}\n[tweak.requires]\nmin_build = 26100\nbelow_build = 22000\n");
            assert_eq!(
                problems_of(&text),
                ["explorer.sample: min_build 26100 is not below below_build 22000"]
            );
        }
    }

    mod state {
        use super::*;

        #[test]
        fn is_on_when_every_value_is_the_on_one() {
            let registry =
                FakeRegistry::default().with(r"Software\Test", "Shown", Value::Dword(1).to_raw());
            assert_eq!(sample().tweaks[0].state(&registry).unwrap(), TweakState::On);
        }

        #[test]
        fn is_off_when_the_value_is_absent_as_off_asks() {
            assert_eq!(
                sample().tweaks[0].state(&FakeRegistry::default()).unwrap(),
                TweakState::Off
            );
        }

        #[test]
        fn takes_a_missing_value_as_what_windows_goes_by() {
            let text = ONE.replace(
                r#"off = "delete""#,
                r#"off = { type = "dword", data = 0 }
                missing = { type = "dword", data = 0 }"#,
            );
            let catalog = Catalog::parse(&text).unwrap();
            assert_eq!(
                catalog.tweaks[0].state(&FakeRegistry::default()).unwrap(),
                TweakState::Off
            );
        }

        #[test]
        fn reads_a_system_parameter_that_is_on_as_off() {
            let catalog = Catalog::parse(PARAMETER).unwrap();
            assert_eq!(
                catalog.tweaks[0].state(&FakeRegistry::default()).unwrap(),
                TweakState::Off
            );
        }

        #[test]
        fn is_mixed_for_a_value_that_is_neither() {
            let registry =
                FakeRegistry::default().with(r"Software\Test", "Shown", Value::Dword(7).to_raw());
            assert_eq!(
                sample().tweaks[0].state(&registry).unwrap(),
                TweakState::Mixed
            );
        }
    }
}
