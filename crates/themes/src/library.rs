//! The library of theme packages: a folder per theme under one root, each
//! holding `theme.json` and the files it names. Nothing else lands on disk:
//! a package is read, checked and written file by file, never unpacked whole.

use std::path::{Path, PathBuf};

use platform::sounds::SoundEvent;

use crate::cursors::{CursorSet, Role};
use crate::pack::Source;
use crate::package::{
    DESCRIPTOR, FORMAT, FileKind, MAX_DESCRIPTOR_BYTES, MAX_THEME_BYTES, Manifest, ThemeError,
    is_valid_id,
};
use crate::sounds::{SoundChoice, SoundSet};

/// Where a package is written before it is moved into place.
const STAGING: &str = ".staging";

/// Gives the bytes of one file of what is being imported, named as the theme
/// names it, at most `limit` of them.
pub(crate) type Reader<'a> = dyn Fn(&str, u64) -> Result<Vec<u8>, ThemeError> + 'a;

pub struct Library {
    pub(crate) root: PathBuf,
}

/// A theme that is in the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryTheme {
    pub manifest: Manifest,
    pub dir: PathBuf,
}

impl Library {
    pub fn at(root: PathBuf) -> Self {
        Self { root }
    }

    /// Whether anything has ever been imported: the folder is made by the first
    /// import.
    pub fn exists(&self) -> bool {
        self.root.is_dir()
    }

    /// The themes in the library, by name. A folder that is not a whole,
    /// valid theme is left out, so a half-deleted one does not break the list.
    pub fn list(&self) -> Vec<LibraryTheme> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut themes: Vec<LibraryTheme> = entries
            .filter_map(Result::ok)
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .filter_map(|entry| load(&entry.path()))
            .collect();
        themes.sort_by_key(|theme| theme.manifest.name.to_lowercase());
        themes
    }

    pub fn get(&self, id: &str) -> Option<LibraryTheme> {
        if !is_valid_id(id) {
            return None;
        }
        load(&self.root.join(id))
    }

    /// Reads, checks and adds the package at `package`. A package with anything
    /// in it but cursors, sounds, a picture and the descriptor is refused whole,
    /// and so is one whose files are not what their names say.
    pub fn import(&self, package: &Path) -> Result<LibraryTheme, ThemeError> {
        let members = wpk::list_members(package)?;
        let has = |name: &str| members.iter().any(|member| member.name == name);
        if !has(DESCRIPTOR) {
            return Err(if has(wpk::MANIFEST_NAME) {
                ThemeError::WallpaperPackage
            } else {
                ThemeError::MissingDescriptor
            });
        }
        if let Some(stranger) = members
            .iter()
            .find(|member| member.name != DESCRIPTOR && FileKind::of(&member.name).is_none())
        {
            return Err(ThemeError::BadFile {
                name: stranger.name.clone(),
                why: "a theme holds cursors, sounds and a picture, nothing else".into(),
            });
        }
        let descriptor = wpk::read_member(package, DESCRIPTOR, MAX_DESCRIPTOR_BYTES)?;
        let manifest = parse(&descriptor)?;
        if let Some((missing, _)) = manifest.files().iter().find(|(name, _)| !has(name)) {
            return Err(ThemeError::BadFile {
                name: missing.clone(),
                why: "named in the descriptor, but not in the package".into(),
            });
        }
        self.install(manifest, &descriptor, &|name, limit| {
            Ok(wpk::read_member(package, name, limit)?)
        })
    }

    /// Whether no theme has `id` yet.
    pub(crate) fn is_free(&self, id: &str) -> bool {
        !self.root.join(id).exists()
    }

    /// Puts the theme `manifest` describes into the library, with the files it
    /// names as `read` gives them. Each is checked against its kind and the caps
    /// before it is written, to a staging folder that is moved into place only
    /// when every file has passed, so a refusal leaves nothing behind.
    pub(crate) fn install(
        &self,
        manifest: Manifest,
        descriptor: &[u8],
        read: &Reader<'_>,
    ) -> Result<LibraryTheme, ThemeError> {
        let files = manifest.files();
        let target = self.root.join(&manifest.id);
        if target.exists() {
            return Err(ThemeError::AlreadyInLibrary(manifest.id));
        }

        let staging =
            self.root
                .join(STAGING)
                .join(format!("{}-{}", manifest.id, std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging)?;
        let staged = stage(read, &staging, descriptor, &files);
        let placed = staged.and_then(|()| std::fs::rename(&staging, &target).map_err(Into::into));
        // The staging folder is gone after a rename; after a failure it holds
        // the debris. Its parent goes too when nothing else is being staged.
        let _ = std::fs::remove_dir_all(&staging);
        let _ = std::fs::remove_dir(self.root.join(STAGING));
        placed?;
        Ok(LibraryTheme {
            manifest,
            dir: target,
        })
    }

    pub fn remove(&self, id: &str) -> Result<(), ThemeError> {
        let dir = self.root.join(id);
        if !is_valid_id(id) || !dir.is_dir() {
            return Err(ThemeError::NotInLibrary(id.to_owned()));
        }
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }

    /// Saves the theme `id` as a package at `target`, replacing a file there
    /// only when the package is whole. `credit` is who made the theme and under
    /// what license, as the person saving it says; it is kept in the library
    /// too, so the card and the next save agree. Every file is checked again on
    /// the way out, so one that was changed in the library is not passed on.
    pub fn export(
        &self,
        id: &str,
        target: &Path,
        credit: Option<(&str, &str)>,
    ) -> Result<(), ThemeError> {
        let theme = self
            .get(id)
            .ok_or_else(|| ThemeError::NotInLibrary(id.to_owned()))?;
        let mut manifest = theme.manifest;
        if let Some((author, license)) = credit {
            manifest.author = author.trim().to_owned();
            manifest.license = license.trim().to_owned();
            manifest.validate()?;
        }
        let descriptor = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| ThemeError::Descriptor(error.to_string()))?;

        let source = Source::Folder(theme.dir.clone());
        let mut total = descriptor.len() as u64;
        let mut members = vec![(DESCRIPTOR.to_owned(), descriptor.clone())];
        for (name, kind) in manifest.files() {
            let bytes = read_checked(&|name, limit| source.read(name, limit), &name, kind)?;
            total += bytes.len() as u64;
            if total > MAX_THEME_BYTES {
                return Err(ThemeError::TooBig);
            }
            members.push((name, bytes));
        }
        let members: Vec<(&str, &[u8])> = members
            .iter()
            .map(|(name, bytes)| (name.as_str(), bytes.as_slice()))
            .collect();
        wpk::write_archive(target, &members)?;

        if credit.is_some() {
            replace_file(&theme.dir.join(DESCRIPTOR), &descriptor)?;
        }
        Ok(())
    }
}

/// One file of a theme as `read` gives it, within the cap for its kind and with
/// the header its kind has.
fn read_checked(read: &Reader<'_>, name: &str, kind: FileKind) -> Result<Vec<u8>, ThemeError> {
    let bytes = read(name, kind.cap()).map_err(|error| match error {
        ThemeError::Package(wpk::WpkError::TooLarge { limit, .. }) => ThemeError::BadFile {
            name: name.to_owned(),
            why: format!("a {} may be {limit} bytes at most", kind.describe()),
        },
        other => other,
    })?;
    if !kind.matches(&bytes) {
        return Err(ThemeError::BadFile {
            name: name.to_owned(),
            why: format!("is not a {} file", kind.describe()),
        });
    }
    Ok(bytes)
}

/// Writes `bytes` to `path` through a temporary file beside it, so that what
/// was there is replaced whole or not at all.
fn replace_file(path: &Path, bytes: &[u8]) -> Result<(), ThemeError> {
    let mut partial = path.as_os_str().to_owned();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    std::fs::write(&partial, bytes)?;
    std::fs::rename(&partial, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&partial);
    })?;
    Ok(())
}

/// Writes the descriptor and each file the descriptor names into `staging`,
/// after checking the file against its kind and the sizes against the caps.
fn stage(
    read: &Reader<'_>,
    staging: &Path,
    descriptor: &[u8],
    files: &[(String, FileKind)],
) -> Result<(), ThemeError> {
    let mut total = descriptor.len() as u64;
    for (name, kind) in files {
        let bytes = read_checked(read, name, *kind)?;
        total += bytes.len() as u64;
        if total > MAX_THEME_BYTES {
            return Err(ThemeError::TooBig);
        }
        let path = file_path(staging, name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, bytes)?;
    }
    std::fs::write(staging.join(DESCRIPTOR), descriptor)?;
    Ok(())
}

/// The descriptor as a checked manifest. The format number is read first, so
/// a newer format says so rather than failing on a field this version has not
/// heard of.
fn parse(descriptor: &[u8]) -> Result<Manifest, ThemeError> {
    let value: serde_json::Value = serde_json::from_slice(descriptor)
        .map_err(|error| ThemeError::Descriptor(error.to_string()))?;
    if let Some(format) = value
        .get("format")
        .and_then(serde_json::Value::as_u64)
        .filter(|format| *format != u64::from(FORMAT))
    {
        return Err(ThemeError::NewerFormat(format));
    }
    let manifest: Manifest =
        serde_json::from_value(value).map_err(|error| ThemeError::Descriptor(error.to_string()))?;
    manifest.validate()?;
    Ok(manifest)
}

/// A theme folder that is whole: a valid descriptor, named like the folder,
/// and every file the descriptor names.
fn load(dir: &Path) -> Option<LibraryTheme> {
    let path = dir.join(DESCRIPTOR);
    if std::fs::metadata(&path).ok()?.len() > MAX_DESCRIPTOR_BYTES {
        return None;
    }
    let manifest = parse(&std::fs::read(path).ok()?).ok()?;
    if dir.file_name()?.to_str()? != manifest.id {
        return None;
    }
    let whole = manifest
        .files()
        .iter()
        .all(|(name, _)| file_path(dir, name).is_file());
    whole.then(|| LibraryTheme {
        manifest,
        dir: dir.to_owned(),
    })
}

/// `dir` with the package's forward-slash name appended, in Windows form.
fn file_path(dir: &Path, name: &str) -> PathBuf {
    let mut path = dir.to_owned();
    for segment in name.split('/') {
        path.push(segment);
    }
    path
}

impl LibraryTheme {
    pub fn id(&self) -> &str {
        &self.manifest.id
    }

    /// The cursors the theme sets, in the order Windows lists the roles, as
    /// files in the library. `None` when it sets none.
    pub fn cursor_set(&self) -> Option<CursorSet> {
        if self.manifest.cursors.is_empty() {
            return None;
        }
        let roles = Role::ALL
            .iter()
            .filter_map(|role| {
                let file = self.manifest.cursors.get(role.name())?;
                Some((*role, self.path_of(file)))
            })
            .collect();
        Some(CursorSet {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            roles,
        })
    }

    /// The sounds the theme sets for the events this Windows has, and the
    /// `program/event` names it could not use. `None` when it sets none.
    /// Events are matched without regard to case, as the registry does, and
    /// keep the spelling Windows has.
    pub fn sound_set(&self, events: &[SoundEvent]) -> Option<(SoundSet, Vec<String>)> {
        if self.manifest.sounds.is_empty() {
            return None;
        }
        let mut choices = Vec::new();
        let mut skipped = Vec::new();
        for (app, list) in &self.manifest.sounds {
            for (event, file) in list {
                let known = events.iter().find(|known| {
                    known.app.eq_ignore_ascii_case(app) && known.event.eq_ignore_ascii_case(event)
                });
                match known {
                    Some(known) => choices.push(SoundChoice {
                        app: known.app.clone(),
                        event: known.event.clone(),
                        label: known.label.clone(),
                        sound: self.path_of(file),
                    }),
                    None => skipped.push(format!("{app}/{event}")),
                }
            }
        }
        let set = SoundSet {
            id: self.manifest.id.clone(),
            events: choices,
        };
        Some((set, skipped))
    }

    /// The picture for the card, if the package has one.
    pub fn preview_path(&self) -> Option<PathBuf> {
        let name = self.manifest.preview.as_deref()?;
        Some(file_path(&self.dir, name))
    }

    /// A file of the theme as the registry will hold it; empty stays empty.
    fn path_of(&self, file: &str) -> String {
        if file.is_empty() {
            return String::new();
        }
        file_path(&self.dir, file).to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const CUR: [u8; 8] = [0, 0, 2, 0, 1, 0, 32, 32];
    const WAV: &[u8] = b"RIFF\0\0\0\0WAVEfmt ";
    const PNG: [u8; 9] = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0];

    const DESCRIPTOR_JSON: &str = r#"{
        "format": 1, "id": "midnight", "name": "Midnight", "author": "A. Author",
        "license": "CC-BY-4.0", "version": "1.0", "preview": "preview.png",
        "cursors": { "Arrow": "cursors/arrow.cur", "IBeam": "" },
        "sounds": {
            ".Default": { "SystemAsterisk": "sounds/a.wav", "Close": "" },
            "Nobody": { "Nothing": "sounds/a.wav" }
        }
    }"#;

    fn package(dir: &Path, name: &str, members: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let mut writer = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (member, bytes) in members {
            writer.start_file(*member, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn midnight(dir: &Path) -> PathBuf {
        package(
            dir,
            "midnight.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("cursors/arrow.cur", &CUR),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
            ],
        )
    }

    fn library(dir: &Path) -> Library {
        Library::at(dir.join("themes"))
    }

    fn event(app: &str, name: &str) -> SoundEvent {
        SoundEvent {
            app: app.into(),
            event: name.into(),
            label: format!("label of {name}"),
            sound: Some(r"C:\Windows\media\old.wav".into()),
        }
    }

    fn refusal(library: &Library, package: &Path) -> String {
        library
            .import(package)
            .expect_err("must be refused")
            .to_string()
    }

    #[test]
    fn a_package_is_imported_into_a_folder_of_its_own_and_listed() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        assert!(!library.exists(), "nothing is made before the first import");

        let theme = library.import(&midnight(temp.path())).unwrap();

        assert_eq!(theme.id(), "midnight");
        assert!(theme.dir.join("theme.json").is_file());
        assert!(theme.dir.join("cursors").join("arrow.cur").is_file());
        assert!(theme.dir.join("sounds").join("a.wav").is_file());
        assert!(theme.dir.join("preview.png").is_file());
        assert_eq!(library.list(), std::slice::from_ref(&theme));
        assert_eq!(library.get("midnight"), Some(theme));
        assert!(
            !library.root.join(STAGING).exists(),
            "the staging folder is gone"
        );
    }

    #[test]
    fn the_cursor_set_points_at_the_files_in_the_library() {
        let temp = tempfile::tempdir().unwrap();
        let theme = library(temp.path()).import(&midnight(temp.path())).unwrap();

        let set = theme.cursor_set().unwrap();

        assert_eq!(set.id, "midnight");
        assert_eq!(set.name, "Midnight");
        let arrow = theme.dir.join("cursors").join("arrow.cur");
        assert_eq!(
            set.path(Role::Arrow),
            Some(arrow.to_string_lossy().as_ref())
        );
        assert!(!set.path(Role::Arrow).unwrap().contains('/'));
        // The empty string is the built-in cursor, kept as a role of the set.
        assert!(set.roles.contains(&(Role::IBeam, String::new())));
        assert_eq!(set.roles.len(), 2);
    }

    #[test]
    fn the_sound_set_keeps_the_events_this_windows_has_and_names_the_rest() {
        let temp = tempfile::tempdir().unwrap();
        let theme = library(temp.path()).import(&midnight(temp.path())).unwrap();
        let events = [
            event(".Default", "SystemAsterisk"),
            event(".default", "close"),
        ];

        let (set, skipped) = theme.sound_set(&events).unwrap();

        assert_eq!(set.id, "midnight");
        let named: Vec<(&str, &str, bool)> = set
            .events
            .iter()
            .map(|choice| {
                (
                    choice.event.as_str(),
                    choice.label.as_str(),
                    choice.sound.is_empty(),
                )
            })
            .collect();
        assert_eq!(
            named,
            [
                ("close", "label of close", true),
                ("SystemAsterisk", "label of SystemAsterisk", false)
            ],
            "Windows' own spelling and silence kept"
        );
        assert_eq!(skipped, ["Nobody/Nothing"]);
    }

    #[test]
    fn a_wallpaper_package_is_told_apart_from_a_theme() {
        let temp = tempfile::tempdir().unwrap();
        let wallpaper = package(temp.path(), "w.limekit", &[("manifest.json", b"{}")]);

        let refusal = refusal(&library(temp.path()), &wallpaper);

        assert_eq!(refusal, "this is a wallpaper package, not a theme");
    }

    #[test]
    fn a_package_without_a_descriptor_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let empty = package(temp.path(), "e.limetheme", &[("cursors/arrow.cur", &CUR)]);

        assert!(refusal(&library(temp.path()), &empty).contains("theme.json is missing"));
    }

    #[test]
    fn a_newer_format_says_so_instead_of_failing_on_its_fields() {
        let temp = tempfile::tempdir().unwrap();
        let newer = package(
            temp.path(),
            "n.limetheme",
            &[("theme.json", br#"{"format": 2, "something": "new"}"#)],
        );

        assert!(refusal(&library(temp.path()), &newer).contains("format 2"));
    }

    #[test]
    fn a_misspelt_field_is_an_error_not_a_silent_gap() {
        let temp = tempfile::tempdir().unwrap();
        let json = DESCRIPTOR_JSON.replace("\"cursors\"", "\"cursor\"");
        let odd = package(
            temp.path(),
            "o.limetheme",
            &[
                ("theme.json", json.as_bytes()),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
            ],
        );

        assert!(refusal(&library(temp.path()), &odd).contains("unknown field"));
    }

    #[test]
    fn a_package_that_holds_anything_but_data_is_refused_whole() {
        let temp = tempfile::tempdir().unwrap();
        let sneaky = package(
            temp.path(),
            "s.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("cursors/arrow.cur", &CUR),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
                ("run.exe", b"MZ"),
            ],
        );
        let library = library(temp.path());

        assert!(refusal(&library, &sneaky).contains("run.exe"));
        assert!(library.list().is_empty());
    }

    #[test]
    fn a_file_the_descriptor_names_has_to_be_in_the_package() {
        let temp = tempfile::tempdir().unwrap();
        let short = package(
            temp.path(),
            "m.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
            ],
        );

        assert!(refusal(&library(temp.path()), &short).contains("cursors/arrow.cur"));
    }

    #[test]
    fn a_file_of_the_wrong_shape_refuses_the_package() {
        let temp = tempfile::tempdir().unwrap();
        let wrong = package(
            temp.path(),
            "w.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("cursors/arrow.cur", &PNG),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
            ],
        );

        assert!(refusal(&library(temp.path()), &wrong).contains("is not a cursor file"));
    }

    #[test]
    fn a_file_over_its_cap_refuses_the_package() {
        let temp = tempfile::tempdir().unwrap();
        let mut big = CUR.to_vec();
        big.resize(FileKind::Cursor.cap() as usize + 1, 0);
        let heavy = package(
            temp.path(),
            "h.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("cursors/arrow.cur", &big),
                ("sounds/a.wav", WAV),
                ("preview.png", &PNG),
            ],
        );

        assert!(refusal(&library(temp.path()), &heavy).contains("at most"));
    }

    #[test]
    fn a_name_that_climbs_refuses_the_package() {
        let temp = tempfile::tempdir().unwrap();
        let climbing = package(
            temp.path(),
            "c.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("../evil.cur", &CUR),
            ],
        );

        assert!(refusal(&library(temp.path()), &climbing).contains("unsafe"));
    }

    #[test]
    fn a_refused_package_leaves_nothing_behind() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        let wrong = package(
            temp.path(),
            "w.limetheme",
            &[
                ("theme.json", DESCRIPTOR_JSON.as_bytes()),
                ("cursors/arrow.cur", &CUR),
                ("sounds/a.wav", b"not a wave at all"),
                ("preview.png", &PNG),
            ],
        );

        assert!(library.import(&wrong).is_err());

        assert!(library.list().is_empty());
        assert!(!library.root.join("midnight").exists());
        assert!(!library.root.join(STAGING).exists());
    }

    #[test]
    fn the_same_theme_twice_is_already_in_the_library() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        library.import(&midnight(temp.path())).unwrap();

        let again = library.import(&midnight(temp.path())).unwrap_err();

        assert!(matches!(again, ThemeError::AlreadyInLibrary(id) if id == "midnight"));
        assert_eq!(library.list().len(), 1);
    }

    #[test]
    fn folders_that_are_not_whole_themes_are_not_listed() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        library.import(&midnight(temp.path())).unwrap();
        let root = &library.root;
        // Not JSON, named for another id, and missing a file it names.
        std::fs::create_dir_all(root.join("broken")).unwrap();
        std::fs::write(root.join("broken").join("theme.json"), "{ nope").unwrap();
        std::fs::create_dir_all(root.join("other")).unwrap();
        std::fs::write(root.join("other").join("theme.json"), DESCRIPTOR_JSON).unwrap();
        std::fs::create_dir_all(root.join("gone")).unwrap();
        std::fs::write(
            root.join("gone").join("theme.json"),
            DESCRIPTOR_JSON.replace("midnight", "gone"),
        )
        .unwrap();

        let ids: Vec<String> = library
            .list()
            .iter()
            .map(|theme| theme.id().to_owned())
            .collect();

        assert_eq!(ids, ["midnight"]);
    }

    #[test]
    fn a_theme_is_removed_by_its_id_and_a_bad_id_removes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        library.import(&midnight(temp.path())).unwrap();

        assert!(library.remove("../themes").is_err());
        assert!(library.remove("nobody").is_err());
        assert_eq!(library.list().len(), 1);

        library.remove("midnight").unwrap();

        assert!(library.list().is_empty());
    }

    #[test]
    fn a_theme_is_saved_as_a_package_that_imports_again_just_the_same() {
        let temp = tempfile::tempdir().unwrap();
        let here = library(&temp.path().join("here"));
        let there = library(&temp.path().join("there"));
        let theme = here.import(&midnight(temp.path())).unwrap();
        let saved = temp.path().join("out").join("saved.limetheme");

        here.export("midnight", &saved, None).unwrap();
        let again = there.import(&saved).unwrap();

        assert_eq!(again.manifest, theme.manifest);
        for file in ["cursors/arrow.cur", "sounds/a.wav", "preview.png"] {
            assert_eq!(
                std::fs::read(file_path(&again.dir, file)).unwrap(),
                std::fs::read(file_path(&theme.dir, file)).unwrap(),
                "{file}"
            );
        }
        assert!(
            !temp
                .path()
                .join("out")
                .join("saved.limetheme.part")
                .exists()
        );
    }

    #[test]
    fn the_credit_given_on_the_way_out_goes_into_the_package_and_stays_in_the_library() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        library.import(&midnight(temp.path())).unwrap();
        let saved = temp.path().join("saved.limetheme");

        library
            .export("midnight", &saved, Some(("  New Author ", "MIT")))
            .unwrap();

        let exported = self::library(&temp.path().join("other"))
            .import(&saved)
            .unwrap();
        assert_eq!(exported.manifest.author, "New Author");
        assert_eq!(exported.manifest.license, "MIT");
        let kept = library.get("midnight").unwrap();
        assert_eq!(
            (
                kept.manifest.author.as_str(),
                kept.manifest.license.as_str()
            ),
            ("New Author", "MIT")
        );
        assert_eq!(
            library.list().len(),
            1,
            "the descriptor is still a good one"
        );
    }

    #[test]
    fn an_empty_credit_is_refused_and_nothing_is_written() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        library.import(&midnight(temp.path())).unwrap();
        let saved = temp.path().join("saved.limetheme");

        let error = library
            .export("midnight", &saved, Some(("", "MIT")))
            .unwrap_err();

        assert!(error.to_string().contains("author"), "{error}");
        assert!(!saved.exists());
        assert_eq!(
            library.get("midnight").unwrap().manifest.author,
            "A. Author"
        );
    }

    #[test]
    fn a_theme_that_is_not_in_the_library_is_not_saved() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());

        let error = library
            .export("nobody", &temp.path().join("x.limetheme"), None)
            .unwrap_err();

        assert!(matches!(error, ThemeError::NotInLibrary(_)));
        assert!(!temp.path().join("x.limetheme").exists());
    }

    #[test]
    fn a_file_changed_in_the_library_is_not_passed_on_and_the_old_package_stays() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        let theme = library.import(&midnight(temp.path())).unwrap();
        std::fs::write(file_path(&theme.dir, "cursors/arrow.cur"), b"not a cursor").unwrap();
        let saved = temp.path().join("saved.limetheme");
        std::fs::write(&saved, b"the package saved before").unwrap();

        let error = library.export("midnight", &saved, None).unwrap_err();

        assert!(error.to_string().contains("arrow.cur"), "{error}");
        assert_eq!(std::fs::read(&saved).unwrap(), b"the package saved before");
        assert!(!temp.path().join("saved.limetheme.part").exists());
    }
}
