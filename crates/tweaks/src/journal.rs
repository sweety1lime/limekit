//! The journal: an append-only file of JSON lines, one event per line, each
//! flushed to disk before the engine moves on. A change is written down before
//! the registry is touched and marked applied, failed or reverted afterwards,
//! so a crash at any point leaves enough on disk to tell what happened.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use platform::parameters::Parameter;
use serde::{Deserialize, Serialize};

use crate::value::Value;

/// The first line of every journal; a newer format gets a new number.
const FORMAT: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EntryId(pub u64);

/// The changes made by one call to apply, undone together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub u64);

/// What a change touches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// Value `name` of the key at `path` under HKEY_CURRENT_USER. An empty
    /// name is the key's default value.
    Registry { path: String, name: String },
    /// A per-user system parameter set through SystemParametersInfo. Its
    /// value is `{"type":"dword","data":1}` for on and 0 for off.
    Parameter { name: Parameter },
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Target::Registry { path, name } if name.is_empty() => {
                write!(f, r"HKCU\{path} (default)")
            }
            Target::Registry { path, name } => write!(f, r"HKCU\{path}\{name}"),
            Target::Parameter { name } => write!(f, "{name}"),
        }
    }
}

/// What was there before a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Before {
    /// `None` when the value did not exist.
    pub value: Option<Value>,
    /// The outermost key the change had to create. Undo removes the keys it
    /// created again, as long as nothing else has moved into them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Written before the registry is touched.
    Intent {
        entry: EntryId,
        session: SessionId,
        /// Seconds since the Unix epoch.
        at: u64,
        tweak: String,
        target: Target,
        before: Before,
        /// `None` when the change removes the value.
        after: Option<Value>,
    },
    /// The change was written and read back as intended.
    Applied { entry: EntryId, at: u64 },
    /// The change did not take; the value was left as it had been.
    Failed {
        entry: EntryId,
        at: u64,
        reason: String,
    },
    /// The change was undone.
    Reverted { entry: EntryId, at: u64 },
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    journal: String,
    format: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal {}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
    #[error("journal {}, line {line}: {message}", path.display())]
    Corrupt {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("journal {} is format {found}; this LimeKit reads format {FORMAT}", path.display())]
    Format { path: PathBuf, found: u32 },
}

pub struct Journal {
    path: PathBuf,
    file: File,
}

impl Journal {
    /// Opens the journal at `path`, creating it when missing, and returns it
    /// with the events already in it.
    ///
    /// A last line cut short by a crash is dropped from the file: the engine
    /// had not acted on it yet, because it writes the line before acting. A
    /// damaged line anywhere else is an error, not something to skip quietly.
    pub fn open(path: &Path) -> Result<(Journal, Vec<Event>), JournalError> {
        let io_error = |source| JournalError::Io {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(io_error)?;
        }
        // Write access rather than append: on Windows an append-only handle
        // cannot cut off a torn last line. Every write seeks to the end.
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(io_error)?;
        let mut text = String::new();
        file.read_to_string(&mut text).map_err(io_error)?;

        let mut journal = Journal {
            path: path.to_owned(),
            file,
        };
        if text.is_empty() {
            journal.write_line(&Header {
                journal: "limekit".into(),
                format: FORMAT,
            })?;
            return Ok((journal, Vec::new()));
        }

        let mut events = Vec::new();
        let mut offset = 0;
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        for (index, line) in lines.iter().enumerate() {
            let number = index + 1;
            let torn = index == lines.len() - 1 && !line.ends_with('\n');
            if index == 0 {
                if torn && journal.check_header(line, number).is_err() {
                    // The crash came while the journal was being created.
                    journal.file.set_len(0).map_err(io_error)?;
                    journal.write_line(&Header {
                        journal: "limekit".into(),
                        format: FORMAT,
                    })?;
                    return Ok((journal, Vec::new()));
                }
                journal.check_header(line, number)?;
            } else {
                match serde_json::from_str::<Event>(line) {
                    Ok(event) => events.push(event),
                    Err(_) if torn => {
                        journal.file.set_len(offset as u64).map_err(io_error)?;
                        break;
                    }
                    Err(error) => return Err(journal.corrupt(number, error.to_string())),
                }
            }
            if torn {
                // Complete but for its newline: finish it so the next event
                // starts on a line of its own.
                journal
                    .file
                    .seek(SeekFrom::End(0))
                    .and_then(|_| journal.file.write_all(b"\n"))
                    .map_err(io_error)?;
            }
            offset += line.len();
        }
        Ok((journal, events))
    }

    /// Appends `event` and flushes it to disk.
    pub fn append(&mut self, event: &Event) -> Result<(), JournalError> {
        self.write_line(event)
    }

    fn write_line(&mut self, record: &impl Serialize) -> Result<(), JournalError> {
        let mut line = serde_json::to_string(record).map_err(|error| JournalError::Io {
            path: self.path.clone(),
            source: io::Error::other(error),
        })?;
        line.push('\n');
        self.file
            .seek(SeekFrom::End(0))
            .and_then(|_| self.file.write_all(line.as_bytes()))
            .and_then(|()| self.file.sync_data())
            .map_err(|source| JournalError::Io {
                path: self.path.clone(),
                source,
            })
    }

    fn check_header(&self, line: &str, number: usize) -> Result<(), JournalError> {
        let header: Header = serde_json::from_str(line)
            .map_err(|error| self.corrupt(number, format!("not a journal header: {error}")))?;
        if header.journal != "limekit" {
            return Err(self.corrupt(number, "not a LimeKit journal".into()));
        }
        if header.format != FORMAT {
            return Err(JournalError::Format {
                path: self.path.clone(),
                found: header.format,
            });
        }
        Ok(())
    }

    fn corrupt(&self, line: usize, message: String) -> JournalError {
        JournalError::Corrupt {
            path: self.path.clone(),
            line,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn applied(entry: u64) -> Event {
        Event::Applied {
            entry: EntryId(entry),
            at: 1,
        }
    }

    fn journal_path(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("journal").join("journal.jsonl")
    }

    #[test]
    fn a_new_journal_starts_with_its_header() {
        let dir = tempfile::tempdir().unwrap();
        Journal::open(&journal_path(&dir)).unwrap();

        let text = std::fs::read_to_string(journal_path(&dir)).unwrap();
        assert_eq!(text, "{\"journal\":\"limekit\",\"format\":1}\n");
    }

    #[test]
    fn appended_events_come_back_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let (mut journal, _) = Journal::open(&journal_path(&dir)).unwrap();
        journal.append(&applied(1)).unwrap();
        journal.append(&applied(2)).unwrap();
        drop(journal);

        let (_, events) = Journal::open(&journal_path(&dir)).unwrap();
        assert_eq!(events, vec![applied(1), applied(2)]);
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let (mut journal, _) = Journal::open(&journal_path(&dir)).unwrap();
        journal.append(&applied(1)).unwrap();
        drop(journal);
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal_path(&dir))
            .unwrap();
        file.write_all(br#"{"event":"applied","ent"#).unwrap();
        drop(file);

        let (mut journal, events) = Journal::open(&journal_path(&dir)).unwrap();
        journal.append(&applied(2)).unwrap();
        drop(journal);
        let (_, events_after) = Journal::open(&journal_path(&dir)).unwrap();
        assert_eq!(
            (events, events_after),
            (vec![applied(1)], vec![applied(1), applied(2)])
        );
    }

    #[test]
    fn a_damaged_line_in_the_middle_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let (mut journal, _) = Journal::open(&journal_path(&dir)).unwrap();
        journal.append(&applied(1)).unwrap();
        drop(journal);
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal_path(&dir))
            .unwrap();
        file.write_all(b"garbage\n").unwrap();
        file.write_all(b"{\"event\":\"applied\",\"entry\":2,\"at\":1}\n")
            .unwrap();
        drop(file);

        let error = Journal::open(&journal_path(&dir)).err().unwrap();
        assert!(matches!(error, JournalError::Corrupt { line: 3, .. }));
    }

    #[test]
    fn a_newer_format_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(journal_path(&dir).parent().unwrap()).unwrap();
        std::fs::write(
            journal_path(&dir),
            "{\"journal\":\"limekit\",\"format\":2}\n",
        )
        .unwrap();

        let error = Journal::open(&journal_path(&dir)).err().unwrap();
        assert!(matches!(error, JournalError::Format { found: 2, .. }));
    }

    #[test]
    fn targets_read_like_registry_paths() {
        let target = Target::Registry {
            path: r"Software\LimeKit".into(),
            name: "Value".into(),
        };
        assert_eq!(target.to_string(), r"HKCU\Software\LimeKit\Value");
    }

    #[test]
    fn a_system_parameter_is_written_by_name() {
        let target = Target::Parameter {
            name: Parameter::MenuAnimation,
        };
        assert_eq!(
            serde_json::to_string(&target).unwrap(),
            r#"{"kind":"parameter","name":"menu_animation"}"#
        );
    }
}
