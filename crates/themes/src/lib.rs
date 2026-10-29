//! Themes: cursors and sounds that LimeKit can always take back.
//!
//! A theme becomes registry changes for the tweak engine, so it goes through
//! the same journal as a tweak: written down first, read back, undone on
//! request. Each part of a theme (cursors, sounds) is its own set of changes,
//! named `theme.<part>.<id>` in the journal, which keeps themes apart from
//! tweaks in the one file.

mod cursors;
mod inf;
mod library;
mod pack;
mod package;
mod sounds;
mod switch;

pub use cursors::{CursorSet, Role, expand_env, windows_sets};
pub use library::{Library, LibraryTheme};
pub use pack::PackImport;
pub use package::{
    DESCRIPTOR, EXTENSION, FORMAT, Manifest, ThemeError, UNSTATED, has_theme_extension, is_valid_id,
};
pub use sounds::{NO_SOUNDS, SOUNDS_KEY, SoundChoice, SoundSet, silence, sound_changes};
pub use switch::{
    Applied, CURSORS_KEY, PREFIX, Part, SwitchError, active, cursor_changes, is_theme_entry,
    journal_id, parse_journal_id, put_on, take_off,
};

/// Builds of Windows the themes were checked on by hand, the way tweaks are.
const VERIFIED_BUILDS: &[u32] = &[];

/// Where themes apply and on which builds they were checked.
pub fn requirements() -> tweaks::Requirements {
    tweaks::Requirements {
        verified: VERIFIED_BUILDS.to_vec(),
        ..Default::default()
    }
}
