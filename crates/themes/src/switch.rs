//! Putting a theme on and taking it off through the tweak engine.

use tweaks::{
    ApplyError, Change, Engine, EngineError, EntryId, Outcome, Parameters, Registry, RevertOutcome,
    State, Target, Value,
};

use crate::cursors::CursorSet;

/// The key the cursor roles live under, below HKEY_CURRENT_USER.
pub const CURSORS_KEY: &str = r"Control Panel\Cursors";

/// Journal names of themes start with this, which tells them from tweaks.
pub const PREFIX: &str = "theme.";

/// One part of a theme. Parts go on and come off independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Cursors,
}

impl Part {
    fn name(self) -> &'static str {
        match self {
            Part::Cursors => "cursors",
        }
    }

    fn from_name(name: &str) -> Option<Part> {
        (name == Part::Cursors.name()).then_some(Part::Cursors)
    }
}

/// `theme.cursors.<id>`: what the journal calls one part of a theme.
pub fn journal_id(part: Part, theme: &str) -> String {
    format!("{PREFIX}{}.{theme}", part.name())
}

/// Whether a journal name belongs to a theme and not to a tweak.
pub fn is_theme_entry(name: &str) -> bool {
    name.starts_with(PREFIX)
}

/// The part and the theme a journal name stands for, the reverse of
/// [`journal_id`].
pub fn parse_journal_id(name: &str) -> Option<(Part, &str)> {
    let (part, theme) = name.strip_prefix(PREFIX)?.split_once('.')?;
    Some((Part::from_name(part)?, theme))
}

/// The theme whose `part` LimeKit has in effect: the newest one still applied.
pub fn active<R: Registry + Parameters>(engine: &Engine<R>, part: Part) -> Option<String> {
    engine
        .entries()
        .iter()
        .rev()
        .filter(|entry| entry.state == State::Applied)
        .find_map(|entry| match parse_journal_id(&entry.tweak) {
            Some((found, theme)) if found == part => Some(theme.to_owned()),
            _ => None,
        })
}

/// The registry changes that give the current user the cursors of `set`, under
/// `key`: `CURSORS_KEY`, or a private key in tests.
pub fn cursor_changes(set: &CursorSet, key: &str) -> Vec<Change> {
    set.roles
        .iter()
        .map(|(role, path)| Change {
            target: Target::Registry {
                path: key.to_owned(),
                name: role.name().to_owned(),
            },
            value: Some(Value::Sz(path.clone())),
        })
        .collect()
}

/// What putting a theme on did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// What the engine did with each change.
    pub outcomes: Vec<Outcome>,
    /// Values of the theme that was in effect before. Something changed them
    /// after it went on, so they were left as they are.
    pub left_alone: Vec<Target>,
}

#[derive(Debug, thiserror::Error)]
pub enum SwitchError {
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Apply(#[from] ApplyError),
}

/// Puts `changes` on as `theme`'s `part`. A theme already in effect for that
/// part comes off first, so themes never pile up in the journal: what was there
/// before LimeKit, then the new theme. If the new one cannot go on, the old one
/// stays off.
pub fn put_on<R: Registry + Parameters>(
    engine: &mut Engine<R>,
    part: Part,
    theme: &str,
    changes: &[Change],
) -> Result<Applied, SwitchError> {
    let wanted = journal_id(part, theme);
    let prefix = format!("{PREFIX}{}.", part.name());
    let mut left_alone = Vec::new();
    for id in in_effect(engine, |name| name.starts_with(&prefix) && name != wanted) {
        revert(engine, id, &mut left_alone)?;
    }
    let (_, outcomes) = engine.apply(&wanted, changes)?;
    Ok(Applied {
        outcomes,
        left_alone,
    })
}

/// Takes `theme`'s `part` off: the values go back to what they were before
/// LimeKit. Values something changed since stay as they are, and come back as
/// the result.
pub fn take_off<R: Registry + Parameters>(
    engine: &mut Engine<R>,
    part: Part,
    theme: &str,
) -> Result<Vec<Target>, EngineError> {
    let wanted = journal_id(part, theme);
    let mut left_alone = Vec::new();
    for id in in_effect(engine, |name| name == wanted) {
        revert(engine, id, &mut left_alone)?;
    }
    Ok(left_alone)
}

/// The applied changes whose journal name satisfies `matches`, newest first.
fn in_effect<R: Registry + Parameters>(
    engine: &Engine<R>,
    matches: impl Fn(&str) -> bool,
) -> Vec<EntryId> {
    engine
        .entries()
        .iter()
        .rev()
        .filter(|entry| entry.state == State::Applied && matches(&entry.tweak))
        .map(|entry| entry.id)
        .collect()
}

fn revert<R: Registry + Parameters>(
    engine: &mut Engine<R>,
    id: EntryId,
    left_alone: &mut Vec<Target>,
) -> Result<(), EngineError> {
    if let RevertOutcome::ChangedSince { .. } = engine.revert(id, false)?
        && let Some(entry) = engine.entries().iter().find(|entry| entry.id == id)
    {
        left_alone.push(entry.target.clone());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_journal_name_says_its_part_and_theme() {
        let name = journal_id(Part::Cursors, "windows-black");
        assert_eq!(name, "theme.cursors.windows-black");
        assert_eq!(
            parse_journal_id(&name),
            Some((Part::Cursors, "windows-black"))
        );
        assert!(is_theme_entry(&name));
    }

    #[test]
    fn a_tweak_name_is_not_a_theme() {
        assert_eq!(parse_journal_id("explorer.file_extensions"), None);
        assert_eq!(parse_journal_id("theme.sounds.quiet"), None);
        assert!(!is_theme_entry("explorer.file_extensions"));
    }
}
