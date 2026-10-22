//! Which tweaks apply to this Windows, and how sure LimeKit is about it.
//!
//! A tweak never fails silently on a Windows it was not made for: it says why
//! it does not apply, or that nobody has checked it on this build yet.

use std::fmt;
use std::str::FromStr;

use platform::system::WindowsVersion;
use serde::{Deserialize, Deserializer, de};

/// A build with an optional update revision: `26100` or `26100.3915`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Build {
    pub number: u32,
    pub ubr: u32,
}

impl Build {
    pub fn of(version: &WindowsVersion) -> Build {
        Build {
            number: version.build,
            ubr: version.ubr,
        }
    }
}

impl FromStr for Build {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (number, ubr) = text.split_once('.').unwrap_or((text, "0"));
        let parse = |part: &str| {
            part.parse::<u32>()
                .map_err(|_| format!("not a build number: {text:?}"))
        };
        Ok(Build {
            number: parse(number)?,
            ubr: parse(ubr)?,
        })
    }
}

impl fmt::Display for Build {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ubr == 0 {
            write!(f, "{}", self.number)
        } else {
            write!(f, "{}.{}", self.number, self.ubr)
        }
    }
}

/// Accepts `26100` as well as `"26100.3915"` in the catalog.
impl<'de> Deserialize<'de> for Build {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Number(u32),
            Text(String),
        }
        match Form::deserialize(deserializer)? {
            Form::Number(number) => Ok(Build { number, ubr: 0 }),
            Form::Text(text) => text.parse().map_err(de::Error::custom),
        }
    }
}

/// Editions as their policies differ: many settings Microsoft documents work
/// only on Enterprise and Education.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edition {
    Home,
    Pro,
    Enterprise,
    Education,
    Server,
}

impl Edition {
    /// The family of an `EditionID`, `None` for one LimeKit does not know.
    pub fn of(edition_id: &str) -> Option<Edition> {
        // Pro Education is listed with Education in Microsoft's policy tables.
        let families = [
            ("ProfessionalEducation", Edition::Education),
            ("Professional", Edition::Pro),
            ("Core", Edition::Home),
            ("IoTEnterprise", Edition::Enterprise),
            ("Enterprise", Edition::Enterprise),
            ("Education", Edition::Education),
            ("Server", Edition::Server),
        ];
        families
            .iter()
            .find(|(prefix, _)| edition_id.starts_with(prefix))
            .map(|&(_, edition)| edition)
    }
}

/// Where and how a tweak applies, as the catalog states it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirements {
    pub min_build: Option<Build>,
    /// The first build the tweak no longer applies to.
    pub below_build: Option<Build>,
    /// `None` for every edition.
    pub editions: Option<Vec<Edition>>,
    /// Builds the tweak was checked on, by hand or by CI.
    #[serde(default)]
    pub verified: Vec<u32>,
    /// From this build on Windows has its own switch for the same thing.
    pub native_since: Option<Build>,
    /// The `ms-settings:` page with that switch.
    pub settings_uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Availability {
    /// Applies here and was checked on this build.
    Verified,
    /// Should apply here, but nobody has checked it on this build yet.
    Unverified,
    /// Windows has its own switch for this here; better use that.
    BuiltIn {
        settings_uri: Option<String>,
    },
    TooOld {
        min: Build,
    },
    TooNew {
        below: Build,
    },
    WrongEdition {
        editions: Vec<Edition>,
    },
}

impl Availability {
    /// Whether LimeKit may apply the tweak here.
    pub fn applies(&self) -> bool {
        matches!(self, Availability::Verified | Availability::Unverified)
    }
}

impl Requirements {
    pub fn availability(&self, version: &WindowsVersion) -> Availability {
        if let Some(editions) = &self.editions {
            let here = Edition::of(&version.edition_id);
            if !here.is_some_and(|edition| editions.contains(&edition)) {
                return Availability::WrongEdition {
                    editions: editions.clone(),
                };
            }
        }
        let build = Build::of(version);
        if let Some(min) = self.min_build
            && build < min
        {
            return Availability::TooOld { min };
        }
        if let Some(below) = self.below_build
            && build >= below
        {
            return Availability::TooNew { below };
        }
        if self.native_since.is_some_and(|since| build >= since) {
            return Availability::BuiltIn {
                settings_uri: self.settings_uri.clone(),
            };
        }
        if self.verified.contains(&version.build) {
            Availability::Verified
        } else {
            Availability::Unverified
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(build: u32, ubr: u32, edition_id: &str) -> WindowsVersion {
        WindowsVersion {
            build,
            ubr,
            edition_id: edition_id.into(),
            display_version: String::new(),
        }
    }

    fn build(text: &str) -> Build {
        text.parse().unwrap()
    }

    mod build_numbers {
        use super::*;

        #[test]
        fn read_a_plain_build() {
            assert_eq!(
                build("26100"),
                Build {
                    number: 26100,
                    ubr: 0
                }
            );
        }

        #[test]
        fn read_a_build_with_its_revision() {
            assert_eq!(
                build("26100.3915"),
                Build {
                    number: 26100,
                    ubr: 3915
                }
            );
        }

        #[test]
        fn compare_by_build_then_revision() {
            assert!(build("26100.3914") < build("26100.3915"));
        }

        #[test]
        fn refuse_text_that_is_not_a_build() {
            assert!("24H2".parse::<Build>().is_err());
        }
    }

    mod editions {
        use super::*;

        #[test]
        fn group_home_variants() {
            assert_eq!(Edition::of("CoreSingleLanguage"), Some(Edition::Home));
        }

        #[test]
        fn count_pro_education_as_education() {
            assert_eq!(
                Edition::of("ProfessionalEducation"),
                Some(Edition::Education)
            );
        }

        #[test]
        fn count_pro_workstation_as_pro() {
            assert_eq!(Edition::of("ProfessionalWorkstation"), Some(Edition::Pro));
        }

        #[test]
        fn know_nothing_of_unknown_ones() {
            assert_eq!(Edition::of("Cloud"), None);
        }
    }

    mod availability {
        use super::*;

        #[test]
        fn is_unverified_by_default() {
            let requirements = Requirements::default();
            assert_eq!(
                requirements.availability(&windows(19045, 6456, "Core")),
                Availability::Unverified
            );
        }

        #[test]
        fn is_verified_on_a_checked_build() {
            let requirements = Requirements {
                verified: vec![19045],
                ..Requirements::default()
            };
            assert_eq!(
                requirements.availability(&windows(19045, 6456, "Core")),
                Availability::Verified
            );
        }

        #[test]
        fn needs_the_minimum_revision_too() {
            let requirements = Requirements {
                min_build: Some(build("26100.3915")),
                ..Requirements::default()
            };
            assert_eq!(
                requirements.availability(&windows(26100, 3000, "Professional")),
                Availability::TooOld {
                    min: build("26100.3915")
                }
            );
        }

        #[test]
        fn stops_at_the_upper_build() {
            let requirements = Requirements {
                below_build: Some(build("26200")),
                ..Requirements::default()
            };
            assert!(matches!(
                requirements.availability(&windows(26200, 1, "Core")),
                Availability::TooNew { .. }
            ));
        }

        #[test]
        fn leaves_out_other_editions() {
            let requirements = Requirements {
                editions: Some(vec![Edition::Enterprise, Edition::Education]),
                ..Requirements::default()
            };
            assert!(matches!(
                requirements.availability(&windows(26100, 1, "Professional")),
                Availability::WrongEdition { .. }
            ));
        }

        #[test]
        fn points_to_the_native_switch_where_windows_has_one() {
            let requirements = Requirements {
                native_since: Some(build("26200")),
                settings_uri: Some("ms-settings:advanced".into()),
                ..Requirements::default()
            };
            assert_eq!(
                requirements.availability(&windows(26200, 1, "Core")),
                Availability::BuiltIn {
                    settings_uri: Some("ms-settings:advanced".into())
                }
            );
        }

        #[test]
        fn applies_only_when_verified_or_unverified() {
            assert!(
                !Availability::TooOld {
                    min: build("22000")
                }
                .applies()
            );
        }
    }
}
