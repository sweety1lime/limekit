//! How registry values are written down in the journal and the catalog.
//!
//! A value is stored typed (`{"type":"dword","data":1}`) when that form turns
//! back into exactly the bytes that were read, and as its type number and hex
//! bytes otherwise. The journal stays readable for the common cases, and an
//! undo still writes back precisely what was there.

use platform::registry::RawValue;
use serde::{Deserialize, Serialize};

const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_BINARY: u32 = 3;
const REG_DWORD: u32 = 4;
const REG_MULTI_SZ: u32 = 7;
const REG_QWORD: u32 = 11;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Value {
    Sz(String),
    ExpandSz(String),
    MultiSz(Vec<String>),
    Dword(u32),
    Qword(u64),
    Binary(#[serde(with = "hex")] Vec<u8>),
    /// Any other type, or bytes no typed form reproduces exactly (a string
    /// without its terminating NUL, a DWORD of the wrong length).
    Raw {
        kind: u32,
        #[serde(with = "hex")]
        bytes: Vec<u8>,
    },
}

impl Value {
    /// The bytes and type to write to the registry.
    pub fn to_raw(&self) -> RawValue {
        let (kind, bytes) = match self {
            Value::Sz(text) => (REG_SZ, utf16_z(&[text.as_str()])),
            Value::ExpandSz(text) => (REG_EXPAND_SZ, utf16_z(&[text.as_str()])),
            Value::MultiSz(list) => {
                let strings: Vec<&str> = list.iter().map(String::as_str).collect();
                // Every string ends with a NUL and the list with one more, so
                // an empty list is that NUL alone.
                let mut bytes = utf16_z(&strings);
                bytes.extend_from_slice(&[0, 0]);
                (REG_MULTI_SZ, bytes)
            }
            Value::Dword(number) => (REG_DWORD, number.to_le_bytes().to_vec()),
            Value::Qword(number) => (REG_QWORD, number.to_le_bytes().to_vec()),
            Value::Binary(bytes) => (REG_BINARY, bytes.clone()),
            Value::Raw { kind, bytes } => (*kind, bytes.clone()),
        };
        RawValue { kind, bytes }
    }

    /// The typed form of `raw` when one reproduces it exactly, `Raw` otherwise.
    pub fn from_raw(raw: &RawValue) -> Value {
        let typed = match raw.kind {
            REG_SZ => decode_strings(&raw.bytes).and_then(single).map(Value::Sz),
            REG_EXPAND_SZ => decode_strings(&raw.bytes)
                .and_then(single)
                .map(Value::ExpandSz),
            REG_MULTI_SZ => decode_strings(&raw.bytes).map(|mut list| {
                // A well-formed list ends with an empty string: the extra NUL.
                list.pop();
                Value::MultiSz(list)
            }),
            REG_DWORD => raw
                .bytes
                .as_slice()
                .try_into()
                .ok()
                .map(|bytes| Value::Dword(u32::from_le_bytes(bytes))),
            REG_QWORD => raw
                .bytes
                .as_slice()
                .try_into()
                .ok()
                .map(|bytes| Value::Qword(u64::from_le_bytes(bytes))),
            REG_BINARY => Some(Value::Binary(raw.bytes.clone())),
            _ => None,
        };
        match typed {
            Some(value) if value.to_raw() == *raw => value,
            _ => Value::Raw {
                kind: raw.kind,
                bytes: raw.bytes.clone(),
            },
        }
    }
}

/// UTF-16LE of each string followed by its NUL.
fn utf16_z(strings: &[&str]) -> Vec<u8> {
    strings
        .iter()
        .flat_map(|text| text.encode_utf16().chain([0]))
        .flat_map(u16::to_le_bytes)
        .collect()
}

/// Splits NUL-terminated UTF-16LE strings; `None` for an odd length, a missing
/// final NUL or invalid UTF-16. Exactness is checked by the round trip after.
fn decode_strings(bytes: &[u8]) -> Option<Vec<String>> {
    if !bytes.len().is_multiple_of(2) {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    let (&last, body) = units.split_last()?;
    if last != 0 {
        return None;
    }
    body.split(|&unit| unit == 0)
        .map(|unit| String::from_utf16(unit).ok())
        .collect()
}

fn single(mut list: Vec<String>) -> Option<String> {
    (list.len() == 1).then(|| list.remove(0))
}

mod hex {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        let text: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        serializer.serialize_str(&text)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        if !text.len().is_multiple_of(2) {
            return Err(D::Error::custom("hex with an odd number of digits"));
        }
        (0..text.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&text[at..at + 2], 16).map_err(D::Error::custom))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(kind: u32, bytes: &[u8]) -> RawValue {
        RawValue {
            kind,
            bytes: bytes.to_vec(),
        }
    }

    mod from_raw {
        use super::*;

        #[test]
        fn reads_a_dword_as_a_number() {
            assert_eq!(Value::from_raw(&raw(4, &[1, 0, 0, 0])), Value::Dword(1));
        }

        #[test]
        fn reads_a_terminated_string_as_text() {
            assert_eq!(
                Value::from_raw(&raw(1, &[b'h', 0, b'i', 0, 0, 0])),
                Value::Sz("hi".into())
            );
        }

        #[test]
        fn reads_a_multi_string_as_a_list() {
            let bytes = [b'a', 0, 0, 0, b'b', 0, 0, 0, 0, 0];
            assert_eq!(
                Value::from_raw(&raw(7, &bytes)),
                Value::MultiSz(vec!["a".into(), "b".into()])
            );
        }

        #[test]
        fn keeps_an_unterminated_string_raw() {
            assert_eq!(
                Value::from_raw(&raw(1, &[b'h', 0, b'i', 0])),
                Value::Raw {
                    kind: 1,
                    bytes: vec![b'h', 0, b'i', 0]
                }
            );
        }

        #[test]
        fn keeps_a_short_dword_raw() {
            assert!(matches!(
                Value::from_raw(&raw(4, &[1, 0])),
                Value::Raw { kind: 4, .. }
            ));
        }

        #[test]
        fn keeps_a_string_with_an_inner_nul_raw() {
            let bytes = [b'a', 0, 0, 0, b'b', 0, 0, 0];
            assert!(matches!(
                Value::from_raw(&raw(1, &bytes)),
                Value::Raw { kind: 1, .. }
            ));
        }
    }

    #[test]
    fn every_form_turns_back_into_the_bytes_it_came_from() {
        let samples = [
            raw(1, &[b'h', 0, b'i', 0, 0, 0]),
            raw(1, &[b'h', 0, b'i', 0]),
            raw(2, &[b'%', 0, b'X', 0, b'%', 0, 0, 0]),
            raw(3, &[0, 255, 7]),
            raw(4, &[1, 2, 3, 4]),
            raw(4, &[1, 2]),
            raw(7, &[0, 0]),
            raw(7, &[b'a', 0, 0, 0, 0, 0]),
            raw(11, &[1, 0, 0, 0, 0, 0, 0, 0]),
            raw(0x1234, &[9]),
        ];
        let back: Vec<RawValue> = samples
            .iter()
            .map(|sample| Value::from_raw(sample).to_raw())
            .collect();
        assert_eq!(back, samples);
    }

    #[test]
    fn serializes_to_readable_json() {
        let json = serde_json::to_string(&Value::Dword(1)).unwrap();
        assert_eq!(json, r#"{"type":"dword","data":1}"#);
    }

    #[test]
    fn raw_bytes_survive_json() {
        let value = Value::Raw {
            kind: 1,
            bytes: vec![0xab, 0x01],
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&json).unwrap(), value);
    }
}
