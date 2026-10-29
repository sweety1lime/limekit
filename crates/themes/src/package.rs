//! The theme package: a zip with one descriptor, `theme.json`, and the cursor,
//! sound and picture files it names. Nothing else goes in: a theme is data,
//! never code.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cursors::Role;
use crate::sounds::NO_SOUNDS;

/// The extension of a theme package, without the dot.
pub const EXTENSION: &str = "limetheme";

/// The name of the descriptor inside a package.
pub const DESCRIPTOR: &str = "theme.json";

/// The format this version reads.
pub const FORMAT: u32 = 1;

/// Whether `path` looks like a theme package.
pub fn has_theme_extension(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
}

/// A descriptor bigger than this is not one.
pub(crate) const MAX_DESCRIPTOR_BYTES: u64 = 256 * 1024;
/// All the files of a theme together.
pub(crate) const MAX_THEME_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ID_LEN: usize = 64;
const MAX_TEXT_LEN: usize = 200;
const MAX_NAME_LEN: usize = 128;
const MAX_SOUND_EVENTS: usize = 400;
const MAX_DEPTH: usize = 3;

#[derive(Debug, thiserror::Error)]
pub enum ThemeError {
    #[error(transparent)]
    Package(#[from] wpk::WpkError),
    #[error("{DESCRIPTOR} is missing from the package")]
    MissingDescriptor,
    /// A wallpaper package, which has `manifest.json` and no descriptor.
    #[error("this is a wallpaper package, not a theme")]
    WallpaperPackage,
    #[error("{DESCRIPTOR}: {0}")]
    Descriptor(String),
    #[error("the package is format {0}, which this LimeKit does not read")]
    NewerFormat(u64),
    #[error("{name}: {why}")]
    BadFile { name: String, why: String },
    #[error("the package is bigger than a theme may be")]
    TooBig,
    #[error("a theme with the id {0:?} is already in the library")]
    AlreadyInLibrary(String),
    #[error("no theme {0:?} in the library")]
    NotInLibrary(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// What a file in a package must be, by its extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKind {
    Cursor,
    AnimatedCursor,
    Sound,
    Picture,
}

impl FileKind {
    pub(crate) fn of(name: &str) -> Option<FileKind> {
        let extension = name.rsplit_once('.')?.1.to_ascii_lowercase();
        match extension.as_str() {
            "cur" => Some(FileKind::Cursor),
            "ani" => Some(FileKind::AnimatedCursor),
            "wav" => Some(FileKind::Sound),
            "png" | "jpg" | "jpeg" => Some(FileKind::Picture),
            _ => None,
        }
    }

    /// The most a file of this kind may hold.
    pub(crate) fn cap(self) -> u64 {
        match self {
            FileKind::Cursor => 1024 * 1024,
            FileKind::AnimatedCursor => 4 * 1024 * 1024,
            FileKind::Sound => 8 * 1024 * 1024,
            FileKind::Picture => 2 * 1024 * 1024,
        }
    }

    pub(crate) fn describe(self) -> &'static str {
        match self {
            FileKind::Cursor => "cursor",
            FileKind::AnimatedCursor => "animated cursor",
            FileKind::Sound => "sound",
            FileKind::Picture => "picture",
        }
    }

    /// Whether the start of a file has the header of this kind. Only the form
    /// is checked, not safety: Windows parses these files itself.
    pub(crate) fn matches(self, head: &[u8]) -> bool {
        let riff =
            |kind: &[u8; 4]| head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == kind;
        match self {
            // An icon directory of type 2, with at least one image.
            FileKind::Cursor => {
                head.len() >= 6 && head[..4] == [0, 0, 2, 0] && head[4..6] != [0, 0]
            }
            FileKind::AnimatedCursor => riff(b"ACON"),
            FileKind::Sound => riff(b"WAVE"),
            FileKind::Picture => {
                head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
                    || head.starts_with(&[0xFF, 0xD8, 0xFF])
            }
        }
    }
}

/// `theme.json`: who made the theme and what it puts on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub id: String,
    pub name: String,
    pub author: String,
    pub license: String,
    pub version: String,
    /// A picture in the package, shown on the theme's card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    /// Cursor file by role. An empty string is the built-in cursor.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cursors: BTreeMap<String, String>,
    /// Sound file by event, program by program. An empty string is silence.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sounds: BTreeMap<String, BTreeMap<String, String>>,
}

/// Lowercase letters, digits and `-_.`, starting with a letter or a digit: safe
/// in a journal name and as a folder name.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
        && !id.contains("..")
        && !id.ends_with('.')
}

/// The ids of the themes LimeKit builds itself.
fn is_reserved(id: &str) -> bool {
    id.starts_with("windows-") || id == NO_SOUNDS
}

fn bad(why: impl Into<String>) -> ThemeError {
    ThemeError::Descriptor(why.into())
}

impl Manifest {
    pub fn validate(&self) -> Result<(), ThemeError> {
        if u64::from(self.format) != u64::from(FORMAT) {
            return Err(ThemeError::NewerFormat(u64::from(self.format)));
        }
        if !is_valid_id(&self.id) {
            return Err(bad(format!(
                "id {:?} must be lowercase letters, digits and - _ . only",
                self.id
            )));
        }
        if is_reserved(&self.id) {
            return Err(bad(format!(
                "id {:?} is reserved for built-in themes",
                self.id
            )));
        }
        for (field, value) in [
            ("name", &self.name),
            ("author", &self.author),
            ("license", &self.license),
            ("version", &self.version),
        ] {
            if value.trim().is_empty() || value.chars().count() > MAX_TEXT_LEN {
                return Err(bad(format!(
                    "{field} must be 1 to {MAX_TEXT_LEN} characters"
                )));
            }
        }
        if self.cursors.is_empty() && self.sounds.is_empty() {
            return Err(bad("the theme has neither cursors nor sounds"));
        }
        if let Some(preview) = &self.preview {
            member_name(preview, &[FileKind::Picture])?;
        }
        for (role, file) in &self.cursors {
            if Role::from_name(role).is_none_or(|known| known.name() != role) {
                return Err(bad(format!("{role:?} is not a cursor role")));
            }
            if !file.is_empty() {
                member_name(file, &[FileKind::Cursor, FileKind::AnimatedCursor])?;
            }
        }
        let mut events = 0;
        for (app, list) in &self.sounds {
            registry_name(app)?;
            for (event, file) in list {
                registry_name(event)?;
                if !file.is_empty() {
                    member_name(file, &[FileKind::Sound])?;
                }
                events += 1;
            }
        }
        if events > MAX_SOUND_EVENTS {
            return Err(bad(format!("more than {MAX_SOUND_EVENTS} sound events")));
        }
        Ok(())
    }

    /// Every file the descriptor names, once, with what it must be.
    pub(crate) fn files(&self) -> Vec<(String, FileKind)> {
        let mut names: Vec<&String> = self
            .preview
            .iter()
            .chain(self.cursors.values())
            .chain(self.sounds.values().flat_map(BTreeMap::values))
            .filter(|name| !name.is_empty())
            .collect();
        names.sort();
        names.dedup();
        names
            .into_iter()
            .filter_map(|name| FileKind::of(name).map(|kind| (name.clone(), kind)))
            .collect()
    }
}

/// A name that becomes a registry key must not be able to climb or nest.
fn registry_name(name: &str) -> Result<(), ThemeError> {
    let wrong = name.is_empty()
        || name.chars().count() > MAX_NAME_LEN
        || name
            .chars()
            .any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c));
    if wrong {
        return Err(bad(format!("{name:?} is not a program or event name")));
    }
    Ok(())
}

/// A file name inside the package: safe, shallow, of one of `kinds`, and not a
/// name Windows keeps for devices.
pub(crate) fn member_name(name: &str, kinds: &[FileKind]) -> Result<(), ThemeError> {
    wpk::ensure_safe_name(name).map_err(|_| bad(format!("{name:?} is not a safe file name")))?;
    let segments: Vec<&str> = name.split('/').collect();
    if segments.len() > MAX_DEPTH {
        return Err(bad(format!("{name:?} is nested too deep")));
    }
    for segment in &segments {
        let stem = segment
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let device = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.ends_with(|c: char| c.is_ascii_digit() && c != '0'));
        let wrong = device
            || segment.ends_with(['.', ' '])
            || segment
                .chars()
                .any(|c| c.is_control() || r#"<>"|?*"#.contains(c));
        if wrong {
            return Err(bad(format!("{name:?} is not a usable file name")));
        }
    }
    match FileKind::of(name) {
        Some(kind) if kinds.contains(&kind) => Ok(()),
        _ => Err(bad(format!(
            "{name:?} is not the kind of file expected here"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            format: FORMAT,
            id: "midnight".into(),
            name: "Midnight".into(),
            author: "A. Author".into(),
            license: "CC-BY-4.0".into(),
            version: "1.0".into(),
            preview: Some("preview.png".into()),
            cursors: BTreeMap::from([
                ("Arrow".to_owned(), "cursors/arrow.cur".to_owned()),
                ("IBeam".to_owned(), String::new()),
            ]),
            sounds: BTreeMap::from([(
                ".Default".to_owned(),
                BTreeMap::from([("SystemAsterisk".to_owned(), "sounds/a.wav".to_owned())]),
            )]),
        }
    }

    fn rejected(change: impl FnOnce(&mut Manifest)) -> String {
        let mut manifest = manifest();
        change(&mut manifest);
        manifest
            .validate()
            .expect_err("must be refused")
            .to_string()
    }

    #[test]
    fn a_good_descriptor_is_accepted_and_names_its_files_once() {
        manifest().validate().unwrap();

        let files: Vec<String> = manifest()
            .files()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(files, ["cursors/arrow.cur", "preview.png", "sounds/a.wav"]);
    }

    #[test]
    fn ids_are_small_and_safe_and_the_built_in_ones_are_taken() {
        for good in ["midnight", "a", "my-theme_2.1", "0day"] {
            assert!(is_valid_id(good), "{good}");
        }
        for wrong in [
            "",
            "Midnight",
            "-x",
            ".x",
            "a..b",
            "a/b",
            "a b",
            "x.",
            &"a".repeat(65),
        ] {
            assert!(!is_valid_id(wrong), "{wrong:?}");
        }
        assert!(rejected(|m| m.id = "windows-black".into()).contains("reserved"));
        assert!(rejected(|m| m.id = "no-sounds".into()).contains("reserved"));
    }

    #[test]
    fn who_made_it_and_under_what_licence_is_required() {
        for field in ["name", "author", "license", "version"] {
            let message = rejected(|m| match field {
                "name" => m.name = " ".into(),
                "author" => m.author.clear(),
                "license" => m.license.clear(),
                _ => m.version.clear(),
            });
            assert!(message.contains(field), "{message}");
        }
    }

    #[test]
    fn only_the_cursor_roles_windows_knows_are_accepted() {
        assert!(
            rejected(|m| {
                m.cursors.insert("Link".into(), "l.cur".into());
            })
            .contains("not a cursor role")
        );
        assert!(
            rejected(|m| {
                m.cursors.insert("arrow".into(), "l.cur".into());
            })
            .contains("not a cursor role")
        );
    }

    #[test]
    fn a_file_must_be_the_kind_its_place_expects() {
        assert!(
            rejected(|m| {
                m.cursors.insert("Hand".into(), "hand.wav".into());
            })
            .contains("expected here")
        );
        assert!(rejected(|m| m.preview = Some("p.cur".into())).contains("expected here"));
        assert!(
            rejected(|m| {
                m.sounds
                    .get_mut(".Default")
                    .unwrap()
                    .insert("Close".into(), "c.cur".into());
            })
            .contains("expected here")
        );
    }

    #[test]
    fn file_names_cannot_climb_nest_deep_or_name_a_device() {
        for name in [
            "../x.cur",
            "/x.cur",
            "C:/x.cur",
            "a\\b.cur",
            "a/b/c/d.cur",
            "con.cur",
            "dir/NUL.wav",
            "lpt1.cur",
            "x.cur ",
            "a.cur.",
        ] {
            let ok = member_name(name, &[FileKind::Cursor, FileKind::Sound]);
            assert!(ok.is_err(), "{name}");
        }
        for name in [
            "arrow.cur",
            "cursors/arrow.cur",
            "a/b/c.cur",
            "com0.cur",
            "console.cur",
        ] {
            assert!(member_name(name, &[FileKind::Cursor]).is_ok(), "{name}");
        }
    }

    #[test]
    fn program_and_event_names_cannot_nest_or_climb() {
        for name in ["", "a\\b", "a/b", "a:b", "a*b", "tab\there"] {
            assert!(registry_name(name).is_err(), "{name:?}");
        }
        for name in [
            ".Default",
            "Explorer",
            "Notification.Default",
            "Распознавание",
        ] {
            assert!(registry_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn a_theme_has_to_change_something() {
        assert!(
            rejected(|m| {
                m.cursors.clear();
                m.sounds.clear();
            })
            .contains("neither")
        );
    }

    #[test]
    fn headers_tell_the_kinds_apart() {
        let cursor = [0u8, 0, 2, 0, 1, 0, 32, 32];
        let animated = *b"RIFF\0\0\0\0ACON";
        let sound = *b"RIFF\0\0\0\0WAVE";
        assert!(FileKind::Cursor.matches(&cursor));
        assert!(
            !FileKind::Cursor.matches(&[0, 0, 1, 0, 1, 0]),
            "an icon is not a cursor"
        );
        assert!(
            !FileKind::Cursor.matches(&[0, 0, 2, 0, 0, 0]),
            "a cursor holds an image"
        );
        assert!(FileKind::AnimatedCursor.matches(&animated));
        assert!(!FileKind::AnimatedCursor.matches(&sound));
        assert!(FileKind::Sound.matches(&sound));
        assert!(FileKind::Picture.matches(&[0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0]));
        assert!(FileKind::Picture.matches(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!FileKind::Picture.matches(b"GIF89a"));
    }
}
