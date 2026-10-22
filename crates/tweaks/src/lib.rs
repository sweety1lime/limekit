//! The tweak engine: changes to the per-user registry that LimeKit can always
//! undo.
//!
//! Every change goes through the journal first. Before touching a value the
//! engine writes down what was there, including "nothing was there"; then it
//! writes the new value, reads it back and compares. If any change of a set
//! fails, the ones already made are undone. Later, any change or whole set can
//! be undone, unless something else has changed the value since, and the undo
//! can be exported as a .reg file that works without LimeKit.

mod engine;
mod journal;
mod reg_file;
mod registry;
mod value;

pub use engine::{
    ApplyError, Before, Change, Drift, Engine, EngineError, Entry, EntryId, Outcome, RevertOutcome,
    SessionId, State, Target,
};
pub use journal::JournalError;
pub use reg_file::{undo_reg_file, undo_reg_text};
pub use registry::{CurrentUser, Registry};
pub use value::Value;
