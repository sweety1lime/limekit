//! Reading the cursor schemes out of an `Install.inf`, which is what cursor
//! packs come with. The INF is only read, never run: installing by it copies
//! files into `C:\Windows`, which takes an administrator, and LimeKit does not
//! ask for one. What is worth having from it is which cursor file goes with
//! which role, and that is all this takes.

use std::collections::HashMap;

use crate::cursors::{Role, expand_env};

/// An `.inf` bigger than this is not one a cursor pack ships.
pub(crate) const MAX_INF_BYTES: u64 = 256 * 1024;
/// A line longer than this is skipped: a scheme names 17 paths.
const MAX_LINE_CHARS: usize = 16 * 1024;
const MAX_LINES: usize = 20_000;
/// How many times a `%name%` may expand to text with another one in it.
const EXPANSION_PASSES: usize = 3;
/// The key a scheme is registered under, as the INF may spell it in full or
/// after `HKLM,"SOFTWARE\Microsoft\Windows\CurrentVersion\`.
const SCHEMES_KEY: &str = r"control panel\cursors\schemes";
/// The key the cursors in effect are written to.
const CURSORS_KEY: &str = r"control panel\cursors";

/// A scheme the INF registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheme {
    /// What the INF calls it; empty when it does not say.
    pub name: String,
    /// The file for each role, by name only: where the INF would copy it to does
    /// not matter, only which file of the pack it is. An empty name is the
    /// built-in cursor.
    pub roles: Vec<(Role, String)>,
}

/// The text of an `.inf`: UTF-16 or UTF-8 when a byte order mark says so, UTF-8
/// when the bytes are valid as that, the system code page otherwise.
pub fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, u16::from_be_bytes);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => platform::text::from_ansi(bytes),
    }
}

fn utf16(bytes: &[u8], word: fn([u8; 2]) -> u16) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| word([pair[0], pair[1]]))
        .collect();
    String::from_utf16_lossy(&units)
}

/// The schemes the INF registers, in the order it does. When it registers none
/// but sets the cursors in effect directly, that is one scheme. A scheme that
/// names no cursor file is left out.
pub fn schemes(text: &str) -> Vec<Scheme> {
    let lines = lines(text);
    let table = strings(&lines);
    let mut registered: Vec<Scheme> = Vec::new();
    let mut direct: Vec<(Role, String)> = Vec::new();
    let mut direct_name = String::new();

    for line in lines.iter().filter(|line| !is_strings(&line.section)) {
        let fields: Vec<String> = fields(&line.text)
            .iter()
            .map(|field| expand(field, &table))
            .collect();
        // `root, key, value name, flags, value`; a value with commas that were
        // not quoted comes in several fields.
        if fields.len() < 5 || !is_registry_root(&fields[0]) {
            continue;
        }
        let key = fields[1]
            .replace('/', "\\")
            .trim_matches('\\')
            .to_ascii_lowercase();
        let name = fields[2].as_str();
        let value = fields[4..].join(",");
        if key.ends_with(SCHEMES_KEY) {
            let roles = roles_of(&value);
            if !roles.is_empty() && !registered.iter().any(|scheme| scheme.name == name) {
                registered.push(Scheme {
                    name: name.to_owned(),
                    roles,
                });
            }
        } else if key == CURSORS_KEY {
            if name.is_empty() {
                direct_name = value.trim().to_owned();
            } else if let Some(role) = Role::from_name(name) {
                let file = file_name(&value);
                if is_cursor_file(file) {
                    direct.retain(|(set, _)| *set != role);
                    direct.push((role, file.to_owned()));
                }
            }
        }
    }

    if registered.is_empty() && !direct.is_empty() {
        direct.sort_by_key(|(role, _)| *role);
        registered.push(Scheme {
            name: direct_name,
            roles: direct,
        });
    }
    registered.retain(|scheme| scheme.roles.iter().any(|(_, file)| !file.is_empty()));
    registered
}

/// A scheme's list of paths, one per role in the order Windows lists the roles.
/// An empty entry is the built-in cursor; one that is not a cursor file is not
/// taken. The reference to the display name that Windows' own schemes end with
/// is not a path and stops the list.
fn roles_of(list: &str) -> Vec<(Role, String)> {
    list.split(',')
        .map(str::trim)
        .take_while(|entry| !entry.starts_with('@'))
        .zip(Role::ALL)
        .filter_map(|(entry, role)| {
            if entry.is_empty() {
                return Some((role, String::new()));
            }
            let file = file_name(entry);
            is_cursor_file(file).then(|| (role, file.to_owned()))
        })
        .collect()
}

/// The file name at the end of a path, which may use either slash.
fn file_name(path: &str) -> &str {
    path.rsplit(['\\', '/']).next().unwrap_or(path).trim()
}

fn is_cursor_file(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with(".cur") || name.ends_with(".ani")
}

fn is_registry_root(field: &str) -> bool {
    matches!(
        field.to_ascii_uppercase().as_str(),
        "HKCU" | "HKLM" | "HKR" | "HKCR" | "HKU" | "HKEY_CURRENT_USER" | "HKEY_LOCAL_MACHINE"
    )
}

fn is_strings(section: &str) -> bool {
    section.starts_with("strings")
}

/// A line of the INF with the section it sits in, lowercase.
struct Line {
    section: String,
    text: String,
}

/// The INF as lines: comments gone, continued lines joined. A line or a file
/// past the limits is not read further.
fn lines(text: &str) -> Vec<Line> {
    let mut out: Vec<Line> = Vec::new();
    let mut section = String::new();
    let mut joined = String::new();
    // Whether a continued line left a string open, which a `;` on the next one
    // is then part of.
    let mut quoted = false;
    for raw in text.lines() {
        if out.len() >= MAX_LINES {
            break;
        }
        let line = strip_comment(raw, &mut quoted);
        let line = line.trim();
        // A backslash at the very end continues the line, inside quotes too:
        // that is how long lists of cursors are wrapped.
        if let Some(head) = line.strip_suffix('\\') {
            joined.push_str(head);
            if joined.len() > MAX_LINE_CHARS {
                joined.clear();
            }
            continue;
        }
        quoted = false;
        joined.push_str(line);
        let complete = std::mem::take(&mut joined);
        let complete = complete.trim();
        if complete.is_empty() || complete.len() > MAX_LINE_CHARS {
            continue;
        }
        if let Some(name) = complete
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name.trim().to_ascii_lowercase();
        } else {
            out.push(Line {
                section: section.clone(),
                text: complete.to_owned(),
            });
        }
    }
    out
}

/// `line` up to a `;` that is not inside quotes. `quoted` says whether a string
/// is open where the line starts, and is left saying whether one is open where
/// it ends.
fn strip_comment<'a>(line: &'a str, quoted: &mut bool) -> &'a str {
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => *quoted = !*quoted,
            ';' if !*quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

/// Splits a line at its commas, except those inside quotes. Each field is
/// trimmed and unquoted, a doubled quote inside quotes standing for one.
fn fields(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                out.push(field.trim().to_owned());
                field.clear();
            }
            _ => field.push(ch),
        }
    }
    out.push(field.trim().to_owned());
    out
}

/// The `[Strings]` of the INF, which `%name%` in the rest of the file stands
/// for, by lowercase name. A name defined twice keeps its first meaning.
fn strings(lines: &[Line]) -> HashMap<String, String> {
    let mut table: HashMap<String, String> = HashMap::new();
    for line in lines.iter().filter(|line| is_strings(&line.section)) {
        let Some((name, value)) = line.text.split_once('=') else {
            continue;
        };
        let value = fields(value).into_iter().next().unwrap_or_default();
        table
            .entry(name.trim().trim_matches('"').to_ascii_lowercase())
            .or_insert(value);
    }
    let names = table.clone();
    for value in table.values_mut() {
        *value = expand(value, &names);
    }
    table
}

/// Replaces `%name%` with what the INF's strings say, as far as that goes: a few
/// rounds, so a string that names another one is followed, and no further, so a
/// string that names itself is not. A `%10%`, which stands for a folder the
/// installer picks, stays as it is.
fn expand(text: &str, table: &HashMap<String, String>) -> String {
    let mut text = text.to_owned();
    for _ in 0..EXPANSION_PASSES {
        if !text.contains('%') {
            break;
        }
        let next = expand_env(&text, |name| table.get(&name.to_ascii_lowercase()).cloned());
        if next == text || next.len() > MAX_LINE_CHARS {
            break;
        }
        text = next;
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What cursor packs ship, in the shape most of them have: the scheme
    /// registered under the user's key with strings for the folder, the name
    /// and every file, and an installer line that has nothing to do with
    /// cursors.
    const PACK: &str = r#"
[Version]
signature="$CHICAGO$"

[DefaultInstall]
CopyFiles = Scheme.Cur
AddReg    = Scheme.Reg
; the files go to the Windows folder

[DestinationDirs]
Scheme.Cur = 10,"%CUR_DIR%"

[Scheme.Reg]
HKCU,"Control Panel\Cursors\Schemes","%SCHEME_NAME%",,"%10%\%CUR_DIR%\%pointer%,%10%\%CUR_DIR%\%help%,%10%\%CUR_DIR%\%work%,%10%\%CUR_DIR%\%busy%,%10%\%CUR_DIR%\%cross%,%10%\%CUR_DIR%\%text%,%10%\%CUR_DIR%\%hand%,%10%\%CUR_DIR%\%unavail%,%10%\%CUR_DIR%\%vert%,%10%\%CUR_DIR%\%horz%,%10%\%CUR_DIR%\%dgn1%,%10%\%CUR_DIR%\%dgn2%,%10%\%CUR_DIR%\%move%,%10%\%CUR_DIR%\%alternate%,%10%\%CUR_DIR%\%link%,%10%\%CUR_DIR%\%pin%,%10%\%CUR_DIR%\%person%"
HKLM,"SOFTWARE\Microsoft\Windows\CurrentVersion\Runonce\Setup\","",,"rundll32.exe shell32.dll,Control_RunDLL main.cpl @0"

[Strings]
CUR_DIR       = "Cursors\Moonlight"
SCHEME_NAME   = "Moonlight Classic"
pointer       = "pointer.ani"
help          = "help.ani"
work          = "work.ani"
busy          = "busy.ani"
cross         = "cross.cur"
text          = "text.cur"
hand          = "hand.cur"
unavail       = "unavail.cur"
vert          = "vert.cur"
horz          = "horz.cur"
dgn1          = "dgn1.cur"
dgn2          = "dgn2.cur"
move          = "move.cur"
alternate     = "alternate.cur"
link          = "link.cur"
pin           = "pin.cur"
person        = "person.cur"
"#;

    fn roles(scheme: &Scheme) -> Vec<(&'static str, &str)> {
        scheme
            .roles
            .iter()
            .map(|(role, file)| (role.name(), file.as_str()))
            .collect()
    }

    #[test]
    fn a_scheme_is_read_from_the_user_key_with_its_strings_filled_in() {
        let found = schemes(PACK);

        assert_eq!(found.len(), 1, "the Runonce line is not a scheme");
        let scheme = &found[0];
        assert_eq!(scheme.name, "Moonlight Classic");
        assert_eq!(scheme.roles.len(), 17);
        let named = roles(scheme);
        assert_eq!(named[0], ("Arrow", "pointer.ani"));
        assert_eq!(named[2], ("AppStarting", "work.ani"));
        assert_eq!(named[5], ("IBeam", "text.cur"));
        assert_eq!(named[14], ("Hand", "link.cur"));
        assert_eq!(named[16], ("Person", "person.cur"));
    }

    #[test]
    fn nothing_in_it_is_run_or_followed() {
        let found = schemes(PACK);

        // Only cursor files come out: not the installer line, not a folder.
        for (_, file) in &found[0].roles {
            assert!(is_cursor_file(file), "{file}");
        }
    }

    #[test]
    fn an_xp_scheme_has_fifteen_paths_and_blank_ones_are_the_built_in_cursor() {
        let text = concat!(
            "[Scheme.Reg]\r\n",
            r#"HKCU,"Control Panel\Cursors\Schemes","Old",,"a.cur,b.cur,,d.ani,,f.cur,,,,,,,,,o.cur""#,
            "\r\n",
        );

        let found = schemes(text);

        let named = roles(&found[0]);
        assert_eq!(named.len(), 15);
        assert_eq!(named[0], ("Arrow", "a.cur"));
        assert_eq!(named[2], ("AppStarting", ""));
        assert_eq!(named[3], ("Wait", "d.ani"));
        assert_eq!(named[14], ("Hand", "o.cur"));
    }

    #[test]
    fn a_scheme_in_the_machine_key_counts_and_so_does_a_second_one() {
        let text = r#"
[Reg]
hklm,"SOFTWARE\Microsoft\Windows\CurrentVersion\Control Panel\Cursors\Schemes","Big",,"big.cur"
HKCU,"CONTROL PANEL\CURSORS\SCHEMES","Small",,"small.cur"
"#;

        let found = schemes(text);

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "Big");
        assert_eq!(found[1].name, "Small");
        assert_eq!(roles(&found[1]), [("Arrow", "small.cur")]);
    }

    #[test]
    fn cursors_set_directly_make_a_scheme_when_none_is_registered() {
        let text = r#"
[Reg]
HKCU,"Control Panel\Cursors",,0x00020000,"Direct set"
HKCU,"Control Panel\Cursors",Hand,0x00020000,"%10%\Cursors\x\link.cur"
HKCU,"Control Panel\Cursors",Arrow,0x00020000,"%10%\Cursors\x\arrow.cur"
HKCU,"Control Panel\Cursors",Wait,0x00020000,"%10%\Cursors\x\clock.png"
"#;

        let found = schemes(text);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Direct set");
        assert_eq!(
            roles(&found[0]),
            [("Arrow", "arrow.cur"), ("Hand", "link.cur")],
            "in the order of the roles, and a picture is not a cursor"
        );
    }

    #[test]
    fn a_scheme_that_is_also_set_directly_is_not_listed_twice() {
        let text = r#"
[Reg]
HKCU,"Control Panel\Cursors\Schemes","Both",,"a.cur"
HKCU,"Control Panel\Cursors",,0x00020000,"Both"
HKCU,"Control Panel\Cursors",Arrow,0x00020000,"a.cur"
"#;

        assert_eq!(schemes(text).len(), 1);
    }

    #[test]
    fn comments_and_continued_lines_are_read_as_the_inf_means_them() {
        let text = "[Reg]\r\n\
            ; HKCU,\"Control Panel\\Cursors\\Schemes\",\"Commented\",,\"x.cur\"\r\n\
            HKCU,\"Control Panel\\Cursors\\Schemes\",\"Joined\",,\"a.cur,\\\r\n\
            b.cur\" ; the second half is on the next line\r\n";

        let found = schemes(text);

        assert_eq!(found.len(), 1, "the commented one is not a scheme");
        assert_eq!(found[0].name, "Joined");
        assert_eq!(roles(&found[0]), [("Arrow", "a.cur"), ("Help", "b.cur")]);
    }

    #[test]
    fn a_name_that_cannot_be_filled_in_stays_as_it_is_and_loops_end() {
        let text = r#"
[Reg]
HKCU,"Control Panel\Cursors\Schemes","%LOOP%",,"%nobody%\a.cur,%B%"

[Strings]
LOOP = "%LOOP%x"
B    = "%C%"
C    = "%B%"
"#;

        let found = schemes(text);

        assert_eq!(found.len(), 1);
        // The loop is cut after a few rounds, not followed forever, and what is
        // left is still a usable name; the unknown folder and `%B%`, which
        // never resolves to a cursor file, do not stop the real one.
        assert!(found[0].name.starts_with("%LOOP%") && found[0].name.len() < 64);
        assert_eq!(roles(&found[0]), [("Arrow", "a.cur")]);
    }

    #[test]
    fn an_expansion_that_would_swell_stops_at_the_limit() {
        let mut text = String::from(
            "[Reg]\nHKCU,\"Control Panel\\Cursors\\Schemes\",\"%A%\",,\"a.cur\"\n[Strings]\n",
        );
        text.push_str("A = \"%A%%A%%A%%A%%A%%A%%A%%A%\"\n");

        let found = schemes(&text);

        assert!(found.len() <= 1);
        if let Some(scheme) = found.first() {
            assert!(scheme.name.len() <= MAX_LINE_CHARS);
        }
    }

    #[test]
    fn what_is_not_a_cursor_file_is_not_taken_and_garbage_gives_nothing() {
        let text = r#"HKCU,"Control Panel\Cursors\Schemes","Mixed",,"a.cur,b.exe,c.ico,d.ani,e""#;

        let found = schemes(text);

        assert_eq!(roles(&found[0]), [("Arrow", "a.cur"), ("Wait", "d.ani")]);
        assert!(schemes("").is_empty());
        assert!(schemes("not an inf at all\n[unclosed\nHKCU,,,").is_empty());
        assert!(
            schemes(r#"HKCU,"Control Panel\Cursors\Schemes","Empty",,",,,,""#).is_empty(),
            "a scheme that names no file has nothing to give"
        );
    }

    #[test]
    fn a_path_may_use_either_slash() {
        assert_eq!(file_name(r"%10%\Cursors\x\a.cur"), "a.cur");
        assert_eq!(file_name("Cursors/x/a.cur"), "a.cur");
        assert_eq!(file_name("a.cur"), "a.cur");
    }

    #[test]
    fn the_text_is_decoded_by_its_byte_order_mark() {
        let text = "[Strings]\nName = \"Курсор\"\n";
        let mut le = vec![0xFF, 0xFE];
        le.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
        let mut be = vec![0xFE, 0xFF];
        be.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
        let mut utf8 = vec![0xEF, 0xBB, 0xBF];
        utf8.extend(text.as_bytes());

        assert_eq!(decode(&le), text);
        assert_eq!(decode(&be), text);
        assert_eq!(decode(&utf8), text);
        assert_eq!(decode(text.as_bytes()), text);
    }

    #[test]
    fn a_unicode_inf_gives_the_same_scheme_as_a_plain_one() {
        let mut le = vec![0xFF, 0xFE];
        le.extend(PACK.encode_utf16().flat_map(u16::to_le_bytes));

        assert_eq!(schemes(&decode(&le)), schemes(PACK));
    }
}
