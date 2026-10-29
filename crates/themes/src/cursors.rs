//! Cursor sets: the roles Windows knows and the file that fills each one.

/// What the pointer is doing when Windows shows a cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Role {
    Arrow,
    Help,
    AppStarting,
    Wait,
    Crosshair,
    IBeam,
    NWPen,
    No,
    SizeNS,
    SizeWE,
    SizeNWSE,
    SizeNESW,
    SizeAll,
    UpArrow,
    Hand,
    Pin,
    Person,
}

/// Schemes written before Windows 10 name 15 roles, without `Pin` and `Person`.
const LEGACY_ROLES: usize = 15;

impl Role {
    /// In the order Windows lists them in a scheme.
    pub const ALL: [Role; 17] = [
        Role::Arrow,
        Role::Help,
        Role::AppStarting,
        Role::Wait,
        Role::Crosshair,
        Role::IBeam,
        Role::NWPen,
        Role::No,
        Role::SizeNS,
        Role::SizeWE,
        Role::SizeNWSE,
        Role::SizeNESW,
        Role::SizeAll,
        Role::UpArrow,
        Role::Hand,
        Role::Pin,
        Role::Person,
    ];

    /// The name of the registry value, which a theme file uses too.
    pub fn name(self) -> &'static str {
        match self {
            Role::Arrow => "Arrow",
            Role::Help => "Help",
            Role::AppStarting => "AppStarting",
            Role::Wait => "Wait",
            Role::Crosshair => "Crosshair",
            Role::IBeam => "IBeam",
            Role::NWPen => "NWPen",
            Role::No => "No",
            Role::SizeNS => "SizeNS",
            Role::SizeWE => "SizeWE",
            Role::SizeNWSE => "SizeNWSE",
            Role::SizeNESW => "SizeNESW",
            Role::SizeAll => "SizeAll",
            Role::UpArrow => "UpArrow",
            Role::Hand => "Hand",
            Role::Pin => "Pin",
            Role::Person => "Person",
        }
    }

    pub fn from_name(name: &str) -> Option<Role> {
        Role::ALL
            .into_iter()
            .find(|role| role.name().eq_ignore_ascii_case(name))
    }
}

/// A named set of cursor files, one per role. An empty path means the cursor
/// Windows has built in for that role. A role the set does not name is left as
/// it is when the set is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorSet {
    /// Lowercase letters, digits and dashes: safe in a journal name and a path.
    pub id: String,
    pub name: String,
    pub roles: Vec<(Role, String)>,
}

impl CursorSet {
    /// The file for `role`. `None` when the set does not name the role or
    /// leaves it to the built-in cursor.
    pub fn path(&self, role: Role) -> Option<&str> {
        self.roles
            .iter()
            .find(|(named, _)| *named == role)
            .map(|(_, path)| path.as_str())
            .filter(|path| !path.is_empty())
    }
}

/// The sets Windows ships, from its scheme list (`platform::cursors`).
/// `expand` resolves `%SystemRoot%` and the like in a path, and `name_of`
/// turns the reference to the scheme's display name into that name in the
/// current language. A scheme that cannot say its name keeps the one it is
/// stored under.
pub fn windows_sets(
    schemes: &[(String, String)],
    expand: impl Fn(&str) -> String,
    name_of: impl Fn(&str) -> Option<String>,
) -> Vec<CursorSet> {
    let mut sets: Vec<CursorSet> = Vec::new();
    for (scheme, list) in schemes {
        let fields: Vec<&str> = list.split(',').map(str::trim).collect();
        // The paths come first, then a reference to the display name:
        // `@%systemroot%\system32\main.cpl,-1020`.
        let paths: Vec<&str> = fields
            .iter()
            .copied()
            .take_while(|field| !field.starts_with('@'))
            .collect();
        if paths.len() < LEGACY_ROLES {
            continue;
        }
        let reference = fields[paths.len()..].join(",");
        let name = name_of(&reference).unwrap_or_else(|| scheme.clone());
        let roles = Role::ALL
            .iter()
            .zip(&paths)
            .map(|(role, path)| (*role, expand(path)))
            .collect();
        sets.push(CursorSet {
            id: unused_id(&sets, windows_id(scheme)),
            name,
            roles,
        });
    }
    sets
}

/// `windows-black-large` for "Windows Black (large)", `windows-magnified` for
/// "Magnified": every set Windows ships starts with `windows-`.
fn windows_id(scheme: &str) -> String {
    let slug = slug(scheme);
    if slug.starts_with("windows-") {
        slug
    } else if slug.is_empty() {
        "windows-scheme".to_owned()
    } else {
        format!("windows-{slug}")
    }
}

fn slug(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_owned()
}

fn unused_id(sets: &[CursorSet], id: String) -> String {
    let taken = |candidate: &str| sets.iter().any(|set| set.id == candidate);
    if !taken(&id) {
        return id;
    }
    (2..)
        .map(|number| format!("{id}-{number}"))
        .find(|candidate| !taken(candidate))
        .expect("an unused number exists")
}

/// Replaces `%NAME%` with what `var` says about that name. A name nobody knows
/// stays as written, percent signs included.
pub fn expand_env(text: &str, var: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        match var(&after[..end]) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "Windows Aero" as this Windows stores it: 17 paths, two of them empty,
    /// then the reference to the display name.
    const AERO: &str = concat!(
        r"%SystemRoot%\cursors\aero_arrow.cur,%SystemRoot%\cursors\aero_helpsel.cur,",
        r"%SystemRoot%\cursors\aero_working.ani,%SystemRoot%\cursors\aero_busy.ani,,,",
        r"%SystemRoot%\cursors\aero_pen.cur,%SystemRoot%\cursors\aero_unavail.cur,",
        r"%SystemRoot%\cursors\aero_ns.cur,%SystemRoot%\cursors\aero_ew.cur,",
        r"%SystemRoot%\cursors\aero_nwse.cur,%SystemRoot%\cursors\aero_nesw.cur,",
        r"%SystemRoot%\cursors\aero_move.cur,%SystemRoot%\cursors\aero_up.cur,",
        r"%SystemRoot%\cursors\aero_link.cur,%SystemRoot%\cursors\aero_pin.cur,",
        r"%SystemRoot%\cursors\aero_person.cur,@%systemroot%\system32\main.cpl,-1020",
    );

    fn windows_root(name: &str) -> Option<String> {
        name.eq_ignore_ascii_case("SystemRoot")
            .then(|| r"C:\WINDOWS".to_owned())
    }

    fn expand(path: &str) -> String {
        expand_env(path, windows_root)
    }

    fn one(scheme: &str, list: &str) -> Vec<CursorSet> {
        windows_sets(
            &[(scheme.to_owned(), list.to_owned())],
            expand,
            |reference| {
                (reference == "@%systemroot%\\system32\\main.cpl,-1020")
                    .then(|| "Windows Aero (localized)".into())
            },
        )
    }

    #[test]
    fn roles_run_in_the_order_windows_lists_them() {
        let names: Vec<&str> = Role::ALL.iter().map(|role| role.name()).collect();
        assert_eq!(
            names,
            [
                "Arrow",
                "Help",
                "AppStarting",
                "Wait",
                "Crosshair",
                "IBeam",
                "NWPen",
                "No",
                "SizeNS",
                "SizeWE",
                "SizeNWSE",
                "SizeNESW",
                "SizeAll",
                "UpArrow",
                "Hand",
                "Pin",
                "Person"
            ]
        );
    }

    #[test]
    fn a_role_is_found_by_its_registry_name_in_any_case() {
        assert_eq!(Role::from_name("ibeam"), Some(Role::IBeam));
        assert_eq!(Role::from_name("SizeNWSE"), Some(Role::SizeNWSE));
        assert_eq!(Role::from_name("Link"), None);
    }

    #[test]
    fn a_windows_scheme_gives_a_path_to_each_role() {
        let sets = one("Windows Aero", AERO);
        let [set] = sets.as_slice() else {
            panic!("one scheme, one set: {sets:?}")
        };
        assert_eq!(set.id, "windows-aero");
        assert_eq!(set.name, "Windows Aero (localized)");
        assert_eq!(set.roles.len(), 17);
        assert_eq!(
            set.roles[0],
            (Role::Arrow, r"C:\WINDOWS\cursors\aero_arrow.cur".to_owned())
        );
        assert_eq!(
            set.roles[16],
            (
                Role::Person,
                r"C:\WINDOWS\cursors\aero_person.cur".to_owned()
            )
        );
    }

    #[test]
    fn an_empty_path_stays_empty_for_the_built_in_cursor() {
        let sets = one("Windows Aero", AERO);
        let role = |wanted| {
            sets[0]
                .roles
                .iter()
                .find(|(role, _)| *role == wanted)
                .map(|(_, path)| path.as_str())
        };
        assert_eq!(role(Role::Crosshair), Some(""));
        assert_eq!(role(Role::IBeam), Some(""));
    }

    #[test]
    fn a_scheme_from_before_windows_10_has_fifteen_roles() {
        let list = (0..15)
            .map(|number| format!(r"%SystemRoot%\cursors\old{number}.cur"))
            .collect::<Vec<_>>()
            .join(",");
        let sets = one("Old", &list);
        assert_eq!(sets[0].roles.len(), 15);
        assert_eq!(
            sets[0].roles.last().map(|(role, _)| *role),
            Some(Role::Hand)
        );
        // No reference to a display name: the stored name stands.
        assert_eq!(sets[0].name, "Old");
    }

    #[test]
    fn a_scheme_with_too_few_paths_is_skipped() {
        assert!(one("Broken", r"%SystemRoot%\cursors\one.cur,two.cur").is_empty());
    }

    #[test]
    fn ids_start_with_windows_and_are_unique() {
        let schemes: Vec<(String, String)> = [
            "Windows Black (extra large)",
            "Windows Aero XL)",
            "Magnified",
            "Magnified",
        ]
        .iter()
        .map(|name| ((*name).to_owned(), AERO.to_owned()))
        .collect();
        let ids: Vec<String> = windows_sets(&schemes, expand, |_| None)
            .into_iter()
            .map(|set| set.id)
            .collect();
        assert_eq!(
            ids,
            [
                "windows-black-extra-large",
                "windows-aero-xl",
                "windows-magnified",
                "windows-magnified-2"
            ]
        );
    }

    #[test]
    fn variables_are_expanded_and_unknown_ones_stay() {
        assert_eq!(
            expand_env(r"%SystemRoot%\cursors\%Nobody%\a.cur", windows_root),
            r"C:\WINDOWS\cursors\%Nobody%\a.cur"
        );
        assert_eq!(expand_env("100% sure", windows_root), "100% sure");
        assert_eq!(expand_env("no variables", windows_root), "no variables");
    }
}
