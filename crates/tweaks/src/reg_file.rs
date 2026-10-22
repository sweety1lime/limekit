//! The undo as a .reg file: it puts back what the changes replaced and works
//! without LimeKit, by double-clicking it.

use std::fmt::Write;

use crate::engine::{Entry, State, Target};
use crate::value::Value;

const REG_BINARY: u32 = 3;

/// The text of a .reg file that undoes the applied ones among `entries`.
/// Newer changes come first, so of several changes to one value the oldest
/// "before" is written last and wins.
pub fn undo_reg_text(entries: &[Entry]) -> String {
    let mut text = String::from(
        "Windows Registry Editor Version 5.00\r\n\r\n\
         ; Возвращает то, что изменил LimeKit. Дважды щёлкните файл, чтобы применить.\r\n\
         ; Ключи, которые LimeKit создал, останутся, но пустыми.\r\n",
    );
    let mut current_key: Option<&str> = None;
    for entry in entries.iter().rev() {
        if entry.state != State::Applied {
            continue;
        }
        let Target::Registry { path, name } = &entry.target;
        if current_key != Some(path.as_str()) {
            let _ = write!(text, "\r\n[HKEY_CURRENT_USER\\{path}]\r\n");
            current_key = Some(path);
        }
        let name = if name.is_empty() {
            "@".to_owned()
        } else {
            format!("\"{}\"", escape(name))
        };
        let _ = write!(text, "{name}={}\r\n", data(entry.before.value.as_ref()));
    }
    text
}

/// [`undo_reg_text`] encoded the way regedit expects it: UTF-16LE with a BOM.
pub fn undo_reg_file(entries: &[Entry]) -> Vec<u8> {
    [0xfeffu16]
        .into_iter()
        .chain(undo_reg_text(entries).encode_utf16())
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn data(value: Option<&Value>) -> String {
    match value {
        None => "-".to_owned(),
        Some(Value::Sz(text)) if !text.chars().any(char::is_control) => {
            format!("\"{}\"", escape(text))
        }
        Some(Value::Dword(number)) => format!("dword:{number:08x}"),
        Some(value) => {
            let raw = value.to_raw();
            let bytes: Vec<String> = raw.bytes.iter().map(|byte| format!("{byte:02x}")).collect();
            if raw.kind == REG_BINARY {
                format!("hex:{}", bytes.join(","))
            } else {
                format!("hex({:x}):{}", raw.kind, bytes.join(","))
            }
        }
    }
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Before, EntryId, SessionId};

    fn entry(id: u64, path: &str, name: &str, before: Option<Value>, state: State) -> Entry {
        Entry {
            id: EntryId(id),
            session: SessionId(1),
            at: 0,
            tweak: "t".into(),
            target: Target::Registry {
                path: path.into(),
                name: name.into(),
            },
            before: Before {
                value: before,
                created_key: None,
            },
            after: Some(Value::Dword(1)),
            state,
        }
    }

    fn body(entries: &[Entry]) -> Vec<String> {
        undo_reg_text(entries)
            .split("\r\n")
            .skip(4)
            .filter(|line| !line.is_empty())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn restores_a_dword_and_removes_a_value_that_did_not_exist() {
        let entries = [
            entry(
                1,
                r"Software\A",
                "Old",
                Some(Value::Dword(0)),
                State::Applied,
            ),
            entry(2, r"Software\A", "New", None, State::Applied),
        ];
        assert_eq!(
            body(&entries),
            [
                r"[HKEY_CURRENT_USER\Software\A]",
                r#""New"=-"#,
                r#""Old"=dword:00000000"#
            ]
        );
    }

    #[test]
    fn escapes_strings_and_uses_at_for_the_default_value() {
        let entries = [entry(
            1,
            r"Software\A",
            "",
            Some(Value::Sz(r#"C:\a "b""#.into())),
            State::Applied,
        )];
        assert_eq!(
            body(&entries),
            [r"[HKEY_CURRENT_USER\Software\A]", r#"@="C:\\a \"b\"""#]
        );
    }

    #[test]
    fn writes_other_types_as_hex() {
        let entries = [entry(
            1,
            r"Software\A",
            "Path",
            Some(Value::ExpandSz("%X%".into())),
            State::Applied,
        )];
        assert_eq!(
            body(&entries)[1],
            r#""Path"=hex(2):25,00,58,00,25,00,00,00"#
        );
    }

    #[test]
    fn leaves_out_changes_that_are_not_in_effect() {
        let entries = [
            entry(1, r"Software\A", "Undone", None, State::Reverted),
            entry(2, r"Software\A", "Failed", None, State::Failed("x".into())),
        ];
        assert!(body(&entries).is_empty());
    }

    #[test]
    fn lets_the_oldest_before_win_for_a_value_changed_twice() {
        let entries = [
            entry(1, r"Software\A", "V", Some(Value::Dword(0)), State::Applied),
            entry(2, r"Software\A", "V", Some(Value::Dword(1)), State::Applied),
        ];
        assert_eq!(body(&entries).last().unwrap(), r#""V"=dword:00000000"#);
    }

    #[test]
    fn the_file_is_utf16_with_a_bom() {
        assert_eq!(undo_reg_file(&[])[..4], [0xff, 0xfe, b'W', 0]);
    }
}
