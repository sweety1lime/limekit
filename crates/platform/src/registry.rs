//! Raw access to values under HKEY_CURRENT_USER, the only hive LimeKit writes.
//! The tweak engine records a value before changing it and writes it back on
//! undo, so values travel as their type number and bytes exactly as stored:
//! no decoding that could lose a trailing NUL or an odd byte. A missing value
//! and a missing key both read as `None`, apart from an empty value.

/// A registry value as stored: its type (`REG_DWORD` is 4, `REG_SZ` 1, ...)
/// and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValue {
    pub kind: u32,
    pub bytes: Vec<u8>,
}

const REG_SZ: u32 = 1;
const REG_EXPAND_SZ: u32 = 2;
const REG_MULTI_SZ: u32 = 7;

impl RawValue {
    /// Whether `other` holds the same value. Exact, except that a string type
    /// written without its terminating NUL may read back with one: other
    /// programs do write strings like that, and writing one back does not
    /// reliably keep it unterminated. Windows reads both as the same text.
    pub fn same_as(&self, other: &RawValue) -> bool {
        if self.kind != other.kind {
            return false;
        }
        if !matches!(self.kind, REG_SZ | REG_EXPAND_SZ | REG_MULTI_SZ) {
            return self.bytes == other.bytes;
        }
        let trim = |bytes: &[u8]| {
            let mut end = bytes.len();
            while end >= 2 && bytes[end - 2] == 0 && bytes[end - 1] == 0 {
                end -= 2;
            }
            bytes[..end].to_vec()
        };
        trim(&self.bytes) == trim(&other.bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// Windows refused the change. Some keys are guarded against programs on
    /// purpose, such as the ones User Choice Protection covers.
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("{0}")]
    Other(String),
}

/// Reads value `name` of the key at `path` (relative to HKCU).
pub fn read_value(path: &str, name: &str) -> Result<Option<RawValue>, RegistryError> {
    #[cfg(windows)]
    {
        win32::read_value(path, name)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, name);
        Err(unsupported())
    }
}

/// Writes value `name`, creating the key at `path` when it does not exist.
pub fn write_value(path: &str, name: &str, value: &RawValue) -> Result<(), RegistryError> {
    #[cfg(windows)]
    {
        win32::write_value(path, name, value)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, name, value);
        Err(unsupported())
    }
}

/// Deletes value `name`. A value or key that is already gone is not an error.
pub fn delete_value(path: &str, name: &str) -> Result<(), RegistryError> {
    #[cfg(windows)]
    {
        win32::delete_value(path, name)
    }
    #[cfg(not(windows))]
    {
        let _ = (path, name);
        Err(unsupported())
    }
}

/// Whether the key at `path` exists.
pub fn key_exists(path: &str) -> Result<bool, RegistryError> {
    #[cfg(windows)]
    {
        win32::key_exists(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(unsupported())
    }
}

/// Deletes the key at `path` if it has no values and no subkeys left, and
/// reports whether it did. A key that is already gone counts as removed.
pub fn remove_key_if_empty(path: &str) -> Result<bool, RegistryError> {
    #[cfg(windows)]
    {
        win32::remove_key_if_empty(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(unsupported())
    }
}

#[cfg(not(windows))]
fn unsupported() -> RegistryError {
    RegistryError::Other("the registry exists only on Windows".into())
}

#[cfg(windows)]
mod win32 {
    use super::{RawValue, RegistryError};
    use windows_registry::{CURRENT_USER, Key};

    // HRESULT_FROM_WIN32 of ERROR_FILE_NOT_FOUND and ERROR_ACCESS_DENIED.
    const NOT_FOUND: u32 = 0x8007_0002;
    const ACCESS_DENIED: u32 = 0x8007_0005;

    fn code(error: &windows::core::Error) -> u32 {
        error.code().0 as u32
    }

    fn map(path: &str, error: &windows::core::Error) -> RegistryError {
        if code(error) == ACCESS_DENIED {
            RegistryError::AccessDenied(path.to_owned())
        } else {
            RegistryError::Other(format!(r"HKCU\{path}: {error}"))
        }
    }

    fn open(path: &str, write: bool) -> Result<Option<Key>, RegistryError> {
        let mut options = CURRENT_USER.options();
        options.read();
        if write {
            options.write();
        }
        match options.open(path) {
            Ok(key) => Ok(Some(key)),
            Err(error) if code(&error) == NOT_FOUND => Ok(None),
            Err(error) => Err(map(path, &error)),
        }
    }

    pub fn read_value(path: &str, name: &str) -> Result<Option<RawValue>, RegistryError> {
        use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA};
        use windows::Win32::System::Registry::{HKEY, REG_VALUE_TYPE, RegQueryValueExW};
        use windows::core::HSTRING;
        let Some(key) = open(path, false)? else {
            return Ok(None);
        };
        let name = HSTRING::from(name);
        // Key::get_value sizes its buffer with one query and keeps that size
        // whatever the read itself returns, padding with zeros. Trust only the
        // length the read reports, and ask again if the value grew meanwhile.
        let mut size = 0u32;
        loop {
            let mut kind = REG_VALUE_TYPE::default();
            let mut bytes = vec![0u8; size as usize];
            let mut len = size;
            // SAFETY: `bytes` holds `len` bytes and outlives the call; the key
            // handle is open throughout.
            let status = unsafe {
                RegQueryValueExW(
                    HKEY(key.as_raw()),
                    &name,
                    None,
                    Some(&mut kind),
                    (size > 0).then_some(bytes.as_mut_ptr()),
                    Some(&mut len),
                )
            };
            if status == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if status == ERROR_MORE_DATA || (size == 0 && len > 0 && status.is_ok()) {
                size = len;
                continue;
            }
            status.ok().map_err(|error| map(path, &error))?;
            bytes.truncate(len as usize);
            return Ok(Some(RawValue {
                kind: kind.0,
                bytes,
            }));
        }
    }

    pub fn write_value(path: &str, name: &str, value: &RawValue) -> Result<(), RegistryError> {
        use windows::Win32::System::Registry::{HKEY, REG_VALUE_TYPE, RegSetValueExW};
        use windows::core::HSTRING;
        let key = CURRENT_USER
            .create(path)
            .map_err(|error| map(path, &error))?;
        // Straight to RegSetValueExW: Key::set_bytes asserts that string values
        // end in NUL, and the ones other programs write do not always, yet an
        // undo has to put them back exactly as they were.
        // SAFETY: the key handle is open for the whole call and both buffers
        // outlive it.
        let status = unsafe {
            RegSetValueExW(
                HKEY(key.as_raw()),
                &HSTRING::from(name),
                None,
                REG_VALUE_TYPE(value.kind),
                Some(&value.bytes),
            )
        };
        status.ok().map_err(|error| map(path, &error))
    }

    pub fn delete_value(path: &str, name: &str) -> Result<(), RegistryError> {
        let Some(key) = open(path, true)? else {
            return Ok(());
        };
        match key.remove_value(name) {
            Ok(()) => Ok(()),
            Err(error) if code(&error) == NOT_FOUND => Ok(()),
            Err(error) => Err(map(path, &error)),
        }
    }

    pub fn key_exists(path: &str) -> Result<bool, RegistryError> {
        Ok(open(path, false)?.is_some())
    }

    pub fn remove_key_if_empty(path: &str) -> Result<bool, RegistryError> {
        let Some(key) = open(path, false)? else {
            return Ok(true);
        };
        let has_values = key
            .values()
            .map_err(|error| map(path, &error))?
            .next()
            .is_some();
        let has_keys = key
            .keys()
            .map_err(|error| map(path, &error))?
            .next()
            .is_some();
        if has_values || has_keys {
            return Ok(false);
        }
        drop(key);
        CURRENT_USER
            .remove_tree(path)
            .map_err(|error| map(path, &error))?;
        Ok(true)
    }
}

#[cfg(all(windows, test))]
mod tests {
    use super::*;

    /// A private key under HKCU, deleted on drop, so tests never touch real
    /// settings.
    struct Sandbox(String);

    impl Sandbox {
        fn new(name: &str) -> Self {
            Self(format!(
                r"Software\LimeKitTest-{}-registry-{name}",
                std::process::id()
            ))
        }

        fn path(&self, sub: &str) -> String {
            if sub.is_empty() {
                self.0.clone()
            } else {
                format!(r"{}\{sub}", self.0)
            }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = windows_registry::CURRENT_USER.remove_tree(&self.0);
        }
    }

    fn raw(kind: u32, bytes: &[u8]) -> RawValue {
        RawValue {
            kind,
            bytes: bytes.to_vec(),
        }
    }

    #[test]
    fn values_read_back_with_their_exact_type_and_bytes() {
        let sandbox = Sandbox::new("roundtrip");
        let values = [
            raw(1, &[b'h', 0, b'i', 0, 0, 0]),
            raw(2, &[b'%', 0, b'X', 0, b'%', 0, 0, 0]),
            raw(3, &[0, 1, 2, 255]),
            raw(4, &[1, 2, 3, 4]),
            raw(7, &[b'a', 0, 0, 0, b'b', 0, 0, 0, 0, 0]),
            raw(11, &[1, 0, 0, 0, 0, 0, 0, 0]),
            // A type Windows has no name for still keeps its bytes.
            raw(0x1234, &[9, 8, 7]),
        ];
        for (index, value) in values.iter().enumerate() {
            write_value(&sandbox.path(""), &format!("V{index}"), value).unwrap();
        }

        let read: Vec<_> = (0..values.len())
            .map(|index| read_value(&sandbox.path(""), &format!("V{index}")).unwrap())
            .collect();
        assert_eq!(read, values.map(Some));
    }

    #[test]
    fn a_string_without_its_nul_reads_back_as_the_same_value() {
        // Other programs write REG_SZ values like this. Writing one back does
        // not reliably keep it unterminated, so only the text is promised.
        let sandbox = Sandbox::new("unterminated");
        let unterminated = raw(1, &[b'h', 0, b'i', 0]);
        write_value(&sandbox.path(""), "S", &unterminated).unwrap();

        let read = read_value(&sandbox.path(""), "S").unwrap().unwrap();
        assert!(read.same_as(&unterminated), "read back {read:?}");
    }

    mod same_as {
        use super::*;

        #[test]
        fn ignores_a_missing_terminating_nul_on_strings() {
            assert!(raw(1, &[b'h', 0, b'i', 0]).same_as(&raw(1, &[b'h', 0, b'i', 0, 0, 0])));
        }

        #[test]
        fn compares_other_types_byte_for_byte() {
            assert!(!raw(3, &[1, 0]).same_as(&raw(3, &[1])));
        }

        #[test]
        fn tells_different_types_apart() {
            assert!(!raw(4, &[1, 0, 0, 0]).same_as(&raw(3, &[1, 0, 0, 0])));
        }

        #[test]
        fn tells_different_text_apart() {
            assert!(!raw(1, &[b'h', 0, 0, 0]).same_as(&raw(1, &[b'i', 0, 0, 0])));
        }
    }

    #[test]
    fn a_missing_value_reads_as_none() {
        let sandbox = Sandbox::new("missing-value");
        write_value(&sandbox.path(""), "Other", &raw(4, &[1, 0, 0, 0])).unwrap();

        assert_eq!(read_value(&sandbox.path(""), "Absent").unwrap(), None);
    }

    #[test]
    fn a_missing_key_reads_as_none() {
        let sandbox = Sandbox::new("missing-key");

        assert_eq!(read_value(&sandbox.path("Nowhere"), "Value").unwrap(), None);
    }

    #[test]
    fn an_empty_value_is_not_a_missing_one() {
        let sandbox = Sandbox::new("empty");
        write_value(&sandbox.path(""), "Empty", &raw(3, &[])).unwrap();

        assert_eq!(
            read_value(&sandbox.path(""), "Empty").unwrap(),
            Some(raw(3, &[]))
        );
    }

    #[test]
    fn writing_creates_missing_keys() {
        let sandbox = Sandbox::new("create");
        write_value(&sandbox.path(r"A\B"), "V", &raw(4, &[2, 0, 0, 0])).unwrap();

        assert!(key_exists(&sandbox.path(r"A\B")).unwrap());
    }

    #[test]
    fn deleting_a_value_that_is_not_there_succeeds() {
        let sandbox = Sandbox::new("delete-absent");

        assert!(delete_value(&sandbox.path("Nowhere"), "V").is_ok());
    }

    #[test]
    fn an_empty_key_is_removed() {
        let sandbox = Sandbox::new("remove-empty");
        write_value(&sandbox.path("Leaf"), "V", &raw(4, &[0; 4])).unwrap();
        delete_value(&sandbox.path("Leaf"), "V").unwrap();

        assert!(remove_key_if_empty(&sandbox.path("Leaf")).unwrap());
    }

    #[test]
    fn a_key_with_values_is_kept() {
        let sandbox = Sandbox::new("keep");
        write_value(&sandbox.path("Leaf"), "V", &raw(4, &[0; 4])).unwrap();

        assert!(!remove_key_if_empty(&sandbox.path("Leaf")).unwrap());
    }
}
