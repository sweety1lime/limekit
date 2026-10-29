//! Cursor packs as people share them: a folder, a zip or an `Install.inf`, with
//! the `.cur` and `.ani` files beside it. Each scheme in a pack becomes a
//! theme of its own in the library, the same as one from a `.limetheme`, so the
//! checks, the caps and the way it is applied are the same too. The INF is read
//! for which file goes with which role, and nothing else is taken from it.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cursors::slug;
use crate::inf::{self, MAX_INF_BYTES, Scheme};
use crate::library::{Library, LibraryTheme};
use crate::package::{FORMAT, FileKind, Manifest, ThemeError, UNSTATED, is_reserved, member_name};

/// A pack with more schemes than this is not read.
const MAX_SCHEMES: usize = 40;
/// How many files a folder may hold, and how deep, before it is not a pack.
const MAX_FILES: usize = 4096;
const MAX_DEPTH: usize = 4;
/// The most of a theme's name that goes into its id.
const MAX_SLUG: usize = 48;
const MAX_NAME_CHARS: usize = 100;

/// What adding a pack brought.
#[derive(Debug)]
pub struct PackImport {
    /// The themes now in the library, one for each scheme that could be added.
    pub themes: Vec<LibraryTheme>,
    /// Files the schemes name that the pack does not have, once each.
    pub missing: Vec<String>,
    /// Schemes left out, each with what was wrong with it.
    pub skipped: Vec<String>,
}

/// Where the files of a pack are read from.
enum Source {
    Zip(PathBuf),
    Folder(PathBuf),
}

impl Source {
    /// Every file, named with `/` and relative to the root of the pack.
    fn files(&self) -> Result<Vec<String>, ThemeError> {
        match self {
            Source::Zip(package) => Ok(wpk::list_members(package)?
                .into_iter()
                .map(|member| member.name)
                .collect()),
            Source::Folder(root) => {
                let mut files = Vec::new();
                walk(root, root, 0, &mut files)?;
                Ok(files)
            }
        }
    }

    /// One file as bytes, at most `limit` of them.
    fn read(&self, name: &str, limit: u64) -> Result<Vec<u8>, ThemeError> {
        match self {
            Source::Zip(package) => Ok(wpk::read_member(package, name, limit)?),
            Source::Folder(root) => {
                let path = name
                    .split('/')
                    .fold(root.clone(), |path, part| path.join(part));
                if is_link(&path) {
                    return Err(ThemeError::BadFile {
                        name: name.to_owned(),
                        why: "is a link, not a file".into(),
                    });
                }
                let mut bytes = Vec::new();
                std::fs::File::open(&path)?
                    .take(limit + 1)
                    .read_to_end(&mut bytes)?;
                if bytes.len() as u64 > limit {
                    return Err(wpk::WpkError::TooLarge {
                        name: name.to_owned(),
                        limit,
                    }
                    .into());
                }
                Ok(bytes)
            }
        }
    }
}

/// Lists the files under `dir`. Links are not followed: a pack is what is in
/// the folder, not what a link in it points at.
fn walk(root: &Path, dir: &Path, depth: usize, files: &mut Vec<String>) -> Result<(), ThemeError> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if is_link(&path) {
            continue;
        }
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(root, &path, depth + 1, files)?;
            }
        } else if path.is_file() {
            if files.len() >= MAX_FILES {
                return Err(wpk::WpkError::TooLarge {
                    name: "<folder>".into(),
                    limit: MAX_FILES as u64,
                }
                .into());
            }
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let parts: Option<Vec<&str>> = relative
                .components()
                .map(|part| part.as_os_str().to_str())
                .collect();
            if let Some(parts) = parts {
                files.push(parts.join("/"));
            }
        }
    }
    Ok(())
}

/// A symbolic link or a junction, which a pack must not lead out through.
fn is_link(path: &Path) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return true;
    };
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & REPARSE_POINT != 0 {
            return true;
        }
    }
    false
}

/// The source a path stands for, and the one `.inf` to read when the path is
/// that file.
fn open(path: &Path) -> Result<(Source, Option<String>), ThemeError> {
    if path.is_dir() {
        return Ok((Source::Folder(path.to_owned()), None));
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("zip") => Ok((Source::Zip(path.to_owned()), None)),
        Some("inf") => {
            let folder = path
                .parent()
                .filter(|folder| !folder.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(ThemeError::NotAPack)?;
            Ok((Source::Folder(folder.to_owned()), Some(name.to_owned())))
        }
        _ => Err(ThemeError::NotAPack),
    }
}

impl Library {
    /// Adds the cursor pack at `path`: a folder, a zip, or an `.inf` with its
    /// files beside it. Every scheme the INFs register becomes a theme, with the
    /// cursor files they name; a scheme that cannot be added is left out and
    /// said so, and it is an error only when none can. Nothing in the pack is
    /// run, and only `.cur` and `.ani` files are copied, after the same checks as
    /// a `.limetheme`.
    pub fn import_pack(&self, path: &Path) -> Result<PackImport, ThemeError> {
        let (source, only) = open(path)?;
        let mut files = source.files()?;
        files.sort();
        let infs: Vec<&String> = files
            .iter()
            .filter(|name| is_inf(name) && only.as_ref().is_none_or(|only| only == *name))
            .collect();
        if infs.is_empty() {
            return Err(ThemeError::NoInf);
        }

        let mut schemes: Vec<(String, Scheme)> = Vec::new();
        for name in infs {
            let text = inf::decode(&source.read(name, MAX_INF_BYTES)?);
            for scheme in inf::schemes(&text) {
                schemes.push((dir_of(name).to_owned(), scheme));
            }
            if schemes.len() > MAX_SCHEMES {
                return Err(ThemeError::TooManySchemes(MAX_SCHEMES));
            }
        }
        if schemes.is_empty() {
            return Err(ThemeError::NoScheme);
        }

        let fallback = pack_name(path);
        let mut added = PackImport {
            themes: Vec::new(),
            missing: Vec::new(),
            skipped: Vec::new(),
        };
        let mut first_failure = None;
        for (inf_dir, scheme) in &schemes {
            let name = clean_name(&scheme.name).unwrap_or_else(|| fallback.clone());
            let theme =
                self.add_scheme(&source, &files, inf_dir, &name, scheme, &mut added.missing);
            match theme {
                Ok(theme) => added.themes.push(theme),
                Err(error) => {
                    added.skipped.push(format!("{name}: {error}"));
                    first_failure.get_or_insert(error);
                }
            }
        }
        if added.themes.is_empty() {
            return Err(first_failure.unwrap_or(ThemeError::NoCursors));
        }
        Ok(added)
    }

    /// One scheme as a theme: the cursor files it names, found in the pack, as
    /// the theme's own. A role whose file the pack lacks is left out, and an empty
    /// one stays the built-in cursor.
    fn add_scheme(
        &self,
        source: &Source,
        files: &[String],
        inf_dir: &str,
        name: &str,
        scheme: &Scheme,
        missing: &mut Vec<String>,
    ) -> Result<LibraryTheme, ThemeError> {
        let mut cursors: BTreeMap<String, String> = BTreeMap::new();
        // The theme's name for a file, and where in the pack it comes from.
        let mut from: HashMap<String, String> = HashMap::new();
        for (role, wanted) in &scheme.roles {
            if wanted.is_empty() {
                cursors.insert(role.name().to_owned(), String::new());
                continue;
            }
            let found = find(files, inf_dir, wanted).and_then(|member| {
                let path = format!("cursors/{}", base_of(member));
                member_name(&path, &[FileKind::Cursor, FileKind::AnimatedCursor]).ok()?;
                Some((member.clone(), path))
            });
            match found {
                Some((member, path)) => {
                    from.insert(path.clone(), member);
                    cursors.insert(role.name().to_owned(), path);
                }
                None if !missing.contains(wanted) => missing.push(wanted.clone()),
                None => {}
            }
        }
        if from.is_empty() {
            return Err(ThemeError::NoCursors);
        }

        let manifest = Manifest {
            format: FORMAT,
            id: self.fresh_id(name),
            name: name.to_owned(),
            author: UNSTATED.to_owned(),
            license: UNSTATED.to_owned(),
            version: "1".to_owned(),
            preview: None,
            cursors,
            sounds: BTreeMap::new(),
        };
        manifest.validate()?;
        let descriptor = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| ThemeError::Descriptor(error.to_string()))?;
        self.install(manifest, &descriptor, &|path, limit| match from.get(path) {
            Some(member) => source.read(member, limit),
            None => Err(ThemeError::BadFile {
                name: path.to_owned(),
                why: "is not a file of the pack".into(),
            }),
        })
    }

    /// An id for a theme called `name`: its letters and digits, made one nobody
    /// has. The names of the built-in themes are not taken.
    fn fresh_id(&self, name: &str) -> String {
        let mut base = slug(name);
        base.truncate(MAX_SLUG);
        let base = base.trim_matches('-');
        let base = if base.is_empty() { "cursors" } else { base };
        let base = if is_reserved(base) {
            format!("pack-{base}")
        } else {
            base.to_owned()
        };
        std::iter::once(base.clone())
            .chain((2..).map(|number| format!("{base}-{number}")))
            .find(|id| self.is_free(id))
            .expect("an unused number exists")
    }
}

fn is_inf(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".inf")
}

fn base_of(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn dir_of(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file of the pack called `wanted`, without regard to case: the one in the
/// folder of the INF if there is one, else the one nearest the top.
fn find<'a>(files: &'a [String], inf_dir: &str, wanted: &str) -> Option<&'a String> {
    files
        .iter()
        .filter(|file| base_of(file).eq_ignore_ascii_case(wanted))
        .min_by_key(|file| (dir_of(file) != inf_dir, file.matches('/').count()))
}

/// A name to show: no control characters, and not too long. `None` when nothing
/// is left.
fn clean_name(name: &str) -> Option<String> {
    let name: String = name
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_NAME_CHARS)
        .collect();
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

/// What a scheme that does not name itself is called: the pack, by the name of
/// its folder or file.
fn pack_name(path: &Path) -> String {
    let folder_name = |dir: &Path| {
        dir.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
    };
    let name = if path.is_dir() {
        folder_name(path)
    } else if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("inf"))
    {
        path.parent().and_then(folder_name)
    } else {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_owned)
    };
    name.and_then(|name| clean_name(&name))
        .unwrap_or_else(|| "Cursors".to_owned())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    const CUR: [u8; 8] = [0, 0, 2, 0, 1, 0, 32, 32];
    const ANI: &[u8] = b"RIFF\0\0\0\0ACONLIST";

    /// A pack's INF: one scheme, `pointer.cur` for the arrow, an animation for
    /// the busy cursor, `hand.cur` for the hand, the rest of the roles blank.
    const INF: &str = r#"
[DefaultInstall]
CopyFiles = Scheme.Cur
AddReg    = Scheme.Reg

[Scheme.Reg]
HKCU,"Control Panel\Cursors\Schemes","%SCHEME_NAME%",,"%10%\%CUR_DIR%\%pointer%,,,%10%\%CUR_DIR%\%busy%,,,,,,,,,,,%10%\%CUR_DIR%\%hand%"

[Strings]
CUR_DIR     = "Cursors\Moonlight"
SCHEME_NAME = "Moonlight Classic"
pointer     = "pointer.cur"
busy        = "busy.ani"
hand        = "hand.cur"
"#;

    fn library(dir: &Path) -> Library {
        Library::at(dir.join("themes"))
    }

    fn write(dir: &Path, files: &[(&str, &[u8])]) {
        for (name, bytes) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
    }

    fn zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in files {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn pack_files() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("Install.inf", INF.as_bytes()),
            ("pointer.cur", &CUR),
            ("busy.ani", ANI),
            ("hand.cur", &CUR),
        ]
    }

    #[test]
    fn a_folder_with_an_inf_becomes_a_theme_of_its_own() {
        let temp = tempfile::tempdir().unwrap();
        write(&temp.path().join("pack"), &pack_files());
        let library = library(temp.path());

        let added = library.import_pack(&temp.path().join("pack")).unwrap();

        assert_eq!(added.themes.len(), 1);
        assert!(added.missing.is_empty() && added.skipped.is_empty());
        let theme = &added.themes[0];
        assert_eq!(theme.id(), "moonlight-classic");
        assert_eq!(theme.manifest.name, "Moonlight Classic");
        assert_eq!(theme.manifest.author, UNSTATED);
        assert_eq!(theme.manifest.license, UNSTATED);
        let cursors = &theme.manifest.cursors;
        assert_eq!(cursors["Arrow"], "cursors/pointer.cur");
        assert_eq!(cursors["Wait"], "cursors/busy.ani");
        assert_eq!(cursors["Hand"], "cursors/hand.cur");
        assert_eq!(cursors["Help"], "", "a blank slot is the built-in cursor");
        assert!(theme.dir.join("cursors").join("pointer.cur").is_file());
        assert!(
            !theme.dir.join("Install.inf").exists(),
            "the INF is not kept"
        );
        assert_eq!(library.list().len(), 1, "it is a theme like any other");
        assert!(!library.root.join(".staging").exists());
    }

    #[test]
    fn the_inf_itself_or_a_zip_gives_the_same_theme() {
        let temp = tempfile::tempdir().unwrap();
        write(&temp.path().join("pack"), &pack_files());
        let nested: Vec<(String, &[u8])> = pack_files()
            .into_iter()
            .map(|(name, bytes)| (format!("Moonlight/{name}"), bytes))
            .collect();
        let nested: Vec<(&str, &[u8])> = nested
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes))
            .collect();
        zip(&temp.path().join("pack.zip"), &nested);

        let by_inf = library(&temp.path().join("a"))
            .import_pack(&temp.path().join("pack").join("Install.inf"))
            .unwrap();
        let by_zip = library(&temp.path().join("b"))
            .import_pack(&temp.path().join("pack.zip"))
            .unwrap();

        assert_eq!(by_inf.themes[0].manifest, by_zip.themes[0].manifest);
        assert!(
            by_zip.themes[0]
                .dir
                .join("cursors")
                .join("busy.ani")
                .is_file()
        );
    }

    #[test]
    fn a_file_is_found_without_regard_to_case_and_the_one_by_the_inf_wins() {
        let temp = tempfile::tempdir().unwrap();
        let mut ours = CUR.to_vec();
        ours.extend(b"by the inf");
        let mut theirs = CUR.to_vec();
        theirs.extend(b"elsewhere");
        write(
            &temp.path().join("pack"),
            &[
                ("Install.inf", INF.as_bytes()),
                ("POINTER.CUR", &ours),
                ("other/pointer.cur", &theirs),
                ("busy.ani", ANI),
                ("hand.cur", &CUR),
            ],
        );

        let added = library(temp.path())
            .import_pack(&temp.path().join("pack"))
            .unwrap();

        let theme = &added.themes[0];
        assert_eq!(theme.manifest.cursors["Arrow"], "cursors/POINTER.CUR");
        assert_eq!(
            std::fs::read(theme.dir.join("cursors").join("POINTER.CUR")).unwrap(),
            ours
        );
    }

    #[test]
    fn a_file_the_pack_lacks_is_said_and_the_rest_still_comes() {
        let temp = tempfile::tempdir().unwrap();
        write(
            &temp.path().join("pack"),
            &[
                ("Install.inf", INF.as_bytes()),
                ("pointer.cur", &CUR),
                ("busy.ani", ANI),
            ],
        );

        let added = library(temp.path())
            .import_pack(&temp.path().join("pack"))
            .unwrap();

        assert_eq!(added.missing, ["hand.cur"]);
        let cursors = &added.themes[0].manifest.cursors;
        assert!(cursors.contains_key("Arrow") && !cursors.contains_key("Hand"));
    }

    #[test]
    fn a_pack_that_has_nothing_to_take_is_refused_and_says_why() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        write(&temp.path().join("no-inf"), &[("pointer.cur", &CUR)]);
        write(
            &temp.path().join("no-scheme"),
            &[
                ("Install.inf", b"[Version]\nsignature=\"$CHICAGO$\"\n"),
                ("pointer.cur", &CUR),
            ],
        );
        write(
            &temp.path().join("no-files"),
            &[("Install.inf", INF.as_bytes())],
        );
        std::fs::write(temp.path().join("note.txt"), b"x").unwrap();

        let refused = |name: &str| library.import_pack(&temp.path().join(name)).unwrap_err();

        assert!(matches!(refused("no-inf"), ThemeError::NoInf));
        assert!(matches!(refused("no-scheme"), ThemeError::NoScheme));
        assert!(matches!(refused("no-files"), ThemeError::NoCursors));
        assert!(matches!(refused("note.txt"), ThemeError::NotAPack));
        assert!(!library.exists(), "refusals make nothing");
    }

    #[test]
    fn a_cursor_that_is_not_one_refuses_the_theme_and_leaves_nothing() {
        let temp = tempfile::tempdir().unwrap();
        write(
            &temp.path().join("pack"),
            &[
                ("Install.inf", INF.as_bytes()),
                ("pointer.cur", b"this is a text file"),
                ("busy.ani", ANI),
                ("hand.cur", &CUR),
            ],
        );
        let library = library(temp.path());

        let error = library.import_pack(&temp.path().join("pack")).unwrap_err();

        assert!(error.to_string().contains("pointer.cur"), "{error}");
        assert!(library.list().is_empty());
        assert!(!library.root.join(".staging").exists());
    }

    #[test]
    fn several_schemes_become_several_themes_and_a_bad_one_is_left_out() {
        let temp = tempfile::tempdir().unwrap();
        let inf = r#"
[Reg]
HKCU,"Control Panel\Cursors\Schemes","Good",,"good.cur"
HKCU,"Control Panel\Cursors\Schemes","Broken",,"broken.cur"
HKCU,"Control Panel\Cursors\Schemes","Absent",,"absent.cur"
"#;
        write(
            &temp.path().join("pack"),
            &[
                ("Install.inf", inf.as_bytes()),
                ("good.cur", &CUR),
                ("broken.cur", b"not a cursor"),
            ],
        );

        let added = library(temp.path())
            .import_pack(&temp.path().join("pack"))
            .unwrap();

        assert_eq!(added.themes.len(), 1);
        assert_eq!(added.themes[0].manifest.name, "Good");
        assert_eq!(added.skipped.len(), 2);
        assert!(
            added.skipped[0].starts_with("Broken: ") && added.skipped[1].starts_with("Absent: ")
        );
        assert_eq!(added.missing, ["absent.cur"]);
    }

    #[test]
    fn ids_come_from_names_and_are_never_reused_or_a_built_in_one() {
        let temp = tempfile::tempdir().unwrap();
        let library = library(temp.path());
        write(&temp.path().join("pack"), &pack_files());
        let reserved = INF.replace("Moonlight Classic", "Windows Aero");
        write(
            &temp.path().join("reserved"),
            &[
                ("Install.inf", reserved.as_bytes()),
                ("pointer.cur", &CUR),
                ("busy.ani", ANI),
                ("hand.cur", &CUR),
            ],
        );
        let russian = INF.replace("Moonlight Classic", "Лунный свет");
        write(
            &temp.path().join("russian"),
            &[
                ("Install.inf", russian.as_bytes()),
                ("pointer.cur", &CUR),
                ("busy.ani", ANI),
                ("hand.cur", &CUR),
            ],
        );

        let ids: Vec<String> = ["pack", "pack", "reserved", "russian"]
            .iter()
            .map(|name| library.import_pack(&temp.path().join(name)).unwrap())
            .map(|added| added.themes[0].id().to_owned())
            .collect();

        assert_eq!(
            ids,
            [
                "moonlight-classic",
                "moonlight-classic-2",
                "pack-windows-aero",
                "cursors"
            ]
        );
        assert_eq!(library.list().len(), 4);
    }

    #[test]
    fn a_scheme_without_a_name_is_called_after_its_pack() {
        let temp = tempfile::tempdir().unwrap();
        let inf = "[Reg]\nHKCU,\"Control Panel\\Cursors\\Schemes\",,,\"a.cur\"\n";
        write(
            &temp.path().join("Night Pointers"),
            &[("Install.inf", inf.as_bytes()), ("a.cur", &CUR)],
        );

        let added = library(temp.path())
            .import_pack(&temp.path().join("Night Pointers"))
            .unwrap();

        assert_eq!(added.themes[0].manifest.name, "Night Pointers");
        assert_eq!(added.themes[0].id(), "night-pointers");
    }

    #[test]
    fn files_too_deep_in_a_folder_are_not_part_of_the_pack() {
        let temp = tempfile::tempdir().unwrap();
        write(
            &temp.path().join("pack"),
            &[
                ("Install.inf", INF.as_bytes()),
                ("a/b/c/d/e/pointer.cur", &CUR),
                ("busy.ani", ANI),
                ("hand.cur", &CUR),
            ],
        );

        let added = library(temp.path())
            .import_pack(&temp.path().join("pack"))
            .unwrap();

        assert_eq!(added.missing, ["pointer.cur"]);
    }
}
